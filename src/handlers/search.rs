use axum::extract::{Query, State};
use indexmap::IndexMap;
use maud::{html, Markup, PreEscaped};
use serde::Deserialize;
use regex::Regex;

use crate::AppState;
use crate::helpers::{build_fts_query, fmt_rel, cwd_label};
use crate::html::components::{error_page, welcome};

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub fav: String,
    #[serde(default)]
    pub sub: String,
}

struct Hit {
    seq: i64,
    role: String,
    snippet: String,
}

struct Group {
    title: String,
    ended_at: String,
    cwd: String,
    ref_num: i64,
    hits: Vec<Hit>,
    extra: usize,
}

/// Hits shown per conversation before collapsing into "+N more matches".
const HITS_PER_SESSION: usize = 5;

/// Same vocabulary as the conversation view: USER / Claude / tool call.
fn role_label(role: &str) -> &'static str {
    match role {
        "user" => "USER",
        "assistant" => "Claude",
        "tool_use" => "tool call",
        _ => "message",
    }
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
        let conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return error_page("search index unavailable — try again in a moment"),
        };
        let fts_q = build_fts_query(&q);
        if fts_q.is_empty() {
            return html! { div id="main" { (welcome()) } };
        }

        // Optional filters share one WHERE tail between count and rows.
        let mut where_extra = String::new();
        let mut binds: Vec<String> = vec![fts_q.clone()];
        if params.sub != "1" {
            where_extra.push_str(" AND s.is_subagent=0");
        }
        if params.fav == "1" {
            where_extra.push_str(" AND s.is_favourite=1");
        }
        if !params.project.is_empty() {
            binds.push(params.project.clone());
            where_extra.push_str(&format!(" AND s.cwd=?{}", binds.len()));
        }

        let total: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM msg_fts
                 JOIN messages m ON m.id = msg_fts.rowid
                 JOIN sessions s ON s.session_id = m.session_id
                 WHERE msg_fts MATCH ?1{}",
                where_extra
            ),
            rusqlite::params_from_iter(binds.iter()),
            |r| r.get(0),
        ).unwrap_or(0);

        let sql = format!(
            "SELECT m.session_id, m.seq, m.role,
                    s.first_user_text, s.ended_at, s.cwd,
                    COALESCE(s.ref_num, 0),
                    snippet(msg_fts, 0, '<mark>', '</mark>', '\u{2026}', 20) AS hit
             FROM msg_fts
             JOIN messages m ON m.id = msg_fts.rowid
             JOIN sessions s ON s.session_id = m.session_id
             WHERE msg_fts MATCH ?1{}
             ORDER BY msg_fts.rank LIMIT 200",
            where_extra
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return error_page("search query failed"),
        };
        #[allow(clippy::type_complexity)]
        let rows: Vec<(String, i64, String, String, String, String, i64, String)> = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            )))
            .map(|rs| rs.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        // Group hits by session, preserving best-rank-first order.
        let mut groups: IndexMap<String, Group> = IndexMap::new();
        for (sid, seq, role, title, ended_at, cwd, ref_num, hit) in rows {
            let g = groups.entry(sid).or_insert_with(|| Group {
                title, ended_at, cwd, ref_num,
                hits: Vec::new(), extra: 0,
            });
            if g.hits.len() < HITS_PER_SESSION {
                g.hits.push(Hit { seq, role, snippet: hit });
            } else {
                g.extra += 1;
            }
        }

        let filter_bar = filter_bar_html(&conn, &params, &home);
        let filters_active = !params.project.is_empty() || params.fav == "1" || params.sub == "1";

        if groups.is_empty() {
            return html! {
                div id="main" {
                    (filter_bar)
                    div.welcome {
                        div.welcome-icon { "\u{25C8}" }
                        p.welcome-text { (format!("No results for \u{201C}{}\u{201D}", q)) }
                        @if filters_active {
                            p.welcome-sub { "Filters are narrowing this search" }
                            button.clear-filters-btn onclick="clearFilters()" { "clear filters" }
                        }
                    }
                }
            };
        }

        let shown: i64 = groups.values().map(|g| g.hits.len() as i64).sum();
        let count_label = if total > shown {
            format!(
                "{} matches in {} conversations for \u{201C}{}\u{201D} (showing top {})",
                total, groups.len(), q, shown
            )
        } else {
            format!("{} matches in {} conversations for \u{201C}{}\u{201D}", total, groups.len(), q)
        };

        html! {
            div id="main" {
                (filter_bar)
                div.search-hd { (count_label) }
                @for (sid, g) in &groups {
                    div.sgroup {
                        div.sgroup-hd
                            hx-get=(format!("/session/{}?seq={}", sid, g.hits.first().map(|h| h.seq).unwrap_or(0)))
                            hx-target="#main"
                            hx-push-url=(format!("/session/{}", sid))
                            onclick=(format!("activateSession('{}')", sid))
                        {
                            p.sgroup-title { (if g.title.is_empty() { "—" } else { &g.title }) }
                            div.sgroup-meta {
                                span.sgroup-ref { (format!("#{}", g.ref_num)) }
                                span { (fmt_rel(&g.ended_at)) }
                                span { (cwd_label(&g.cwd, &home)) }
                                span.sgroup-count { (format!("{} match{}", g.hits.len() + g.extra, if g.hits.len() + g.extra == 1 { "" } else { "es" })) }
                            }
                        }
                        @for h in &g.hits {
                            div.sr.sr-grouped
                                hx-get=(format!("/session/{}?seq={}", sid, h.seq))
                                hx-target="#main"
                                hx-push-url=(format!("/session/{}", sid))
                                onclick=(format!("activateSession('{}')", sid))
                            {
                                p.sr-snip { (PreEscaped(&h.snippet)) }
                                div.sr-meta { span { (role_label(&h.role)) } }
                            }
                        }
                        @if g.extra > 0 {
                            div.sr-more
                                hx-get=(format!("/session/{}?seq={}", sid, g.hits.first().map(|h| h.seq).unwrap_or(0)))
                                hx-target="#main"
                                hx-push-url=(format!("/session/{}", sid))
                                onclick=(format!("activateSession('{}')", sid))
                            {
                                (format!("+{} more match{} in this conversation", g.extra, if g.extra == 1 { "" } else { "es" }))
                            }
                        }
                    }
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| error_page("search failed — try refreshing"))
}

/// Project dropdown + favourites/subagent toggles. Rendered with the results
/// so selections persist across HTMX swaps; controls re-run the search via
/// runSearch() which collects q + all filter values.
fn filter_bar_html(conn: &rusqlite::Connection, params: &SearchParams, home: &str) -> Markup {
    let projects: Vec<String> = conn
        .prepare(
            "SELECT cwd FROM sessions WHERE cwd IS NOT NULL AND cwd != ''
             GROUP BY cwd ORDER BY MAX(ended_at) DESC LIMIT 40",
        )
        .and_then(|mut stmt| {
            let v = stmt.query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            Ok(v)
        })
        .unwrap_or_default();

    html! {
        div.search-filters {
            select.sfilter id="sf-project" onchange="runSearch()" {
                option value="" { "all projects" }
                @for p in &projects {
                    option value=(p) selected[*p == params.project] { (cwd_label(p, home)) }
                }
            }
            label.sfilter-toggle {
                input type="checkbox" id="sf-fav" onchange="runSearch()" checked[params.fav == "1"];
                "\u{2605} favourites"
            }
            label.sfilter-toggle {
                input type="checkbox" id="sf-sub" onchange="runSearch()" checked[params.sub == "1"];
                "subagents"
            }
        }
    }
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

        let mut paths: Vec<_> = match std::fs::read_dir(&plans_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                .collect(),
            Err(_) => return error_page("plans directory unreadable"),
        };
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
            // idx comes from the lowercased copy; lowercasing can shift byte
            // offsets, so snap both bounds to char boundaries of `text` to
            // avoid panicking mid-codepoint.
            let idx = tl.find(first_word.as_str()).unwrap_or(0).min(text.len());
            let mut start = idx.saturating_sub(60);
            while start > 0 && !text.is_char_boundary(start) { start -= 1; }
            let mut end = (idx + first_word.len() + 120).min(text.len());
            while end < text.len() && !text.is_char_boundary(end) { end += 1; }
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
    .unwrap_or_else(|_| error_page("plans search failed — try refreshing"))
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
