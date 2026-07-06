use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use walkdir::WalkDir;

use crate::helpers::session_codename;
use crate::parser::{self, Message, SessionMeta};

/// Schema version — bump when the stored message format changes to trigger full re-index.
const SCHEMA_VERSION: &str = "6";

/// Startup, the 120s timer, the file watcher, and POST /refresh can all call
/// build_or_refresh_index from different threads. Serialize them: overlapping
/// runs on separate connections would hit SQLITE_BUSY mid-index.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

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
    pub is_subagent: i64,
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
                msg_count, first_user_text, is_resumed, is_automated, is_subagent,
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
            is_subagent:     r.get::<_, Option<i64>>(10)?.unwrap_or(0),
            ref_num:         r.get(11)?,
            ref_code:        r.get(12)?,
            notes:           r.get(13)?,
            is_favourite:    r.get::<_, Option<i64>>(14)?.unwrap_or(0),
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
    let _guard = INDEX_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    println!("Indexing conversations...");
    let projects_dir = home_dir.join(".claude").join("projects");
    if !projects_dir.exists() { return; }

    let mut conn = match crate::db::open(db_path) {
        Ok(c) => c,
        Err(e) => { eprintln!("DB open error: {e}"); return; }
    };

    // -----------------------------------------------------------------------
    // Schema migration — wipes and re-indexes, but preserves every session,
    // and preserves full message content for sessions whose source JSONL has
    // been pruned by Claude (the DB is their only remaining copy).
    // -----------------------------------------------------------------------
    let stored_version: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
        .ok();

    if stored_version.as_deref() != Some(SCHEMA_VERSION) {
        println!("Schema changed → full re-index");

        // 1. Snapshot before wiping so no data is lost.
        let saved = snapshot_sessions(&conn);
        let saved_count = saved.len();

        // 2. Sessions whose source file still exists get their messages wiped
        //    and re-parsed below. Orphans (file pruned) keep their messages.
        let live_sids: Vec<String> = {
            let mut stmt = match conn.prepare("SELECT session_id, path FROM files") {
                Ok(s) => s,
                Err(_) => { eprintln!("files table read failed"); return; }
            };
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map(|rows| {
                    rows.filter_map(|r| r.ok())
                        .filter(|(_, path)| Path::new(path).exists())
                        .map(|(sid, _)| sid)
                        .collect()
                })
                .unwrap_or_default()
        };

        // 3. Find the highest existing ref_num so new assignments don't collide
        //    with the ones we're about to restore.
        let max_existing_ref_num: i64 = saved.iter()
            .filter_map(|s| s.ref_num)
            .max()
            .unwrap_or(0);
        let mut next_ref_num = max_existing_ref_num + 1;

        if let Ok(tx) = conn.transaction() {
            for sid in &live_sids {
                let _ = tx.execute("DELETE FROM messages WHERE session_id=?1", [sid]);
            }
            let _ = tx.execute_batch("DELETE FROM files; DELETE FROM sessions;");
            // FTS index must be rebuilt from the (partially wiped) content table.
            let _ = tx.execute("INSERT INTO msg_fts(msg_fts) VALUES('rebuild')", []);

            // 4. Pre-insert every snapshotted session as a stub row.
            //    index_full (below) will overwrite rows whose source files
            //    still exist, preserving ref_num / notes / is_favourite.
            //    Rows whose files are gone are never touched by the walk and
            //    so survive as permanent stubs — with their messages intact.
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
                      msg_count, first_user_text, is_resumed, is_automated, is_subagent,
                      ref_num, ref_code, notes, is_favourite)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                    rusqlite::params![
                        s.session_id, s.file_path, s.project_dir, s.cwd,
                        s.started_at, s.ended_at, s.msg_count, s.first_user_text,
                        s.is_resumed, s.is_automated, s.is_subagent,
                        ref_num, ref_code, s.notes, s.is_favourite,
                    ],
                );
            }

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

    let mut jsonl_paths: Vec<PathBuf> = WalkDir::new(&projects_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
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
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let size = stat.len() as i64;
        let path_str = path.to_string_lossy().into_owned();

        if let Some((cached_mtime, cached_size, cached_sid)) = known.get(&path_str) {
            if *cached_mtime == mtime && *cached_size == size {
                continue;
            }
            // Incremental: file grew
            if size > *cached_size
                && index_incremental(&mut conn, path, *cached_size, mtime, size, cached_sid)
                    .is_ok()
                {
                    count_new += 1;
                    continue;
                }
        }

        // Full re-index for this file.
        if let Some((meta, messages)) = parser::parse_session(path) {
            if index_full(&mut conn, &meta, &messages, mtime, size).is_ok() {
                count_new += 1;
            }
        }
    }

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
        let ids: Vec<i64> = stmt.query_map([sid], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        ids
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
          msg_count, first_user_text, is_resumed, is_automated, is_subagent,
          ref_num, ref_code, notes, is_favourite)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        rusqlite::params![
            sid, meta.file_path, meta.project_dir, meta.cwd,
            meta.started_at, meta.ended_at, meta.msg_count,
            meta.first_user_text, meta.is_resumed, meta.is_automated, meta.is_subagent,
            ref_num, ref_code, existing_notes, existing_is_fav,
        ],
    )?;

    for m in messages {
        insert_message(&tx, sid, m)?;
    }

    tx.execute(
        "INSERT INTO files (path,mtime,size,session_id) VALUES (?1,?2,?3,?4)",
        rusqlite::params![meta.file_path, mtime, size, sid],
    )?;
    tx.commit()
}

