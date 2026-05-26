use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::helpers::session_codename;
use crate::parser::{self, Message, SessionMeta};

/// Schema version — bump when the stored message format changes to trigger full re-index.
const SCHEMA_VERSION: &str = "5";

// ---------------------------------------------------------------------------
// Migration snapshot
// ---------------------------------------------------------------------------

/// All columns of a `sessions` row that must survive a schema-migration wipe.
/// This includes user-generated data (notes, is_favourite) and stable identifiers
/// (ref_num, ref_code) that would otherwise be re-assigned with different values.
pub(crate) struct SavedSession {
    pub session_id: String,
    pub file_path: String,
    pub project_dir: String,
    pub cwd: String,
    pub started_at: String,
    pub ended_at: String,
    pub msg_count: i64,
    pub first_user_text: String,
    pub is_resumed: i64,
    pub is_automated: i64,
    pub ref_num: Option<i64>,
    pub ref_code: Option<String>,
    pub notes: Option<String>,
    pub is_favourite: i64,
}

/// Capture every session row before a migration wipe so we can restore stubs
/// for sessions whose source JSONL files have been pruned by Claude.
pub(crate) fn snapshot_sessions(conn: &Connection) -> Vec<SavedSession> {
    let mut stmt = match conn.prepare(
        "SELECT session_id, file_path, project_dir, cwd, started_at, ended_at,
                msg_count, first_user_text, is_resumed, is_automated,
                ref_num, ref_code, notes, is_favourite
         FROM sessions",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |r| {
        Ok(SavedSession {
            session_id:      r.get(0)?,
            file_path:       r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            project_dir:     r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            cwd:             r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            started_at:      r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            ended_at:        r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            msg_count:       r.get::<_, Option<i64>>(6)?.unwrap_or(0),
            first_user_text: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            is_resumed:      r.get::<_, Option<i64>>(8)?.unwrap_or(0),
            is_automated:    r.get::<_, Option<i64>>(9)?.unwrap_or(0),
            ref_num:         r.get(10)?,
            ref_code:        r.get(11)?,
            notes:           r.get(12)?,
            is_favourite:    r.get::<_, Option<i64>>(13)?.unwrap_or(0),
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub fn build_or_refresh_index(db_path: &Path, home_dir: &Path) {
    println!("Indexing conversations...");
    let projects_dir = home_dir.join(".claude").join("projects");
    if !projects_dir.exists() { return; }

    let mut conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => { eprintln!("DB open error: {e}"); return; }
    };
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");

    // -----------------------------------------------------------------------
    // Schema migration — wipes and re-indexes, but preserves every session.
    // -----------------------------------------------------------------------
    let stored_version: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
        .ok();

    if stored_version.as_deref() != Some(SCHEMA_VERSION) {
        println!("Schema changed → full re-index");

        // 1. Snapshot before wiping so no data is lost.
        let saved = snapshot_sessions(&conn);
        let saved_count = saved.len();

        // 2. Find the highest existing ref_num so new assignments don't collide
        //    with the ones we're about to restore.
        let max_existing_ref_num: i64 = saved.iter()
            .filter_map(|s| s.ref_num)
            .max()
            .unwrap_or(0);
        let mut next_ref_num = max_existing_ref_num + 1;

        if let Ok(tx) = conn.transaction() {
            // Wipe content tables.  FTS5 content tables must be rebuilt from
            // the (now-empty) messages table rather than deleted directly.
            let _ = tx.execute_batch(
                "DELETE FROM files; DELETE FROM messages; DELETE FROM sessions;",
            );
            let _ = tx.execute("INSERT INTO msg_fts(msg_fts) VALUES('rebuild')", []);

            // 3. Pre-insert every snapshotted session as a stub row.
            //    index_full (below) will overwrite rows whose source files
            //    still exist, preserving ref_num / notes / is_favourite.
            //    Rows whose files are gone are never touched by the walk and
            //    so survive as permanent stubs — the DB is the archive.
            for s in &saved {
                let ref_code = s.ref_code.clone()
                    .unwrap_or_else(|| session_codename(&s.session_id));
                let ref_num = s.ref_num.unwrap_or_else(|| {
                    let n = next_ref_num;
                    next_ref_num += 1;
                    n
                });
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO sessions
                     (session_id, file_path, project_dir, cwd, started_at, ended_at,
                      msg_count, first_user_text, is_resumed, is_automated,
                      ref_num, ref_code, notes, is_favourite)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                    rusqlite::params![
                        s.session_id, s.file_path, s.project_dir, s.cwd,
                        s.started_at, s.ended_at, s.msg_count, s.first_user_text,
                        s.is_resumed, s.is_automated,
                        ref_num, ref_code, s.notes, s.is_favourite,
                    ],
                );
            }

            let _ = tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_scan_at', '0')",
                [],
            );
            let _ = tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                [SCHEMA_VERSION],
            );
            let _ = tx.commit();
        }

        if saved_count > 0 {
            println!(
                "Preserved {} session stubs across schema migration; \
                 re-indexing from source files now.",
                saved_count
            );
        }
    }

    // -----------------------------------------------------------------------
    // One-time backfills for features added after initial release.
    // -----------------------------------------------------------------------
    let _ = conn.execute_batch("
        WITH numbered AS (
            SELECT session_id,
                   ROW_NUMBER() OVER (ORDER BY COALESCE(started_at,''), session_id) AS rn
            FROM sessions WHERE ref_num IS NULL
        )
        UPDATE sessions SET ref_num = (
            SELECT rn FROM numbered WHERE numbered.session_id = sessions.session_id
        ) WHERE ref_num IS NULL;
    ");
    {
        let sids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT session_id FROM sessions WHERE ref_code IS NULL",
            ).unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        for sid in sids {
            let code = session_codename(&sid);
            let _ = conn.execute(
                "UPDATE sessions SET ref_code=?1 WHERE session_id=?2",
                rusqlite::params![code, sid],
            );
        }
    }

    // -----------------------------------------------------------------------
    // Incremental indexing.
    // -----------------------------------------------------------------------

    // Build known-files map: path -> (mtime, size, session_id)
    let known: HashMap<String, (f64, i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT path, mtime, size, session_id FROM files",
        ).unwrap();
        stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, f64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        )))
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|(path, mtime, size, sid)| (path, (mtime, size, sid)))
        .collect()
    };

    let last_scan_at: f64 = conn
        .query_row("SELECT value FROM meta WHERE key='last_scan_at'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let mut jsonl_paths: Vec<PathBuf> = WalkDir::new(&projects_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().and_then(|x| x.to_str()) == Some("jsonl")
                && !p.components().any(|c| c.as_os_str() == "subagents")
        })
        .map(|e| e.path().to_path_buf())
        .collect();
    jsonl_paths.sort();

    let mut count_new = 0usize;

    for path in &jsonl_paths {
        let stat = match std::fs::metadata(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mtime = stat.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let size = stat.len() as i64;
        let path_str = path.to_string_lossy().into_owned();

        if mtime < last_scan_at {
            continue;
        }

        if let Some((cached_mtime, cached_size, cached_sid)) = known.get(&path_str) {
            if *cached_mtime == mtime && *cached_size == size {
                continue;
            }
            // Incremental: file grew
            if size > *cached_size {
                if index_incremental(&mut conn, path, *cached_size, mtime, size, cached_sid)
                    .is_ok()
                {
                    count_new += 1;
                    continue;
                }
            }
        }

        // Full re-index for this file.
        if let Some((meta, messages)) = parser::parse_session(path) {
            if index_full(&mut conn, &meta, &messages, mtime, size).is_ok() {
                count_new += 1;
            }
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let _ = conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_scan_at', ?1)",
        [now.to_string()],
    );
    println!("Indexing done. {} updated.", count_new);
}

// ---------------------------------------------------------------------------
// index_full
// ---------------------------------------------------------------------------

fn index_full(
    conn: &mut Connection,
    meta: &SessionMeta,
    messages: &[Message],
    mtime: f64,
    size: i64,
) -> rusqlite::Result<()> {
    let sid = &meta.session_id;
    let ref_code = session_codename(sid);

    // Preserve user-generated data (ref_num, notes, is_favourite) from any
    // existing row — whether it's a live row from incremental indexing or a
    // stub pre-inserted by the migration path above.
    let existing: Option<(Option<i64>, Option<String>, i64)> = conn.query_row(
        "SELECT ref_num, notes, is_favourite FROM sessions WHERE session_id=?1",
        [sid],
        |r| Ok((
            r.get::<_, Option<i64>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        )),
    ).ok();
    let existing_ref_num   = existing.as_ref().and_then(|(n, _, _)| *n);
    let existing_notes     = existing.as_ref().and_then(|(_, notes, _)| notes.clone());
    let existing_is_fav    = existing.as_ref().map(|(_, _, f)| *f).unwrap_or(0);

    let tx = conn.transaction()?;

    let ref_num: i64 = match existing_ref_num {
        Some(n) => n,
        None => tx.query_row(
            "SELECT COALESCE(MAX(ref_num), 0) + 1 FROM sessions",
            [],
            |r| r.get(0),
        ).unwrap_or(1),
    };

    // Remove old data (FTS rows must be deleted before messages rows).
    let old_ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM messages WHERE session_id=?1")?;
        stmt.query_map([sid], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect()
    };
    for id in &old_ids {
        tx.execute("DELETE FROM msg_fts WHERE rowid=?1", [id])?;
    }
    tx.execute("DELETE FROM messages WHERE session_id=?1", [sid])?;
    tx.execute("DELETE FROM sessions  WHERE session_id=?1", [sid])?;
    tx.execute("DELETE FROM files     WHERE path=?1", [&meta.file_path])?;

    tx.execute(
        "INSERT INTO sessions
         (session_id, file_path, project_dir, cwd, started_at, ended_at,
          msg_count, first_user_text, is_resumed, is_automated,
          ref_num, ref_code, notes, is_favourite)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        rusqlite::params![
            sid, meta.file_path, meta.project_dir, meta.cwd,
            meta.started_at, meta.ended_at, meta.msg_count,
            meta.first_user_text, meta.is_resumed, meta.is_automated,
            ref_num, ref_code, existing_notes, existing_is_fav,
        ],
    )?;

    for m in messages {
        let row_id: i64 = tx.query_row(
            "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?1,?2,?3,?4,?5) RETURNING id",
            rusqlite::params![sid, m.seq, m.role, m.ts, m.text],
            |r| r.get(0),
        )?;
        let fts = m.fts_text.as_deref().unwrap_or(&m.text);
        tx.execute(
            "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?1,?2,?3,?4)",
            rusqlite::params![row_id, fts, sid, m.seq],
        )?;
    }

    tx.execute(
        "INSERT INTO files (path,mtime,size,session_id) VALUES (?1,?2,?3,?4)",
        rusqlite::params![meta.file_path, mtime, size, sid],
    )?;
    tx.commit()
}

// ---------------------------------------------------------------------------
// index_incremental
// ---------------------------------------------------------------------------

fn index_incremental(
    conn: &mut Connection,
    path: &Path,
    cached_size: i64,
    new_mtime: f64,
    new_size: i64,
    sid: &str,
) -> rusqlite::Result<()> {
    use std::io::{BufReader, Read, Seek, SeekFrom};
    use std::fs::File;

    let io_err = |e: std::io::Error| rusqlite::Error::InvalidParameterName(e.to_string());

    let last_row: Option<(i64, String)> = conn.query_row(
        "SELECT seq, ts FROM messages WHERE session_id=?1 ORDER BY seq DESC LIMIT 1",
        [sid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).ok();
    let mut seq = last_row.as_ref().map(|(s, _)| s + 1).unwrap_or(0);
    let last_ts = last_row.map(|(_, ts)| ts);

    let file = File::open(path).map_err(io_err)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(cached_size as u64)).map_err(io_err)?;

    let mut remainder = String::new();
    reader.read_to_string(&mut remainder).map_err(io_err)?;

    let mut new_msgs: Vec<Message> = Vec::new();
    for line in remainder.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let obj: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let (msgs, _) = parser::parse_jsonl_obj(&obj, &mut seq);
        new_msgs.extend(msgs);
    }

    let tx = conn.transaction()?;
    for m in &new_msgs {
        let row_id: i64 = tx.query_row(
            "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?1,?2,?3,?4,?5) RETURNING id",
            rusqlite::params![sid, m.seq, m.role, m.ts, m.text],
            |r| r.get(0),
        )?;
        let fts = m.fts_text.as_deref().unwrap_or(&m.text);
        tx.execute(
            "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?1,?2,?3,?4)",
            rusqlite::params![row_id, fts, sid, m.seq],
        )?;
    }

    if !new_msgs.is_empty() {
        let new_ended = new_msgs.last().map(|m| m.ts.as_str()).unwrap_or("");
        let new_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id=?1 AND (role='user' OR role='assistant')",
            [sid],
            |r| r.get(0),
        ).unwrap_or(0);
        let mut is_resumed: i64 = tx.query_row(
            "SELECT is_resumed FROM sessions WHERE session_id=?1",
            [sid],
            |r| r.get(0),
        ).unwrap_or(0);
        if is_resumed == 0 {
            if let (Some(lt), Some(nt)) = (&last_ts, new_msgs.first()) {
                if let (Some(a), Some(b)) = (parser::parse_ts(lt), parser::parse_ts(&nt.ts)) {
                    if (b - a).num_seconds() > 7200 { is_resumed = 1; }
                }
            }
        }
        tx.execute(
            "UPDATE sessions SET ended_at=?1, msg_count=?2, is_resumed=?3 WHERE session_id=?4",
            rusqlite::params![new_ended, new_count, is_resumed, sid],
        )?;
    }
    tx.execute(
        "UPDATE files SET mtime=?1, size=?2 WHERE path=?3",
        rusqlite::params![new_mtime, new_size, path.to_string_lossy().as_ref()],
    )?;
    tx.commit()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Write a minimal two-turn JSONL session file (one user turn, one assistant).
    fn write_session(dir: &Path, sid: &str, user_msg: &str, ts: &str) {
        let content = format!(
            "{}\n{}\n",
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": user_msg },
                "timestamp": ts
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "role": "assistant", "content": "Understood." },
                "timestamp": ts
            })
        );
        fs::write(dir.join(format!("{}.jsonl", sid)), content).unwrap();
    }

    /// Simulate a schema-version mismatch so the next build_or_refresh_index
    /// triggers the full migration path.
    fn force_schema_mismatch(db_path: &Path) {
        let conn = crate::db::open(db_path).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '99')",
            [],
        ).unwrap();
    }

    // -----------------------------------------------------------------------
    // Core regression: sessions whose JSONL files were pruned by Claude must
    // not disappear when a schema migration forces a full DB wipe + re-index.
    // -----------------------------------------------------------------------
    #[test]
    fn schema_migration_preserves_orphaned_sessions() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("myproject");
        fs::create_dir_all(&proj).unwrap();

        let sid_gone = "aaaa1111bbbb2222cccc3333dddd4444"; // will be deleted (Claude prune)
        let sid_live = "1111aaaa2222bbbb3333cccc4444dddd"; // stays

        write_session(&proj, sid_gone, "old question", "2026-01-01T10:00:00Z");
        write_session(&proj, sid_live, "recent question", "2026-04-01T10:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        // Verify both sessions indexed
        {
            let conn = crate::db::open(&db_path).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 2, "both sessions must be indexed initially");
        }

        // Claude prunes the old file
        fs::remove_file(proj.join(format!("{}.jsonl", sid_gone))).unwrap();

        // Schema bump triggers migration on next launch
        force_schema_mismatch(&db_path);
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2, "orphaned session must survive schema migration");

        let gone_present: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_gone],
                |r| r.get::<_, i64>(0),
            )
            .unwrap() > 0;
        assert!(gone_present, "pruned session must be preserved as a stub");

        let live_present: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_live],
                |r| r.get::<_, i64>(0),
            )
            .unwrap() > 0;
        assert!(live_present, "active session must still be in DB after re-index");
    }

    // -----------------------------------------------------------------------
    // User data (notes, is_favourite) must survive migration for both orphaned
    // sessions (file gone) and sessions that are re-indexed from their files.
    // -----------------------------------------------------------------------
    #[test]
    fn schema_migration_preserves_user_data() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj2");
        fs::create_dir_all(&proj).unwrap();

        let sid_gone = "bbbb2222cccc3333dddd4444eeee5555";
        let sid_live = "2222bbbb3333cccc4444dddd5555eeee";

        write_session(&proj, sid_gone, "question A", "2026-02-01T10:00:00Z");
        write_session(&proj, sid_live, "question B", "2026-05-01T10:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        // Add user data before migration
        {
            let conn = crate::db::open(&db_path).unwrap();
            conn.execute(
                "UPDATE sessions SET notes='orphan note', is_favourite=1 WHERE session_id=?1",
                rusqlite::params![sid_gone],
            ).unwrap();
            conn.execute(
                "UPDATE sessions SET notes='live note', is_favourite=1 WHERE session_id=?1",
                rusqlite::params![sid_live],
            ).unwrap();
        }

        // File pruned + schema change
        fs::remove_file(proj.join(format!("{}.jsonl", sid_gone))).unwrap();
        force_schema_mismatch(&db_path);
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();

        // Orphaned session: notes and favourite intact
        let (gone_notes, gone_fav): (String, i64) = conn
            .query_row(
                "SELECT COALESCE(notes,''), is_favourite FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_gone],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("orphaned session must exist");
        assert_eq!(gone_notes, "orphan note", "orphaned session note must be preserved");
        assert_eq!(gone_fav, 1, "orphaned session favourite must be preserved");

        // Live session: re-indexed from file, but user data carried over
        let (live_notes, live_fav): (String, i64) = conn
            .query_row(
                "SELECT COALESCE(notes,''), is_favourite FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_live],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("live session must exist");
        assert_eq!(live_notes, "live note", "live session note must survive re-index");
        assert_eq!(live_fav, 1, "live session favourite must survive re-index");
    }

    // -----------------------------------------------------------------------
    // ref_num must be stable across migrations — sessions keep their old
    // numbers so external references (e.g. "#42") remain valid.
    // -----------------------------------------------------------------------
    #[test]
    fn schema_migration_preserves_ref_nums() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj3");
        fs::create_dir_all(&proj).unwrap();

        let sid_a = "cccc3333dddd4444eeee5555ffff6666";
        let sid_b = "3333cccc4444dddd5555eeee6666ffff";

        write_session(&proj, sid_a, "session alpha", "2026-03-01T09:00:00Z");
        write_session(&proj, sid_b, "session beta",  "2026-03-02T09:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        // Capture ref_nums assigned in first index
        let (ref_a_before, ref_b_before): (i64, i64) = {
            let conn = crate::db::open(&db_path).unwrap();
            let ra = conn.query_row(
                "SELECT ref_num FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_a],
                |r| r.get(0),
            ).unwrap();
            let rb = conn.query_row(
                "SELECT ref_num FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_b],
                |r| r.get(0),
            ).unwrap();
            (ra, rb)
        };

        force_schema_mismatch(&db_path);
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let ref_a_after: i64 = conn.query_row(
            "SELECT ref_num FROM sessions WHERE session_id=?1",
            rusqlite::params![sid_a],
            |r| r.get(0),
        ).unwrap();
        let ref_b_after: i64 = conn.query_row(
            "SELECT ref_num FROM sessions WHERE session_id=?1",
            rusqlite::params![sid_b],
            |r| r.get(0),
        ).unwrap();

        assert_eq!(ref_a_before, ref_a_after, "ref_num for session A must be stable");
        assert_eq!(ref_b_before, ref_b_after, "ref_num for session B must be stable");
    }
}
