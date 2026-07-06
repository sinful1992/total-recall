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
    /// Override used for FTS indexing; if None, `text` is indexed directly.
    pub fts_text: Option<String>,
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
    pub is_subagent: i64,
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

/// Parse a single decoded JSONL object into 0 or more Messages.
/// Advances `seq` for each Message produced.
/// Returns (messages, Option<cwd>) — cwd is present on lines that carry it.
pub fn parse_jsonl_obj(obj: &Value, seq: &mut i64) -> (Vec<Message>, Option<String>) {
    let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());

    let t = match obj.get("type").and_then(|v| v.as_str()) {
        Some(t) if t == "user" || t == "assistant" => t.to_string(),
        _ => return (vec![], cwd),
    };

    let content_val = obj
        .get("message")
        .and_then(|m| m.get("content"))
        .cloned()
        .unwrap_or(Value::Null);

    let ts = obj.get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut messages = Vec::new();

    match t.as_str() {
        "user" => {
            // Skip tool_result echoes (user rows with array content)
            if content_val.is_array() {
                return (vec![], cwd);
            }
            let text = extract_text(&content_val);
            // Skip system-injected messages (CLI scaffolding like <local-command-caveat>, /clear, etc.)
            if text.is_empty() || is_system_injected(&text) {
                return (vec![], cwd);
            }
            messages.push(Message {
                seq: *seq,
                role: "user".to_string(),
                ts,
                text,
                fts_text: None,
            });
            *seq += 1;
        }
        "assistant" => {
            // Extract text blocks first
            let text = extract_text(&content_val);
            if !text.is_empty() {
                messages.push(Message {
                    seq: *seq,
                    role: "assistant".to_string(),
                    ts: ts.clone(),
                    text,
                    fts_text: None,
                });
                *seq += 1;
            }
            // Then extract tool_use blocks
            if let Some(arr) = content_val.as_array() {
                for block in arr {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        if let Some(msg) = tool_use_to_message(block, &ts, *seq) {
                            messages.push(msg);
                            *seq += 1;
                        }
                    }
                }
            }
        }
        _ => {}
    }

    (messages, cwd)
}

/// Convert a tool_use JSON block into a Message with role="tool_use".
/// The text field is a compact JSON object for rendering.
fn tool_use_to_message(block: &Value, ts: &str, seq: i64) -> Option<Message> {
    let name = block.get("name")?.as_str()?;
    let input = block.get("input").cloned().unwrap_or(Value::Null);

    let (text, fts_text) = build_tool_use_text(name, &input);

    Some(Message {
        seq,
        role: "tool_use".to_string(),
        ts: ts.to_string(),
        text,
        fts_text: Some(fts_text),
    })
}

fn build_tool_use_text(name: &str, input: &Value) -> (String, String) {
    match name {
        "Edit" => {
            let file = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let old = input.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new_s = input.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            let diff = make_edit_diff(old, new_s, file);
            let json = serde_json::json!({"n":"Edit","f":file,"d":diff}).to_string();
            (json, format!("Edit {}", file))
        }
        "Write" => {
            let file = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let diff = make_write_diff(content, file);
            let json = serde_json::json!({"n":"Write","f":file,"d":diff}).to_string();
            (json, format!("Write {}", file))
        }
        "Read" => {
            let file = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let offset = input.get("offset").and_then(|v| v.as_i64());
            let limit = input.get("limit").and_then(|v| v.as_i64());
            let mut extras = String::new();
            if let Some(o) = offset { extras.push_str(&format!(" offset={}", o)); }
            if let Some(l) = limit { extras.push_str(&format!(" limit={}", l)); }
            let json = serde_json::json!({"n":"Read","f":file,"extras":extras}).to_string();
            (json, format!("Read {}", file))
        }
        "Bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let desc = input.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let json = serde_json::json!({"n":"Bash","cmd":cmd,"desc":desc}).to_string();
            let fts = if !desc.is_empty() {
                format!("Bash: {}", desc)
            } else {
                // chars(), not a byte slice — a multibyte char at the cut
                // point would panic and kill indexing.
                format!("Bash: {}", cmd.chars().take(120).collect::<String>())
            };
            (json, fts)
        }
        _ => {
            // Generic: extract string fields, truncate to 300 chars each
            let mut args = serde_json::Map::new();
            if let Some(obj) = input.as_object() {
                for (k, v) in obj {
                    match v {
                        Value::String(s) => {
                            args.insert(k.clone(), Value::String(s.chars().take(300).collect()));
                        }
                        Value::Bool(b) => {
                            args.insert(k.clone(), Value::Bool(*b));
                        }
                        Value::Number(n) => {
                            args.insert(k.clone(), Value::Number(n.clone()));
                        }
                        _ => {}
                    }
                }
            }
            let json = serde_json::json!({"n":name,"args":Value::Object(args)}).to_string();
            (json, format!("Tool: {}", name))
        }
    }
}

