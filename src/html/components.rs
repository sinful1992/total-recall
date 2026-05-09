use maud::{html, Markup, PreEscaped};
use rusqlite::Row;
use crate::helpers::{
    fmt_rel, fmt_date, fmt_ts_short, gap_label, cwd_label, density_pct,
    date_bucket, BUCKET_ORDER, session_codename,
};
use crate::parser::parse_ts;

pub fn welcome() -> Markup {
    html! {
        div.welcome {
            div.welcome-icon { "\u{25C8}" }
            p.welcome-text { "Select a conversation" }
            p.welcome-sub { "or search above to find something" }
        }
    }
}

pub fn session_item_html(
    session_id: &str,
    first_user_text: &str,
    ended_at: &str,
    msg_count: i64,
    is_resumed: i64,
    ref_num: i64,
    active_sid: Option<&str>,
) -> Markup {
    let rel_time = fmt_rel(ended_at);
    let density = density_pct(msg_count);
    let is_active = active_sid == Some(session_id);
    let cls = if is_active { "s-item active" } else { "s-item" };
    let title = if first_user_text.is_empty() { "—" } else { first_user_text };

    let codename = session_codename(session_id);

    html! {
        div class=(cls)
            data-sid=(session_id)
            hx-get=(format!("/session/{}", session_id))
            hx-target="#main"
            hx-push-url="true"
            onclick=(format!("activateSession('{}')", session_id))
        {
            p.s-title { (title) }
            div.s-codename { (codename) }
            div.s-foot {
                span.s-ref { (format!("#{}", ref_num)) }
                span.s-time { (rel_time) }
                div.s-density {
                    div.s-density-fill style=(format!("width:{}%", density)) {}
                }
                span.s-badge { (msg_count) }
                @if is_resumed != 0 {
                    span.s-resumed { "resumed" }
                }
            }
        }
    }
}

pub struct SessionRow {
    pub session_id: String,
    pub first_user_text: String,
    pub ended_at: String,
    pub msg_count: i64,
    pub is_resumed: i64,
    pub is_automated: i64,
    pub cwd: String,
    pub project_dir: String,
    pub ref_num: i64,
    pub ref_code: String,
}

impl SessionRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            session_id: row.get("session_id")?,
            first_user_text: row.get::<_, Option<String>>("first_user_text")?.unwrap_or_default(),
            ended_at: row.get::<_, Option<String>>("ended_at")?.unwrap_or_default(),
            msg_count: row.get("msg_count")?,
            is_resumed: row.get("is_resumed")?,
            is_automated: row.get("is_automated")?,
            cwd: row.get::<_, Option<String>>("cwd")?.unwrap_or_default(),
            project_dir: row.get::<_, Option<String>>("project_dir")?.unwrap_or_default(),
            ref_num: row.get::<_, Option<i64>>("ref_num")?.unwrap_or(0),
            ref_code: row.get::<_, Option<String>>("ref_code")?.unwrap_or_default(),
        })
    }
}

pub fn drawer_timeline_html(sessions: &[SessionRow], auto_count: i64, show_automated: bool) -> Markup {
    // Group into buckets
    let mut buckets: std::collections::HashMap<&str, Vec<&SessionRow>> = Default::default();
    for s in sessions {
        let b = date_bucket(&s.ended_at);
        buckets.entry(b).or_default().push(s);
    }

    let auto_chip = if auto_count > 0 {
        if show_automated {
            html! {
                div.auto-chip.active
                    hx-get="/drawer?by=timeline&auto=0"
                    hx-target="#sidebar"
                    hx-swap="outerHTML"
                {
                    span style="flex:1" { (format!("Hiding {} automated", auto_count)) }
                }
            }
        } else {
            html! {
                div.auto-chip
                    hx-get="/drawer?by=timeline&auto=1"
                    hx-target="#sidebar"
                    hx-swap="outerHTML"
                {
                    span { (format!("+ {} automated sessions", auto_count)) }
                }
            }
        }
    } else {
        html! {}
    };

    let auto_param = if show_automated { "1" } else { "0" };

    html! {
        div id="sidebar" {
            @for &bkt in BUCKET_ORDER {
                @if let Some(items) = buckets.get(bkt) {
                    @if !items.is_empty() {
                        @if bkt == "Older" {
                            (drawer_group_older(items, auto_param))
                        } @else {
                            (drawer_group(bkt, items, bkt == "Today" || bkt == "Yesterday"))
                        }
                    }
                }
            }
            (auto_chip)
        }
    }
}

