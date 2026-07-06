use axum::extract::State;
use maud::Markup;

use crate::AppState;
use crate::html::components::error_page;
use crate::handlers::index::default_sidebar;

pub async fn handler(State(state): State<AppState>) -> Markup {
    let db_path = state.db_path.clone();
    let home = state.home_dir.clone();
    tokio::task::spawn_blocking(move || {
        crate::indexer::build_or_refresh_index(&db_path, &home);
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return error_page("index unavailable"),
        };
        default_sidebar(&conn)
    })
    .await
    .unwrap_or_else(|_| error_page("refresh failed — try again"))
}
