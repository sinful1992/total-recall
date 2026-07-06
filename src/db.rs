use rusqlite::{Connection, Result};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = open(db_path)?;
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY, mtime REAL, size INTEGER, session_id TEXT
        );
        CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            file_path TEXT, project_dir TEXT, cwd TEXT,
            started_at TEXT, ended_at TEXT,
            msg_count INTEGER, first_user_text TEXT,
            is_resumed    INTEGER DEFAULT 0,
            is_automated  INTEGER DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY,
            session_id TEXT, seq INTEGER, role TEXT, ts TEXT, text TEXT,
            search_text TEXT
        );
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY, value TEXT
        );
        CREATE INDEX IF NOT EXISTS ix_msg_session ON messages(session_id, seq);
        CREATE INDEX IF NOT EXISTS ix_sessions_ended ON sessions(ended_at DESC);
    ")?;
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN is_automated INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN ref_num INTEGER", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN ref_code TEXT", []);
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS ix_sessions_refnum ON sessions(ref_num)", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN notes TEXT", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN is_favourite INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN is_subagent INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN search_text TEXT", []);
    // Backfill so pre-existing rows (incl. archived orphans) stay searchable.
    conn.execute("UPDATE messages SET search_text = text WHERE search_text IS NULL", [])?;
    upgrade_fts(&conn)?;
    Ok(())
}

/// msg_fts is an external-content FTS5 table. Its indexed column MUST read the
/// same value FTS was given on insert, otherwise deletes remove the wrong
/// tokens and corrupt the index. Older schemas pointed at messages.text while
/// inserting a different fts_text for tool_use rows — rebuild those on the
/// dedicated search_text column.
fn upgrade_fts(conn: &Connection) -> Result<()> {
    let existing_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='msg_fts'",
            [],
            |r| r.get(0),
        )
        .ok();
    let needs_recreate = match &existing_sql {
        Some(sql) => !sql.contains("search_text"),
        None => false,
    };
    if needs_recreate {
        conn.execute_batch("DROP TABLE msg_fts;")?;
    }
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS msg_fts USING fts5(
            search_text, session_id UNINDEXED, seq UNINDEXED,
            tokenize='porter unicode61', content='messages', content_rowid='id'
        );",
    )?;
    if needs_recreate || existing_sql.is_none() {
        conn.execute("INSERT INTO msg_fts(msg_fts) VALUES('rebuild')", [])?;
    }
    Ok(())
}

pub fn open(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}
