use serde_json::Value;

pub const SYSTEM_PREFIXES: &[&str] = &[
    "<local-command", "<system", "<command-name", "<user-prompt",
    "You are a", "You are an", "I am a", "I am an",
    "you are a", "you are an",
    "As a homelab", "As an AI",
];

pub const AUTOMATED_PREFIXES: &[&str] = &[
    "host:", "container:", "disk:", "temp:", "backup:",
];

#[derive(Debug, Clone)]
pub struct Message {
    pub seq: i64,
    pub role: String,
    pub ts: String,
    pub text: String,
}

#[derive(Debug)]
pub struct SessionMeta {
    pub session_id: String,
    pub file_path: String,
    pub project_dir: String,
    pub cwd: String,
    pub started_at: String,
    pub ended_at: String,
    pub msg_count: i64,
    pub first_user_text: String,
    pub is_resumed: i64,
    pub is_automated: i64,
}

pub fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type")?.as_str()? == "text" {
                    let t = b.get("text")?.as_str()?.trim();
                    if t.is_empty() { None } else { Some(t.to_string()) }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

pub fn is_system_injected(text: &str) -> bool {
    SYSTEM_PREFIXES.iter().any(|p| text.starts_with(p))
}

fn clean_assistant_title(text: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") || line == "{" || line == "}" {
            continue;
        }
        let stripped = line.trim_start_matches('#').trim_matches(|c| c == ' ' || c == '*' || c == '_' || c == '`');
        if stripped.len() > 8 {
            return stripped.chars().take(120).collect();
        }
    }
    text.chars().take(120).collect::<String>().replace('\n', " ")
}

pub fn parse_session(path: &std::path::Path) -> Option<(SessionMeta, Vec<Message>)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut messages: Vec<Message> = Vec::new();
    let mut cwd: Option<String> = None;
    let mut seq: i64 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let t = match obj.get("type").and_then(|v| v.as_str()) {
            Some(t) if t == "user" || t == "assistant" => t.to_string(),
            _ => continue,
        };
        let content_val = obj
            .get("message")
            .and_then(|m| m.get("content"))
            .cloned()
            .unwrap_or(Value::Null);

        // Skip tool-result echoes (user rows with array content)
        if t == "user" && content_val.is_array() {
            continue;
        }

        let text = extract_text(&content_val);
        if text.is_empty() { continue; }

        let ts = obj.get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if cwd.is_none() {
            cwd = obj.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
        }

        messages.push(Message { seq, role: t, ts, text });
        seq += 1;
    }

    if messages.is_empty() {
        return None;
    }

    // First meaningful user message
    let first_user_text = messages.iter()
        .find(|m| m.role == "user" && !is_system_injected(&m.text))
        .map(|m| m.text.chars().take(120).collect::<String>().replace('\n', " "))
        .unwrap_or_else(|| {
            messages.iter()
                .find(|m| m.role == "assistant")
                .map(|m| clean_assistant_title(&m.text))
                .unwrap_or_default()
        });

    // Detect resumed (>2h gap)
    let is_resumed = detect_resumed(&messages);

    // Detect automated
    let has_human_user = messages.iter()
        .any(|m| m.role == "user" && !is_system_injected(&m.text));
    let first_real_user = messages.iter()
        .find(|m| m.role == "user" && !is_system_injected(&m.text))
        .map(|m| m.text.as_str())
        .unwrap_or("");
    let is_automated = if !has_human_user
        || AUTOMATED_PREFIXES.iter().any(|p| first_real_user.starts_with(p))
    { 1 } else { 0 };

    let session_id = path.file_stem()?.to_str()?.to_string();
    let project_dir = path.parent()?.file_name()?.to_str()?.to_string();
    let started_at = messages.first().map(|m| m.ts.clone()).unwrap_or_default();
    let ended_at = messages.last().map(|m| m.ts.clone()).unwrap_or_default();

    Some((
        SessionMeta {
            session_id,
            file_path: path.to_string_lossy().into_owned(),
            project_dir,
            cwd: cwd.unwrap_or_default(),
            started_at,
            ended_at,
            msg_count: messages.len() as i64,
            first_user_text,
            is_resumed,
            is_automated,
        },
        messages,
    ))
}

fn detect_resumed(messages: &[Message]) -> i64 {
    let mut prev_dt: Option<chrono::DateTime<chrono::Utc>> = None;
    for m in messages {
        if let Some(dt) = parse_ts(&m.ts) {
            if let Some(prev) = prev_dt {
                if (dt - prev).num_seconds() > 7200 {
                    return 1;
                }
            }
            prev_dt = Some(dt);
        }
    }
    0
}

pub fn parse_ts(ts: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if ts.is_empty() { return None; }
    ts.parse::<chrono::DateTime<chrono::Utc>>().ok()
        .or_else(|| chrono::DateTime::parse_from_rfc3339(ts).ok().map(|dt| dt.with_timezone(&chrono::Utc)))
}