/// Insert one message row plus its FTS entry. search_text is stored on the
/// messages row itself because msg_fts is an external-content table — the
/// indexed value and the content-table value must be identical or deletes
/// corrupt the FTS index.
fn insert_message(tx: &rusqlite::Transaction<'_>, sid: &str, m: &Message) -> rusqlite::Result<()> {
    let search_text = m.fts_text.as_deref().unwrap_or(&m.text);
    let row_id: i64 = tx.query_row(
        "INSERT INTO messages (session_id,seq,role,ts,text,search_text)
         VALUES (?1,?2,?3,?4,?5,?6) RETURNING id",
        rusqlite::params![sid, m.seq, m.role, m.ts, m.text, search_text],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO msg_fts(rowid,search_text,session_id,seq) VALUES (?1,?2,?3,?4)",
        rusqlite::params![row_id, search_text, sid, m.seq],
    )?;
    Ok(())
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
        insert_message(&tx, sid, m)?;
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

    /// Write a session containing a tool_use block (indexed via fts_text,
    /// which differs from the stored text — the FTS-corruption trigger).
    fn write_tool_session(dir: &Path, sid: &str, file_arg: &str, ts: &str) {
        let content = format!(
            "{}\n{}\n",
            serde_json::json!({
                "type": "user",
                "message": { "role": "user", "content": "edit something" },
                "timestamp": ts
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "role": "assistant", "content": [
                    { "type": "text", "text": "Editing now." },
                    { "type": "tool_use", "name": "Edit", "id": "t1",
                      "input": { "file_path": file_arg, "old_string": "aaa", "new_string": "bbb" } }
                ]},
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

    fn fts_integrity_ok(conn: &Connection) -> bool {
        conn.execute("INSERT INTO msg_fts(msg_fts, rank) VALUES('integrity-check', 1)", [])
            .is_ok()
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
    // Orphaned sessions must keep their MESSAGE CONTENT across migrations,
    // not just their metadata stub — the DB is their only remaining copy.
    // -----------------------------------------------------------------------
    #[test]
    fn schema_migration_preserves_orphaned_messages() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj4");
        fs::create_dir_all(&proj).unwrap();

        let sid_gone = "dddd4444eeee5555ffff6666aaaa7777";
        write_session(&proj, sid_gone, "the unforgettable question", "2026-01-01T10:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        fs::remove_file(proj.join(format!("{}.jsonl", sid_gone))).unwrap();
        force_schema_mismatch(&db_path);
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id=?1",
                rusqlite::params![sid_gone],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(msg_count, 2, "orphaned session must keep its messages across migration");

        // And they must still be searchable after the FTS rebuild.
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM msg_fts WHERE msg_fts MATCH 'unforgettable'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(hits > 0, "orphaned messages must remain searchable");
        assert!(fts_integrity_ok(&conn), "FTS index must pass integrity-check");
    }

    // -----------------------------------------------------------------------
    // Regression for FTS corruption: re-indexing a session whose file shrank
    // (full re-parse path) must leave the FTS index consistent — the old
    // design indexed tool_use rows with text that differed from the content
    // table, so deletes removed the wrong tokens and corrupted the index.
    // -----------------------------------------------------------------------
    #[test]
    fn reindex_after_shrink_keeps_fts_consistent() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj5");
        fs::create_dir_all(&proj).unwrap();

        let sid = "eeee5555ffff6666aaaa7777bbbb8888";
        write_tool_session(&proj, sid, "/very/unique/zebra_path.rs", "2026-03-01T10:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        {
            let conn = crate::db::open(&db_path).unwrap();
            let hits: i64 = conn
                .query_row("SELECT COUNT(*) FROM msg_fts WHERE msg_fts MATCH 'zebra_path'", [], |r| r.get(0))
                .unwrap();
            assert!(hits > 0, "tool_use fts text must be searchable");
        }

        // Rewrite the file SMALLER with different content → full re-index path.
        write_session(&proj, sid, "tiny", "2026-03-01T11:00:00Z");
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let stale: i64 = conn
            .query_row("SELECT COUNT(*) FROM msg_fts WHERE msg_fts MATCH 'zebra_path'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stale, 0, "old tool_use tokens must be gone after re-index");
        assert!(fts_integrity_ok(&conn), "FTS index must pass integrity-check after re-index");
    }

    // -----------------------------------------------------------------------
    // A file that appears with an mtime in the past (backup restore, cp -p)
    // must still be indexed — the old last_scan_at short-circuit skipped it.
    // -----------------------------------------------------------------------
    #[test]
    fn old_mtime_file_gets_indexed() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj6");
        fs::create_dir_all(&proj).unwrap();

        let sid_first = "ffff6666aaaa7777bbbb8888cccc9999";
        write_session(&proj, sid_first, "first session", "2026-01-01T10:00:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        // A restored file appears with an OLD mtime (a year ago).
        let sid_restored = "6666ffff7777aaaa8888bbbb9999cccc";
        write_session(&proj, sid_restored, "restored session", "2025-01-01T10:00:00Z");
        let restored_path = proj.join(format!("{}.jsonl", sid_restored));
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(365 * 86400);
        fs::File::options().write(true).open(&restored_path).unwrap()
            .set_modified(old_time).unwrap();

        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let present: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_restored],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "file with old mtime must be indexed");
    }

    // -----------------------------------------------------------------------
    // Subagent transcripts are indexed and flagged.
    // -----------------------------------------------------------------------
    #[test]
    fn subagent_sessions_indexed_and_flagged() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let home = tmp.path().to_path_buf();

        let proj = home.join(".claude").join("projects").join("proj7");
        let sub = proj.join("some-session").join("subagents");
        fs::create_dir_all(&sub).unwrap();

        let sid_main = "7777bbbb8888cccc9999dddd0000eeee";
        let sid_sub  = "bbbb7777cccc8888dddd9999eeee0000";
        write_session(&proj, sid_main, "main session", "2026-05-01T10:00:00Z");
        write_session(&sub, sid_sub, "subagent task", "2026-05-01T10:05:00Z");

        crate::db::init_db(&db_path).unwrap();
        build_or_refresh_index(&db_path, &home);

        let conn = crate::db::open(&db_path).unwrap();
        let sub_flag: i64 = conn
            .query_row(
                "SELECT is_subagent FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_sub],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sub_flag, 1, "subagent session must be flagged");

        let main_flag: i64 = conn
            .query_row(
                "SELECT is_subagent FROM sessions WHERE session_id=?1",
                rusqlite::params![sid_main],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(main_flag, 0, "main session must not be flagged");
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
