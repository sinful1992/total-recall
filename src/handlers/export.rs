use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;
use axum::body::Body;

use crate::AppState;
use crate::helpers::{fmt_date, fmt_ts_short, session_codename};

pub async fn handler(
    State(state): State<AppState>,
    Path(sid): Path<String>,
) -> Response {
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => {
                return Response::builder()
                    .status(503)
                    .body(Body::from("index unavailable"))
                    .unwrap();
            }
        };

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

        let body = match sess {
            Err(_) => "Session not found".to_string(),
            Ok((session_id, first_user_text, started_at, _ended_at, msg_count, is_resumed, cwd, ref_num)) => {
                let codename = session_codename(&session_id);
                let title = if first_user_text.is_empty() { "Conversation".to_string() } else { first_user_text };
                let mut md = String::new();
                md.push_str(&format!("# {}\n\n", title));
                md.push_str(&format!("**#{} · {}**  \n", ref_num, codename));
                md.push_str(&format!("{}  \n", fmt_date(&started_at)));
                if !cwd.is_empty() {
                    md.push_str(&format!("{}  \n", cwd));
                }
                md.push_str(&format!("{} messages", msg_count));
                if is_resumed != 0 { md.push_str(" · resumed"); }
                md.push_str("\n\n---\n\n");

                let mut stmt = conn
                    .prepare("SELECT role, ts, text FROM messages WHERE session_id=?1 ORDER BY seq")
                    .unwrap();
                let rows: Vec<(String, String, String)> = stmt
                    .query_map([&session_id], |r| Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        r.get::<_, String>(2)?,
                    )))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();

                for (role, ts, text) in &rows {
                    let time = fmt_ts_short(ts);
                    if role == "tool_use" {
                        // Parse JSON and render as markdown code block
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(text) {
                            let name = data.get("n").and_then(|v| v.as_str()).unwrap_or("Tool");
                            match name {
                                "Edit" | "Write" => {
                                    let file = data.get("f").and_then(|v| v.as_str()).unwrap_or("?");
                                    let diff = data.get("d").and_then(|v| v.as_str()).unwrap_or("");
                                    md.push_str(&format!("**{}** `{}` _{}_\n\n", name, file, time));
                                    md.push_str("```diff\n");
                                    md.push_str(diff);
                                    md.push_str("```\n\n---\n\n");
                                }
                                "Bash" => {
                                    let cmd = data.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
                                    let desc = data.get("desc").and_then(|v| v.as_str()).unwrap_or("");
                                    md.push_str(&format!("**Bash** _{}_\n\n", time));
                                    if !desc.is_empty() { md.push_str(&format!("> {}\n\n", desc)); }
                                    md.push_str("```sh\n");
                                    md.push_str(cmd);
                                    md.push_str("\n```\n\n---\n\n");
                                }
                                _ => {
                                    md.push_str(&format!("**{}** _{}_\n\n", name, time));
                                    md.push_str(text);
                                    md.push_str("\n\n---\n\n");
                                }
                            }
                        }
                        continue;
                    }
                    let label = if role == "user" { "USER" } else { "Claude" };
                    md.push_str(&format!("**{}** _{}_\n\n", label, time));
                    md.push_str(text);
                    md.push_str("\n\n---\n\n");
                }
                md
            }
        };

        Response::builder()
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(body))
            .unwrap()
    })
    .await
    .unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Body::from("export failed"))
            .unwrap()
    })
}
