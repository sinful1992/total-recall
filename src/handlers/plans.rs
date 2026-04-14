use axum::extract::{Path, State};
use maud::{html, Markup, PreEscaped};

use crate::AppState;

fn plans_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".claude").join("plans")
}

pub async fn sidebar_handler(_state: State<AppState>) -> Markup {
    let dir = plans_dir();
    if !dir.exists() {
        return html! {
            div id="sidebar" {
                p style="padding:12px;color:var(--text-dim)" { "No plans directory found." }
            }
        };
    }

    tokio::task::spawn_blocking(move || {
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
            .collect();
        paths.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

        let items: Vec<Markup> = paths.iter().map(|entry| {
            let path = entry.path();
            let slug = path.file_stem().unwrap().to_string_lossy().into_owned();
            let mut title = "Untitled".to_string();
            let mut size_lines = 0usize;

            if let Ok(text) = std::fs::read_to_string(&path) {
                size_lines = text.lines().count();
                for line in text.lines() {
                    if line.starts_with("# ") {
                        title = line[2..].trim().to_string();
                        break;
                    }
                }
            }

            let age = fmt_rel_mtime(&path);

            html! {
                div.plan-item
                    data-slug=(slug)
                    hx-get=(format!("/plans/{}", slug))
                    hx-target="#main"
                    hx-push-url="true"
                    onclick=(format!("activatePlan('{}')", slug))
                {
                    p.plan-title { (title) }
                    div.plan-meta {
                        span { (age) }
                        span { (format!("{} lines", size_lines)) }
                    }
                }
            }
        }).collect();

        html! {
            div id="sidebar" {
                div.drawer-hd style="cursor:default" {
                    span.drawer-hd-label { "Plans" }
                    span.drawer-count { (items.len()) }
                }
                @for item in &items { (item) }
            }
        }
    })
    .await
    .unwrap()
}

pub async fn plan_handler(_state: State<AppState>, Path(slug): Path<String>) -> Markup {
    tokio::task::spawn_blocking(move || {
        let path = plans_dir().join(format!("{}.md", slug));
        if !path.exists() {
            return html! {
                div id="main" { p { "Plan not found." } }
            };
        }

        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let mtime = path.metadata().ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                chrono::DateTime::from_timestamp(secs as i64, 0)
            })
            .map(|dt| {
                let local: chrono::DateTime<chrono::Local> = dt.into();
                local.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_default();

        let title = raw.lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l[2..].trim().to_string())
            .unwrap_or_else(|| slug.clone());

        let line_count = raw.lines().count();
        // Escape the raw markdown for embedding in data-raw attribute
        let raw_escaped = raw
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");

        html! {
            div id="main" class="plan-view" {
                div.plan-plan-hd {
                    div {
                        h2.sess-hd-title { (title) }
                        div.sess-hd-meta {
                            span.sess-meta-chip { (mtime) }
                            span.sess-meta-chip { (format!("{} lines", line_count)) }
                        }
                    }
                    button.plan-copy-btn onclick="copyPlanRaw(this)" { "copy raw" }
                }
                div id="plan-md-content" class="plan-view"
                    data-raw=(PreEscaped(raw_escaped))
                {}
                script { (PreEscaped("renderPlanMarkdown();")) }
            }
        }
    })
    .await
    .unwrap()
}

fn fmt_rel_mtime(path: &std::path::Path) -> String {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            let secs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64)
                - d.as_secs() as i64;
            if secs < 60 { "just now".to_string() }
            else if secs < 3600 { format!("{}m ago", secs / 60) }
            else if secs < 86400 { format!("{}h ago", secs / 3600) }
            else { format!("{}d ago", secs / 86400) }
        })
        .unwrap_or_else(|| "—".to_string())
}
