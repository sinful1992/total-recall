use rusqlite::{Connection, Result};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(db_path)?;
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
            session_id TEXT, seq INTEGER, role TEXT, ts TEXT, text TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS msg_fts USING fts5(
            text, session_id UNINDEXED, seq UNINDEXED,
            tokenize='porter unicode61', content='messages', content_rowid='id'
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
    Ok(())
}

pub fn open(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}
