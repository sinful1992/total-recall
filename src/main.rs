#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod handlers;
mod helpers;
mod html;
mod indexer;
mod parser;
mod routes;

use std::net::TcpListener;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub home_dir: PathBuf,
}

fn resolve_home() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() { return PathBuf::from(h); }
    }
    if let Ok(p) = std::env::var("USERPROFILE") {
        if !p.is_empty() { return PathBuf::from(p); }
    }
    panic!("Cannot resolve user home: neither HOME nor USERPROFILE is set");
}

fn cache_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = home;
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(local).join("total-recall")
    }
    #[cfg(not(target_os = "windows"))]
    {
        home.join(".cache").join("total-recall")
    }
}

fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    listener.local_addr().unwrap().port()
}

fn wait_ready(port: u16) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            return;
        }
        if std::time::Instant::now() > deadline {
            eprintln!("Server failed to start in time");
            std::process::exit(1);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn main() {
    let home_dir = resolve_home();
    let db_path = cache_dir(&home_dir).join("index.sqlite");

    db::init_db(&db_path).expect("Failed to initialize database");
    indexer::build_or_refresh_index(&db_path, &home_dir);

    let port = pick_free_port();
    let db_path_clone = db_path.clone();
    let home_clone = home_dir.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db_bg = db_path_clone.clone();
            let home_bg = home_clone.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(120));
                interval.tick().await; // skip first tick — already indexed at startup
                loop {
                    interval.tick().await;
                    let db = db_bg.clone();
                    let home = home_bg.clone();
                    tokio::task::spawn_blocking(move || {
                        indexer::build_or_refresh_index(&db, &home);
                    }).await.ok();
                }
            });

            let app = routes::make_router(db_path_clone, home_clone);
            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
                .await
                .unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    wait_ready(port);
    println!("Server ready on :{}", port);

    let url = format!("http://127.0.0.1:{}/", port);

    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External(url.parse().unwrap()),
            )
            .title("Total Recall")
            .inner_size(1300.0, 860.0)
            .min_inner_size(800.0, 600.0)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run Tauri application");
}
