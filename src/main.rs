#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod handlers;
mod helpers;
mod html;
mod indexer;
mod parser;
mod routes;

use std::net::TcpListener;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
}

fn cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(local).join("conv-browser")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".cache").join("conv-browser")
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
    let db_path = cache_dir().join("index.sqlite");

    db::init_db(&db_path).expect("Failed to initialize database");
    indexer::build_or_refresh_index(&db_path);

    let port = pick_free_port();
    let db_path_clone = db_path.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = routes::make_router(db_path_clone);
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
            .title("Conv Browser")
            .inner_size(1300.0, 860.0)
            .min_inner_size(800.0, 600.0)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run Tauri application");
}
