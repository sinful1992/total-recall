use axum::extract::State;
use maud::Markup;

use crate::AppState;
use crate::html::shell::full_page;
use crate::html::components::{SessionRow, drawer_timeline_html, error_page};

/// Default drawer contents: automated + subagent sessions hidden.
pub fn default_sidebar(conn: &rusqlite::Connection) -> Markup {
    let auto_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions WHERE is_automated=1", [], |r| r.get(0))
        .unwrap_or(0);
    let sub_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions WHERE COALESCE(is_subagent,0)=1", [], |r| r.get(0))
        .unwrap_or(0);
    let sessions: Vec<SessionRow> = conn
        .prepare("SELECT * FROM sessions WHERE is_automated=0 AND COALESCE(is_subagent,0)=0 ORDER BY ended_at DESC")
        .and_then(|mut stmt| {
            let v = stmt.query_map([], |r| SessionRow::from_row(r))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            Ok(v)
        })
        .unwrap_or_default();
    drawer_timeline_html(&sessions, auto_count, false, sub_count, false)
}

pub async fn handler(State(state): State<AppState>) -> Markup {
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return full_page(error_page("index unavailable — restart the app")),
        };
        full_page(default_sidebar(&conn))
    })
    .await
    .unwrap_or_else(|_| full_page(error_page("startup failed — restart the app")))
}