/// Build a simple unified diff showing old_string as deletions and new_string as additions.
fn make_edit_diff(old: &str, new_s: &str, file: &str) -> String {
    const MAX: usize = 200;
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new_s.lines().collect();

    let show_old = &old_lines[..old_lines.len().min(MAX)];
    let show_new = &new_lines[..new_lines.len().min(MAX)];

    let mut out = format!("--- {}\n+++ {}\n", file, file);
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        1, show_old.len(),
        1, show_new.len()
    ));
    for l in show_old { out.push_str(&format!("-{}\n", l)); }
    for l in show_new { out.push_str(&format!("+{}\n", l)); }
    if old_lines.len() > MAX || new_lines.len() > MAX {
        out.push_str("... (truncated)\n");
    }
    out
}

/// Build a unified diff showing all content lines as additions.
fn make_write_diff(content: &str, file: &str) -> String {
    const MAX: usize = 300;
    let lines: Vec<&str> = content.lines().collect();
    let show = &lines[..lines.len().min(MAX)];

    let mut out = format!("--- /dev/null\n+++ {}\n", file);
    out.push_str(&format!("@@ -0,0 +1,{} @@\n", show.len()));
    for l in show { out.push_str(&format!("+{}\n", l)); }
    if lines.len() > MAX {
        out.push_str(&format!("... ({} more lines truncated)\n", lines.len() - MAX));
    }
    out
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

        let (new_msgs, line_cwd) = parse_jsonl_obj(&obj, &mut seq);
        if cwd.is_none() { cwd = line_cwd; }
        messages.extend(new_msgs);
    }

    if messages.is_empty() {
        return None;
    }

    // First meaningful user message (for session title)
    let first_user_text = messages.iter()
        .find(|m| m.role == "user" && !is_system_injected(&m.text))
        .map(|m| m.text.chars().take(120).collect::<String>().replace('\n', " "))
        .unwrap_or_else(|| {
            messages.iter()
                .find(|m| m.role == "assistant")
                .map(|m| clean_assistant_title(&m.text))
                .unwrap_or_default()
        });

    // Detect resumed (>2h gap between any two consecutive messages)
    let is_resumed = detect_resumed(&messages);

    // Detect automated (no human user messages, or starts with automated prefix)
    let has_human_user = messages.iter()
        .any(|m| m.role == "user" && !is_system_injected(&m.text));
    let first_real_user = messages.iter()
        .find(|m| m.role == "user" && !is_system_injected(&m.text))
        .map(|m| m.text.as_str())
        .unwrap_or("");
    let is_automated = if !has_human_user
        || AUTOMATED_PREFIXES.iter().any(|p| first_real_user.starts_with(p))
    { 1 } else { 0 };

    // msg_count: only count user/assistant turns (not tool_use)
    let msg_count = messages.iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .count() as i64;

    let session_id = path.file_stem()?.to_str()?.to_string();
    let project_dir = path.parent()?.file_name()?.to_str()?.to_string();
    let is_subagent = if path.components().any(|c| c.as_os_str() == "subagents") { 1 } else { 0 };
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
            msg_count,
            first_user_text,
            is_resumed,
            is_automated,
            is_subagent,
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

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: a multibyte char straddling byte 120 must not panic the
    // Bash fts truncation (this crashed indexing → app failed to launch).
    #[test]
    fn bash_fts_truncation_is_char_safe() {
        // 119 ASCII bytes then a 3-byte char: byte 120 is mid-char.
        let cmd = format!("{}€ and some trailing text to exceed the limit", "a".repeat(119));
        let input = serde_json::json!({ "command": cmd });
        let (_, fts) = build_tool_use_text("Bash", &input);
        assert!(fts.starts_with("Bash: "));
        assert!(fts.contains('€'));
    }

    #[test]
    fn subagent_sessions_are_flagged() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("tr-parser-test-{}", std::process::id()));
        let sub = dir.join("proj").join("sess").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let path = sub.join("abcd1234.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "{}", serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": "do the thing"},
            "timestamp": "2026-01-01T10:00:00Z"
        })).unwrap();
        let (meta, _) = parse_session(&path).unwrap();
        assert_eq!(meta.is_subagent, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
