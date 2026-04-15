use axum::extract::{Query, State};
use maud::Markup;
use serde::Deserialize;

use crate::AppState;
use crate::html::components::{SessionRow, drawer_timeline_html, drawer_projects_html};

#[derive(Deserialize)]
pub struct DrawerParams {
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub auto: String,
}

fn default_by() -> String { "timeline".to_string() }

pub async fn handler(
    State(state): State<AppState>,
    Query(params): Query<DrawerParams>,
) -> Markup {
    let by = params.by.clone();
    let show_automated = params.auto == "1";
    let db_path = state.db_path.clone();
    let home = state.home_dir.to_string_lossy().into_owned();

    tokio::task::spawn_blocking(move || {
        let conn = crate::db::open(&db_path).unwrap();
        let where_clause = if show_automated { "" } else { "WHERE is_automated=0" };
        let sql = format!("SELECT * FROM sessions {} ORDER BY ended_at DESC", where_clause);
        let mut stmt = conn.prepare(&sql).unwrap();
        let sessions: Vec<SessionRow> = stmt
            .query_map([], |r| SessionRow::from_row(r))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        if by == "projects" {
            drawer_projects_html(&sessions, &home)
        } else {
            let auto_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions WHERE is_automated=1", [], |r| r.get(0))
                .unwrap_or(0);
            drawer_timeline_html(&sessions, auto_count, show_automated)
        }
    })
    .await
    .unwrap()
}
