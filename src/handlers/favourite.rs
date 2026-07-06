use axum::extract::{Path, State};
use maud::{html, Markup};

use crate::AppState;
use crate::html::components::error_page;

pub async fn handler(
    State(state): State<AppState>,
    Path(sid): Path<String>,
) -> Markup {
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return error_page("index unavailable"),
        };
        let _ = conn.execute(
            "UPDATE sessions SET is_favourite = CASE WHEN is_favourite=1 THEN 0 ELSE 1 END WHERE session_id=?1",
            [&sid],
        );
        let is_fav: i64 = conn
            .query_row(
                "SELECT COALESCE(is_favourite, 0) FROM sessions WHERE session_id=?1",
                [&sid],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let cls = if is_fav == 1 { "fav-btn active" } else { "fav-btn" };
        let title = if is_fav == 1 { "Remove from favourites" } else { "Mark as favourite" };
        html! {
            button class=(cls)
                hx-post=(format!("/session/{}/favourite", sid))
                hx-target="this"
                hx-swap="outerHTML"
                onclick=(format!("onDetailFavClick(event, '{}')", sid))
                title=(title)
            { "★" }
        }
    })
    .await
    .unwrap_or_else(|_| error_page("favourite toggle failed"))
}