pub fn drawer_projects_html(sessions: &[SessionRow], home: &str) -> Markup {
    // Group by cwd/project_dir
    let mut projects: indexmap::IndexMap<String, Vec<&SessionRow>> = Default::default();
    for s in sessions {
        let key = if !s.cwd.is_empty() { s.cwd.clone() } else if !s.project_dir.is_empty() { s.project_dir.clone() } else { "unknown".to_string() };
        projects.entry(key).or_default().push(s);
    }
    // Sort by most recent session
    let mut sorted: Vec<(String, Vec<&SessionRow>)> = projects.into_iter().collect();
    sorted.sort_by(|a, b| {
        let ta = a.1.first().map(|s| s.ended_at.as_str()).unwrap_or("");
        let tb = b.1.first().map(|s| s.ended_at.as_str()).unwrap_or("");
        tb.cmp(ta)
    });

    html! {
        div id="sidebar" {
            @for (i, (proj, items)) in sorted.iter().enumerate() {
                (proj_group(&format!("proj_{}", i), &cwd_label(proj, home), items))
            }
        }
    }
}

fn drawer_group_older(items: &[&SessionRow], auto_param: &str) -> Markup {
    const PAGE: usize = 30;
    let total = items.len();
    let visible = &items[..total.min(PAGE)];
    let has_more = total > PAGE;

    html! {
        div.drawer-group {
            div.drawer-hd onclick="toggleDrawer('Older')" {
                span.drawer-arr id="da-Older" { "\u{25B6}" }
                span.drawer-hd-label { "Older" }
                span.drawer-count { (total) }
            }
            div.drawer-body id="db-Older" {
                @for s in visible {
                    (session_item_html(&s.session_id, &s.first_user_text, &s.ended_at, s.msg_count, s.is_resumed, s.ref_num, None))
                }
                @if has_more {
                    button.load-more-btn
                        hx-get=(format!("/drawer/older-items?offset={}&auto={}", PAGE, auto_param))
                        hx-target="this"
                        hx-swap="outerHTML"
                    {
                        (format!("Load {} more", (total - PAGE).min(PAGE)))
                    }
                }
            }
        }
    }
}

fn drawer_group(bkt: &str, items: &[&SessionRow], is_open: bool) -> Markup {
    let gid = bkt.replace(' ', "_");
    let arrow = if is_open { "\u{25BC}" } else { "\u{25B6}" };
    let body_cls = if is_open { "drawer-body open" } else { "drawer-body" };

    html! {
        div.drawer-group {
            div.drawer-hd onclick=(format!("toggleDrawer('{}')", gid)) {
                span.drawer-arr id=(format!("da-{}", gid)) { (arrow) }
                span.drawer-hd-label { (bkt) }
                span.drawer-count { (items.len()) }
            }
            div class=(body_cls) id=(format!("db-{}", gid)) {
                @for s in items {
                    (session_item_html(&s.session_id, &s.first_user_text, &s.ended_at, s.msg_count, s.is_resumed, s.ref_num, None))
                }
            }
        }
    }
}

fn proj_group(gid: &str, label: &str, items: &[&SessionRow]) -> Markup {
    html! {
        div.drawer-group {
            div.drawer-hd onclick=(format!("toggleDrawer('{}')", gid)) {
                span.drawer-arr id=(format!("da-{}", gid)) { "\u{25B6}" }
                span.drawer-hd-label { (label) }
                span.drawer-count { (items.len()) }
            }
            div.drawer-body id=(format!("db-{}", gid)) {
                @for s in items {
                    (session_item_html(&s.session_id, &s.first_user_text, &s.ended_at, s.msg_count, s.is_resumed, s.ref_num, None))
                }
            }
        }
    }
}

pub struct MsgRow {
    pub seq: i64,
    pub role: String,
    pub ts: String,
    pub text: String,
}

