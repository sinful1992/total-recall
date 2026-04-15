use axum::extract::State;
use maud::Markup;

use crate::AppState;
use crate::html::components::{SessionRow, drawer_timeline_html};

pub async fn handler(State(state): State<AppState>) -> Markup {
    let db_path = state.db_path.clone();
    let home = state.home_dir.clone();
    tokio::task::spawn_blocking(move || {
        crate::indexer::build_or_refresh_index(&db_path, &home);
        let conn = crate::db::open(&db_path).unwrap();
        let auto_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE is_automated=1", [], |r| r.get(0))
            .unwrap_or(0);
        let mut stmt = conn
            .prepare("SELECT * FROM sessions WHERE is_automated=0 ORDER BY ended_at DESC")
            .unwrap();
        let sessions: Vec<SessionRow> = stmt
            .query_map([], |r| SessionRow::from_row(r))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        drawer_timeline_html(&sessions, auto_count, false)
    })
    .await
    .unwrap()
}
