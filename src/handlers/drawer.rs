use axum::extract::{Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::AppState;
use crate::html::components::{SessionRow, session_item_html, drawer_timeline_html, drawer_projects_html};

#[derive(Deserialize)]
pub struct DrawerParams {
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub auto: String,
}

fn default_by() -> String { "timeline".to_string() }

#[derive(Deserialize)]
pub struct OlderParams {
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub auto: String,
}

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

pub async fn older_handler(
    State(state): State<AppState>,
    Query(params): Query<OlderParams>,
) -> Markup {
    let show_automated = params.auto == "1";
    let offset = params.offset;
    let db_path = state.db_path.clone();

    tokio::task::spawn_blocking(move || {
        let conn = crate::db::open(&db_path).unwrap();
        let auto_clause = if show_automated { "" } else { "AND is_automated=0" };

        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM sessions WHERE julianday('now') - julianday(substr(ended_at,1,10)) >= 30 {}", auto_clause),
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let sql = format!(
            "SELECT * FROM sessions WHERE julianday('now') - julianday(substr(ended_at,1,10)) >= 30 {} ORDER BY ended_at DESC LIMIT 30 OFFSET {}",
            auto_clause, offset
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        let sessions: Vec<SessionRow> = stmt
            .query_map([], |r| SessionRow::from_row(r))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let next_offset = offset + 30;
        let has_more = next_offset < total;
        let auto_param = if show_automated { "1" } else { "0" };

        html! {
            @for s in &sessions {
                (session_item_html(&s.session_id, &s.first_user_text, &s.ended_at, s.msg_count, s.is_resumed, s.ref_num, None))
            }
            @if has_more {
                button.load-more-btn
                    hx-get=(format!("/drawer/older-items?offset={}&auto={}", next_offset, auto_param))
                    hx-target="this"
                    hx-swap="outerHTML"
                {
                    (format!("Load {} more", (total - next_offset).min(30)))
                }
            }
        }
    })
    .await
    .unwrap()
}
