use axum::extract::{Query, State};
use maud::{html, Markup, PreEscaped};
use serde::Deserialize;
use regex::Regex;

use crate::AppState;
use crate::helpers::{build_fts_query, fmt_rel, cwd_label};
use crate::html::components::welcome;

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Markup {
    let q = params.q.trim().to_string();
    if q.len() < 2 {
        return html! {
            div id="main" { (welcome()) }
        };
    }

    let db_path = state.db_path.clone();
    let home = state.home_dir.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || {
        let conn = crate::db::open(&db_path).unwrap();
        let fts_q = build_fts_query(&q);

        let total: i64 = if !fts_q.is_empty() {
            conn.query_row(
                "SELECT COUNT(*) FROM msg_fts JOIN messages m ON m.id = msg_fts.rowid WHERE msg_fts MATCH ?1",
                [&fts_q],
                |r| r.get(0),
            ).unwrap_or(0)
        } else { 0 };

        let rows: Vec<(String, i64, String, String, String, String, String)> = if !fts_q.is_empty() {
            let mut stmt = conn.prepare("
                SELECT m.session_id, m.seq, m.role,
                       s.first_user_text, s.ended_at, s.cwd,
                       snippet(msg_fts, 0, '<mark>', '</mark>', '\u{2026}', 20) AS hit
                FROM msg_fts
                JOIN messages m ON m.id = msg_fts.rowid
                JOIN sessions s ON s.session_id = m.session_id
                WHERE msg_fts MATCH ?1
                ORDER BY msg_fts.rank, s.ended_at DESC LIMIT 60
            ").unwrap();
            stmt.query_map([&fts_q], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get::<_, String>(6)?,
            )))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
        } else {
            vec![]
        };

        if rows.is_empty() {
            return html! {
                div id="main" {
                    div.welcome {
                        div.welcome-icon { "\u{25C8}" }
                        p.welcome-text { (format!("No results for \u{201C}{}\u{201D}", q)) }
                    }
                }
            };
        }

        let count_label = if total > 60 {
            format!("first 60 of {} results for \u{201C}{}\u{201D}", total, q)
        } else {
            format!("{} results for \u{201C}{}\u{201D}", rows.len(), q)
        };

        html! {
            div id="main" {
                div.search-hd { (count_label) }
                @for (sid, seq, role, first_user_text, ended_at, cwd, hit) in &rows {
                    div.sr
                        hx-get=(format!("/session/{}?seq={}", sid, seq))
                        hx-target="#main"
                        hx-push-url=(format!("/session/{}", sid))
                        onclick=(format!("activateSession('{}')", sid))
                    {
                        p.sr-title { (if first_user_text.is_empty() { "—" } else { first_user_text }) }
                        p.sr-snip { (PreEscaped(hit)) }
                        div.sr-meta {
                            span { (fmt_rel(ended_at)) }
                            span { (cwd_label(cwd, &home)) }
                            span { (role) }
                        }
                    }
                }
            }
        }
    })
    .await
    .unwrap()
}

pub async fn plans_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Markup {
    let q = params.q.trim().to_string();
    if q.len() < 2 {
        return html! {
            div id="main" {
                div.welcome {
                    div.welcome-icon { "\u{25C8}" }
                    p.welcome-text { "Search your plans above" }
                }
            }
        };
    }

    let plans_dir = dirs_plans(&state.home_dir);
    if !plans_dir.exists() {
        return html! {
            div id="main" {
                div.welcome { p { "No plans directory." } }
            }
        };
    }

    tokio::task::spawn_blocking(move || {
        let words: Vec<String> = q.split_whitespace().map(|w| w.to_lowercase()).collect();
        let mut results: Vec<(String, String, String, String)> = Vec::new();

        let mut paths: Vec<_> = std::fs::read_dir(&plans_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
            .collect();
        paths.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

        for entry in &paths {
            let path = entry.path();
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let tl = text.to_lowercase();
            if !words.iter().all(|w| tl.contains(w.as_str())) { continue; }

            let title = text.lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l[2..].trim().to_string())
                .unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into_owned());

            let slug = path.file_stem().unwrap().to_string_lossy().into_owned();

            let first_word = &words[0];
            let idx = tl.find(first_word.as_str()).unwrap_or(0);
            let start = idx.saturating_sub(60);
            let end = (idx + first_word.len() + 120).min(text.len());
            let raw = text[start..end].replace('\n', " ");
            let mut snippet = raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            for w in &words {
                let re = Regex::new(&format!("(?i)({})", regex::escape(w))).unwrap();
                snippet = re.replace_all(&snippet, "<mark>$1</mark>").into_owned();
            }

            let age = fmt_rel_from_path(&path);
            results.push((slug, title, snippet, age));
        }

        if results.is_empty() {
            return html! {
                div id="main" {
                    div.welcome {
                        div.welcome-icon { "\u{25C8}" }
                        p.welcome-text { (format!("No plans matching \u{201C}{}\u{201D}", q)) }
                    }
                }
            };
        }

        let count = results.len();
        html! {
            div id="main" {
                div.search-hd {
                    (format!("{} plan{} matching \u{201C}{}\u{201D}", count, if count == 1 { "" } else { "s" }, q))
                }
                @for (slug, title, snippet, age) in &results {
                    div.sr
                        hx-get=(format!("/plans/{}", slug))
                        hx-target="#main"
                        hx-push-url=(format!("/plans/{}", slug))
                        onclick=(format!("activatePlan('{}')", slug))
                    {
                        p.sr-title { (title) }
                        p.sr-snip { (PreEscaped(snippet)) }
                        div.sr-meta { span { (age) } }
                    }
                }
            }
        }
    })
    .await
    .unwrap()
}

fn dirs_plans(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".claude").join("plans")
}

fn fmt_rel_from_path(path: &std::path::Path) -> String {
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

