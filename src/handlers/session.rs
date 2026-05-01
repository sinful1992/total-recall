use axum::extract::{Path, Query, State};
use maud::{html, Markup};
use serde::Deserialize;

use crate::AppState;
use crate::html::components::{MsgRow, session_view_html, welcome};

#[derive(Deserialize)]
pub struct SessionParams {
    pub seq: Option<i64>,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Query(params): Query<SessionParams>,
) -> Markup {
    let db_path = state.db_path.clone();
    let home = state.home_dir.to_string_lossy().into_owned();
    let scroll_seq = params.seq;

    tokio::task::spawn_blocking(move || {
        let conn = crate::db::open(&db_path).unwrap();

        let sess = conn.query_row(
            "SELECT session_id, first_user_text, started_at, ended_at, msg_count, is_resumed, cwd, ref_num
             FROM sessions WHERE session_id=?1",
            [&sid],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                r.get::<_, Option<i64>>(7)?.unwrap_or(0),
            )),
        );

        match sess {
            Err(_) => html! {
                div id="main" {
                    (welcome())
                }
            },
            Ok((session_id, first_user_text, started_at, ended_at, msg_count, is_resumed, cwd, ref_num)) => {
                let mut stmt = conn
                    .prepare("SELECT seq, role, ts, text FROM messages WHERE session_id=?1 ORDER BY seq")
                    .unwrap();
                let messages: Vec<MsgRow> = stmt
                    .query_map([&session_id], |r| MsgRow::from_row(r))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();

                session_view_html(
                    &session_id,
                    &first_user_text,
                    &started_at,
                    &ended_at,
                    msg_count,
                    is_resumed,
                    &cwd,
                    &home,
                    &messages,
                    scroll_seq,
                    ref_num,
                )
            }
        }
    })
    .await
    .unwrap()
}
