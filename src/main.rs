#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use notify::Watcher;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::UpdaterExt;

use total_recall::{db, indexer, routes};

/// Update check runs Rust-side with a native dialog: the webview must not be
/// a dependency of the updater, or a dead renderer (v1.6.1 AppImage EGL
/// crash) leaves users stranded on a broken build with no way to get the fix.
async fn check_for_update(app: tauri::AppHandle) {
    let Ok(updater) = app.updater_builder().build() else { return };
    let Ok(Some(update)) = updater.check().await else { return };
    eprintln!("Update v{} available", update.version);

    let prompt = app.clone();
    let version = update.version.clone();
    let confirmed = tauri::async_runtime::spawn_blocking(move || {
        prompt
            .dialog()
            .message(format!("Total Recall v{} is available.", version))
            .title("Update available")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Install & Restart".into(),
                "Later".into(),
            ))
            .blocking_show()
    })
    .await
    .unwrap_or(false);
    if !confirmed {
        return;
    }

    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(e) => app
            .dialog()
            .message(format!("Update failed: {e}"))
            .title("Update")
            .show(|_| {}),
    }
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

/// The DB holds user data (notes, favourites, pruned-session archive), so it
/// belongs in the data dir, not the cache dir.
fn data_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = home;
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(local).join("total-recall")
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("total-recall")
    }
}

/// One-time move of the DB out of ~/.cache (where cleanup tools may delete it).
#[cfg(not(target_os = "windows"))]
fn migrate_legacy_db(home: &Path, new_db: &Path) {
    if new_db.exists() { return; }
    let old_dir = home.join(".cache").join("total-recall");
    if !old_dir.join("index.sqlite").exists() { return; }
    if let Some(parent) = new_db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    for suffix in ["", "-wal", "-shm"] {
        let old = old_dir.join(format!("index.sqlite{}", suffix));
        if old.exists() {
            let new = new_db.with_file_name(format!("index.sqlite{}", suffix));
            if std::fs::rename(&old, &new).is_err() {
                // Cross-device fallback
                if std::fs::copy(&old, &new).is_ok() {
                    let _ = std::fs::remove_file(&old);
                }
            }
        }
    }
    println!("Migrated database from {} to {}", old_dir.display(), new_db.parent().unwrap().display());
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
    let db_path = data_dir(&home_dir).join("index.sqlite");
    #[cfg(not(target_os = "windows"))]
    migrate_legacy_db(&home_dir, &db_path);

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

    // Live re-index: watch ~/.claude/projects/ for new/modified JSONL files
    let projects_dir = home_dir.join(".claude").join("projects");
    if projects_dir.exists() {
        let db_path_w = db_path.clone();
        let home_w = home_dir.clone();
        std::thread::spawn(move || {
            let last = Arc::new(Mutex::new(
                std::time::Instant::now() - std::time::Duration::from_secs(10),
            ));
            let last_c = last.clone();
            let mut watcher = notify::recommended_watcher(
                move |_res: notify::Result<notify::Event>| {
                    let should = {
                        let mut t = last_c.lock().unwrap();
                        if t.elapsed().as_secs() >= 5 {
                            *t = std::time::Instant::now();
                            true
                        } else {
                            false
                        }
                    };
                    if should {
                        indexer::build_or_refresh_index(&db_path_w, &home_w);
                    }
                },
            )
            .expect("Failed to create file watcher");
            watcher
                .watch(&projects_dir, notify::RecursiveMode::Recursive)
                .expect("Failed to watch projects dir");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        });
    }

    let url = format!("http://127.0.0.1:{}/", port);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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

            tauri::async_runtime::spawn(check_for_update(app.handle().clone()));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run Tauri application");
}
