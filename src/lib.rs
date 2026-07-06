// Library crate: everything except the Tauri shell lives here so that
// `cargo test --lib` runs without linking GTK/webkit system libraries.
pub mod db;
pub mod handlers;
pub mod helpers;
pub mod html;
pub mod indexer;
pub mod parser;
pub mod routes;

use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub home_dir: PathBuf,
}