impl MsgRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            seq: row.get("seq")?,
            role: row.get("role")?,
            ts: row.get::<_, Option<String>>("ts")?.unwrap_or_default(),
            text: row.get("text")?,
        })
    }
}

pub fn session_view_html(
    session_id: &str,
    first_user_text: &str,
    started_at: &str,
    ended_at: &str,
    msg_count: i64,
    is_resumed: i64,
    cwd: &str,
    home: &str,
    messages: &[MsgRow],
    scroll_to_seq: Option<i64>,
    ref_num: i64,
    notes: &str,
) -> Markup {
    let title = if first_user_text.is_empty() { "Conversation" } else { first_user_text };
    let started = fmt_date(started_at);
    let cwd_disp = cwd_label(cwd, home);
    let codename = session_codename(session_id);

    let scroll_script = scroll_to_seq.map(|seq| {
        html! { script { (PreEscaped(format!("scrollToSearchResult({});", seq))) } }
    });

    html! {
        div id="main" {
            div id="main-inner" {
                div.sess-hd {
                    button.back-btn onclick="goBack()" { "\u{2190} back" }
                    h2.sess-hd-title { (title) }
                    div.sess-hd-meta {
                        span.sess-meta-chip.ref-chip { (format!("#{}", ref_num)) }
                        span.sess-meta-chip.ref-chip { (codename) }
                        span.sess-meta-chip { (started) }
                        span.sess-meta-chip { (cwd_disp) }
                        span.sess-meta-chip { (format!("{} messages", msg_count)) }
                        @if is_resumed != 0 {
                            span.sess-meta-chip.accent { (format!("resumed · last {}", fmt_rel(ended_at))) }
                        }
                    }
                    div.sess-hd-actions {
                        button.copy-all-btn onclick="copyAll(this)" { "copy all" }
                        button.export-btn onclick=(format!("exportMarkdown('{}', '{}')", session_id, codename)) { "export md" }
                    }
                    div.sess-notes {
                        textarea.notes-input
                            id="notes-input"
                            name="notes"
                            placeholder="Add notes about this conversation\u{2026}"
                            hx-post=(format!("/session/{}/notes", session_id))
                            hx-trigger="input changed delay:800ms"
                            hx-target="#notes-status"
                            hx-include="#notes-input"
                        { (notes) }
                        span.notes-status id="notes-status" {}
                    }
                }
                (render_messages_proper(messages))
                @if let Some(s) = scroll_script { (s) }
            }
        }
    }
}

fn render_messages_proper(messages: &[MsgRow]) -> Markup {
    let mut parts = Vec::new();
    let mut prev_ts: Option<&str> = None;

    for m in messages {
        if let Some(pt) = prev_ts {
            if let (Some(a), Some(b)) = (parse_ts(pt), parse_ts(&m.ts)) {
                if (b - a).num_seconds() > 7200 {
                    let label = gap_label(pt, &m.ts);
                    parts.push(html! {
                        div.msg-gap {
                            hr.msg-gap-line {}
                            span.msg-gap-label { (label) }
                            hr.msg-gap-line {}
                        }
                    });
                }
            }
        }
        parts.push(render_message(m));
        prev_ts = Some(&m.ts);
    }

    html! { @for p in &parts { (p) } }
}

fn render_message(m: &MsgRow) -> Markup {
    let is_user = m.role == "user";
    let msg_cls = if is_user { "msg msg-user" } else { "msg msg-asst" };
    let role_label = if is_user { "USER" } else { "Claude" };
    let ts_label = fmt_ts_short(&m.ts);
    let data_text = serde_json::to_string(&m.text).unwrap_or_default();
    let onclick = format!("copyMsg(this, {})", data_text);

    html! {
        div class=(msg_cls) id=(format!("msg-{}", m.seq)) {
            div.msg-role-col {
                span.msg-role-label { (role_label) }
                span.msg-ts { (ts_label) }
            }
            div.msg-content-col {
                pre.msg-body { (m.text) }
                div.msg-actions {
                    button.copy-btn onclick=(onclick) { "copy" }
                }
            }
        }
    }
}
