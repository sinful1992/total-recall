use axum::{
    routing::{get, post},
    Router,
};
use std::path::PathBuf;

use crate::AppState;
use crate::handlers::{drawer, index, plans, refresh, search, session};

pub fn make_router(db_path: PathBuf, home_dir: PathBuf) -> Router {
    let state = AppState { db_path, home_dir };
    Router::new()
        .route("/", get(index::handler))
        .route("/drawer", get(drawer::handler))
        .route("/session/:sid", get(session::handler))
        .route("/search", get(search::handler))
        .route("/search/plans", get(search::plans_handler))
        .route("/plans/sidebar", get(plans::sidebar_handler))
        .route("/plans/:slug", get(plans::plan_handler))
        .route("/refresh", post(refresh::handler))
        .with_state(state)
}
