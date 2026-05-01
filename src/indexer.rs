use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::helpers::session_codename;
use crate::parser::{self, Message, SessionMeta};

pub fn build_or_refresh_index(db_path: &Path, home_dir: &Path) {
    println!("Indexing conversations...");
    let projects_dir = home_dir.join(".claude").join("projects");
    if !projects_dir.exists() { return; }

    let mut conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => { eprintln!("DB open error: {e}"); return; }
    };
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");

    let last_scan_at: f64 = conn
        .query_row("SELECT value FROM meta WHERE key='last_scan_at'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    // One-time backfill: assign ref_num to sessions that predate this feature
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
    // One-time backfill: assign ref_code for sessions that predate this feature
    {
        let sids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT session_id FROM sessions WHERE ref_code IS NULL"
            ).unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap()
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

    // Build known-files map: path -> (mtime, size, session_id)
    let known: HashMap<String, (f64, i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT path, mtime, size, session_id FROM files"
        ).unwrap();
        stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, f64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))).unwrap()
        .filter_map(|r| r.ok())
        .map(|(path, mtime, size, sid)| (path, (mtime, size, sid)))
        .collect()
    };

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
                if index_incremental(&mut conn, path, *cached_size, mtime, size, cached_sid).is_ok() {
                    count_new += 1;
                    continue;
                }
            }
        }

        // Full re-index
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

fn index_full(
    conn: &mut Connection,
    meta: &SessionMeta,
    messages: &[Message],
    mtime: f64,
    size: i64,
) -> rusqlite::Result<()> {
    let sid = &meta.session_id;
    let ref_code = session_codename(sid);

    // Preserve ref_num if this session was previously indexed
    let existing_ref_num: Option<i64> = conn.query_row(
        "SELECT ref_num FROM sessions WHERE session_id=?1",
        [sid],
        |r| r.get::<_, Option<i64>>(0),
    ).ok().flatten();

    let tx = conn.transaction()?;

    let ref_num: i64 = match existing_ref_num {
        Some(n) => n,
        None => tx.query_row(
            "SELECT COALESCE(MAX(ref_num), 0) + 1 FROM sessions",
            [],
            |r| r.get(0),
        ).unwrap_or(1),
    };

    // Remove old data
    let old_ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM messages WHERE session_id=?1")?;
        let ids: Vec<i64> = stmt.query_map([sid], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        ids
    };
    if !old_ids.is_empty() {
        for id in &old_ids {
            tx.execute("DELETE FROM msg_fts WHERE rowid=?1", [id])?;
        }
    }
    tx.execute("DELETE FROM messages WHERE session_id=?1", [sid])?;
    tx.execute("DELETE FROM sessions  WHERE session_id=?1", [sid])?;
    tx.execute("DELETE FROM files     WHERE path=?1", [&meta.file_path])?;

    tx.execute(
        "INSERT INTO sessions
         (session_id,file_path,project_dir,cwd,started_at,ended_at,msg_count,first_user_text,is_resumed,is_automated,ref_num,ref_code)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        rusqlite::params![
            sid, meta.file_path, meta.project_dir, meta.cwd,
            meta.started_at, meta.ended_at, meta.msg_count,
            meta.first_user_text, meta.is_resumed, meta.is_automated,
            ref_num, ref_code,
        ],
    )?;

    for m in messages {
        let row_id: i64 = tx.query_row(
            "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?1,?2,?3,?4,?5) RETURNING id",
            rusqlite::params![sid, m.seq, m.role, m.ts, m.text],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?1,?2,?3,?4)",
            rusqlite::params![row_id, m.text, sid, m.seq],
        )?;
    }

    tx.execute(
        "INSERT INTO files (path,mtime,size,session_id) VALUES (?1,?2,?3,?4)",
        rusqlite::params![meta.file_path, mtime, size, sid],
    )?;
    tx.commit()
}

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

    let mut new_msgs: Vec<parser::Message> = Vec::new();
    for line in remainder.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let obj: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let t = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) if t == "user" || t == "assistant" => t.to_string(),
            _ => continue,
        };
        let content_val = obj.get("message").and_then(|m| m.get("content"))
            .cloned().unwrap_or(serde_json::Value::Null);
        if t == "user" && content_val.is_array() { continue; }
        let text = parser::extract_text(&content_val);
        if text.is_empty() { continue; }
        let ts = obj.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();
        new_msgs.push(parser::Message { seq, role: t, ts, text });
        seq += 1;
    }

    let tx = conn.transaction()?;
    for m in &new_msgs {
        let row_id: i64 = tx.query_row(
            "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?1,?2,?3,?4,?5) RETURNING id",
            rusqlite::params![sid, m.seq, m.role, m.ts, m.text],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?1,?2,?3,?4)",
            rusqlite::params![row_id, m.text, sid, m.seq],
        )?;
    }

    if !new_msgs.is_empty() {
        let new_ended = new_msgs.last().map(|m| m.ts.as_str()).unwrap_or("");
        let new_count = seq;
        // Check resume gap between old last and first new
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

