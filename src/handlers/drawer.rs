use axum::extract::{Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::AppState;
use crate::html::components::{SessionRow, session_item_html, drawer_timeline_html, drawer_projects_html, error_page};

#[derive(Deserialize)]
pub struct DrawerParams {
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub auto: String,
    #[serde(default)]
    pub sub: String,
}

fn default_by() -> String { "timeline".to_string() }

#[derive(Deserialize)]
pub struct OlderParams {
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub auto: String,
    #[serde(default)]
    pub sub: String,
}

/// WHERE tail hiding automated/subagent sessions unless toggled on.
pub fn visibility_clause(show_automated: bool, show_subagents: bool) -> String {
    let mut conds: Vec<&str> = Vec::new();
    if !show_automated { conds.push("is_automated=0"); }
    if !show_subagents { conds.push("COALESCE(is_subagent,0)=0"); }
    if conds.is_empty() { String::new() } else { format!("WHERE {}", conds.join(" AND ")) }
}

pub async fn handler(
    State(state): State<AppState>,
    Query(params): Query<DrawerParams>,
) -> Markup {
    let by = params.by.clone();
    let show_automated = params.auto == "1";
    let show_subagents = params.sub == "1";
    let db_path = state.db_path.clone();
    let home = state.home_dir.to_string_lossy().into_owned();

    tokio::task::spawn_blocking(move || {
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return error_page("index unavailable"),
        };
        let where_clause = visibility_clause(show_automated, show_subagents);
        let sql = format!("SELECT * FROM sessions {} ORDER BY ended_at DESC", where_clause);
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return error_page("index unavailable"),
        };
        let sessions: Vec<SessionRow> = stmt
            .query_map([], |r| SessionRow::from_row(r))
            .map(|rs| rs.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        if by == "projects" {
            drawer_projects_html(&sessions, &home)
        } else {
            let auto_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions WHERE is_automated=1", [], |r| r.get(0))
                .unwrap_or(0);
            let sub_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions WHERE COALESCE(is_subagent,0)=1", [], |r| r.get(0))
                .unwrap_or(0);
            drawer_timeline_html(&sessions, auto_count, show_automated, sub_count, show_subagents)
        }
    })
    .await
    .unwrap_or_else(|_| error_page("drawer failed — try refreshing"))
}

pub async fn older_handler(
    State(state): State<AppState>,
    Query(params): Query<OlderParams>,
) -> Markup {
    let show_automated = params.auto == "1";
    let show_subagents = params.sub == "1";
    let offset = params.offset;
    let db_path = state.db_path.clone();

    tokio::task::spawn_blocking(move || {
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return error_page("index unavailable"),
        };
        let mut vis = String::new();
        if !show_automated { vis.push_str(" AND is_automated=0"); }
        if !show_subagents { vis.push_str(" AND COALESCE(is_subagent,0)=0"); }

        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM sessions WHERE julianday('now') - julianday(substr(ended_at,1,10)) >= 30{}", vis),
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let sql = format!(
            "SELECT * FROM sessions WHERE julianday('now') - julianday(substr(ended_at,1,10)) >= 30{} ORDER BY ended_at DESC LIMIT 30 OFFSET {}",
            vis, offset
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return error_page("index unavailable"),
        };
        let sessions: Vec<SessionRow> = stmt
            .query_map([], |r| SessionRow::from_row(r))
            .map(|rs| rs.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        let next_offset = offset + 30;
        let has_more = next_offset < total;
        let auto_param = if show_automated { "1" } else { "0" };
        let sub_param = if show_subagents { "1" } else { "0" };

        html! {
            @for s in &sessions {
                (session_item_html(&s.session_id, &s.first_user_text, &s.ended_at, s.msg_count, s.is_resumed, s.is_favourite, s.ref_num, None))
            }
            @if has_more {
                button.load-more-btn
                    hx-get=(format!("/drawer/older-items?offset={}&auto={}&sub={}", next_offset, auto_param, sub_param))
                    hx-target="this"
                    hx-swap="outerHTML"
                {
                    (format!("Load {} more", (total - next_offset).min(30)))
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| error_page("drawer failed — try refreshing"))
}
