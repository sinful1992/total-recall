use chrono::{DateTime, Local, Utc};
use crate::parser::parse_ts;
use regex::Regex;

pub static BUCKET_ORDER: &[&str] = &[
    "Today", "Yesterday", "This week", "This month", "Older",
];

pub fn date_bucket(ts_str: &str) -> &'static str {
    let now = Utc::now();
    let today = now.date_naive();
    match parse_ts(ts_str) {
        None => "Older",
        Some(dt) => {
            let d = dt.date_naive();
            let diff = today.signed_duration_since(d).num_days();
            if diff == 0 { "Today" }
            else if diff == 1 { "Yesterday" }
            else if diff < 7 { "This week" }
            else if diff < 30 { "This month" }
            else { "Older" }
        }
    }
}

pub fn fmt_rel(ts_str: &str) -> String {
    match parse_ts(ts_str) {
        None => "—".to_string(),
        Some(dt) => {
            let secs = (Utc::now() - dt).num_seconds();
            if secs < 60 { "just now".to_string() }
            else if secs < 3600 { format!("{}m ago", secs / 60) }
            else if secs < 86400 { format!("{}h ago", secs / 3600) }
            else if secs < 86400 * 7 { format!("{}d ago", secs / 86400) }
            else if secs < 86400 * 30 { format!("{}w ago", secs / (86400 * 7)) }
            else if secs < 86400 * 365 { format!("{}mo ago", secs / (86400 * 30)) }
            else { format!("{}y ago", secs / (86400 * 365)) }
        }
    }
}

pub fn fmt_date(ts_str: &str) -> String {
    match parse_ts(ts_str) {
        None => "—".to_string(),
        Some(dt) => {
            let local: DateTime<Local> = dt.into();
            local.format("%Y-%m-%d").to_string()
        }
    }
}

pub fn fmt_ts_short(ts_str: &str) -> String {
    match parse_ts(ts_str) {
        None => "".to_string(),
        Some(dt) => {
            let local: DateTime<Local> = dt.into();
            local.format("%H:%M").to_string()
        }
    }
}

pub fn gap_label(ts1: &str, ts2: &str) -> String {
    let dt1 = parse_ts(ts1);
    let dt2 = parse_ts(ts2);
    match (dt1, dt2) {
        (Some(a), Some(b)) => {
            let secs = (b - a).num_seconds().abs();
            if secs < 3600 { format!("{}m gap", secs / 60) }
            else if secs < 86400 { format!("{}h gap", secs / 3600) }
            else { format!("{}d gap", secs / 86400) }
        }
        _ => "gap".to_string(),
    }
}

pub fn cwd_label(cwd: &str) -> String {
    if cwd.is_empty() { return "—".to_string(); }
    let home = std::env::var("HOME").unwrap_or_default();
    let s = if cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    };
    // Keep last 2 path components
    let parts: Vec<&str> = s.trim_end_matches('/').rsplitn(3, '/').collect();
    if parts.len() >= 2 {
        format!("{}/{}", parts[1], parts[0])
    } else {
        s
    }
}

pub fn density_pct(msg_count: i64) -> i64 {
    (msg_count.min(50) * 100 / 50).max(5)
}

pub fn build_fts_query(q: &str) -> String {
    let re_quoted = Regex::new(r#""[^"]*"|\S+"#).unwrap();
    let re_bare = Regex::new(r"[^\w\-]").unwrap();

    let mut parts: Vec<String> = Vec::new();
    for m in re_quoted.find_iter(q) {
        let tok = m.as_str();
        if tok.starts_with('"') {
            parts.push(tok.to_string());
        } else {
            let safe = re_bare.replace_all(tok, "").to_string();
            if !safe.is_empty() {
                parts.push(safe);
            }
        }
    }

    if parts.is_empty() { return String::new(); }

    let last = parts.len() - 1;
    parts.iter().enumerate().map(|(i, p)| {
        if i == last && !p.starts_with('"') {
            format!("{}*", p)
        } else {
            p.clone()
        }
    }).collect::<Vec<_>>().join(" AND ")
}
