# Conv Browser

A native desktop app for browsing Claude Code conversation history. Reads the JSONL session files that Claude Code writes to `~/.claude/projects/`, indexes them into SQLite FTS5, and presents them in a searchable, navigable UI.

Built with **PyWebView** (native GTK window) + **FastHTML** (Python web framework). Linux only — PyWebView uses GTK/WebKit on Linux.

## What it does

- **Three navigation tabs** (in the left sidebar): Timeline, Projects, Plans
  - **Timeline** — conversations grouped by last activity: Today, Yesterday, This week, This month, Older
  - **Projects** — collapsed project folders (`/home/giedrius`, `/home/giedrius/homelab`, etc.), expand to see sessions inside
  - **Plans** — browses `~/.claude/plans/*.md` plan files written by Claude during planning mode
- **Context-aware search** — searches conversations when in Timeline/Projects, searches plan content when in Plans
- **Full conversation view** — renders the back-and-forth with role labels, timestamps, copy buttons per message, copy-all button
- **Resumed sessions** — detects sessions with >2h gaps (Claude `/resume` appends to the same file), shows a gap indicator inline
- **Automated session filtering** — n8n monitoring calls and aborted sessions hidden by default, expandable via a chip at the bottom of the sidebar
- **Plans rendered as markdown** — via `marked.js` + `highlight.js` loaded from CDN

## File locations

| Path | Purpose |
|------|---------|
| `~/conv-browser.py` | The entire app — single file |
| `~/.cache/conv-browser/index.sqlite` | SQLite FTS5 index (auto-created) |
| `~/.claude/projects/` | Claude Code session source data (read-only) |
| `~/.claude/plans/` | Claude plan markdown files (read-only) |
| `~/bin/conv-browser` | Shell launcher script |
| `~/.local/share/applications/conv-browser.desktop` | Desktop entry for app grid |

## Install

```bash
pip install python-fasthtml 'pywebview[gtk]' --break-system-packages
# If GTK WebKit is missing:
sudo apt install gir1.2-webkit2-4.1 python3-gi python3-gi-cairo
```

## Launch

```bash
conv-browser          # via ~/bin/conv-browser on PATH
# or
python3 ~/conv-browser.py
# or search "Conv Browser" in GNOME app grid
```

## Architecture

```
startup
  init_db()                  create SQLite schema + migrate existing DBs
  build_or_refresh_index()   incremental indexer (see Indexing below)
  pick_port()                random free localhost port
  uvicorn.run(app)           FastHTML server in daemon thread
  webview.create_window()    native GTK window wrapping the local server
  webview.start()            blocks main thread (GTK requirement)
```

### Indexing (three-layer cache)

Every startup runs `build_or_refresh_index()`:

1. **`last_scan_at`** — timestamp stored in `meta` table. Files with `mtime < last_scan_at` are skipped before any DB work — no stat needed.
2. **Bulk mtime check** — one `SELECT path, mtime, size FROM files` loads the entire known-file index into a Python dict. No per-file DB queries.
3. **Incremental append** — if `new_size > cached_size`, open the file at `cached_size` byte offset, parse only new lines, insert only new messages, update session metadata. JSONL files are append-only so this is always correct. A file that shrank (corrupt/overwritten) falls back to full re-parse.

This means indexing the current active session costs only the bytes since last refresh — not a full 24MB re-parse.

### Session parsing

```python
# Skip tool-result echoes (user rows where content is a list, not a string)
if t == "user" and isinstance(content, list): continue

# Skip assistant turns with no renderable text (tool_use/thinking only)
if not extract_text(content): continue

# Skip system-injected user messages for title extraction
SYSTEM_PREFIXES = ("<local-command", "<system", "You are a", ...)
```

Session title = first user message that doesn't start with a system prefix. If none exists (automated/aborted sessions), falls back to the first meaningful assistant line.

### Automated session detection

Sessions tagged `is_automated=1` if:
- No user message passes the system-prefix filter (all system-injected), OR
- First real user message starts with `host:`, `container:`, `disk:`, `temp:`, `backup:` (n8n monitoring payloads)

Automated sessions hidden by default in sidebar, accessible via `+ N automated sessions` chip.

### Database schema

```sql
files    (path, mtime, size, session_id)
sessions (session_id, file_path, project_dir, cwd, started_at, ended_at,
          msg_count, first_user_text, is_resumed, is_automated)
messages (id, session_id, seq, role, ts, text)
msg_fts  VIRTUAL TABLE fts5(text, session_id UNINDEXED, seq UNINDEXED)
meta     (key, value)   -- stores last_scan_at
```

## Routes

| Route | Description |
|-------|-------------|
| `GET /` | Full app shell |
| `GET /drawer?by=timeline&auto=0` | Sidebar — timeline view |
| `GET /drawer?by=projects&auto=0` | Sidebar — projects view (all collapsed) |
| `GET /session/{sid}` | Conversation view |
| `GET /search?q=...` | FTS5 conversation search |
| `GET /plans/sidebar` | Sidebar — plans list |
| `GET /plans/{slug}` | Plan markdown view |
| `GET /search/plans?q=...` | Plan content search |
| `POST /refresh` | Re-run indexer, return updated sidebar |

## Key dependencies

```
python-fasthtml   FastHTML/HTMX web framework
pywebview[gtk]    Native window (GTK/WebKit on Linux)
uvicorn           ASGI server (already installed with fasthtml)
sqlite3           stdlib — FTS5 virtual tables
```

CDN loaded at runtime (requires internet on first use, cached by WebKit after):
- `htmx.org` — HTMX for partial page updates
- `fonts.googleapis.com` — Syne, JetBrains Mono, Instrument Sans
- `highlight.js` — code block syntax highlighting in plan view
- `marked.js` — markdown rendering in plan view
