# Total Recall

Your complete Claude Code memory — every conversation, every plan, every exchange — browsable and free.

![Total Recall screenshot](assets/screenshot.webp)

Memory tools like MemPalace try to decide what matters. They burn tokens on every message mining your history, and still miss half of what you actually need.

Total Recall stores everything and costs nothing to access. Browse your full history, search across all sessions, read plans inline — then bring exactly what's relevant into context yourself. No hooks, no background calls, no token overhead.

## Features

### Browsing
- **Timeline view** — conversations grouped into Today, Yesterday, This Week, This Month, Older
- **Projects view** — conversations grouped by working directory
- **Starred sessions** — star any conversation; starred sessions are pinned at the top of the timeline
- **Automated sessions** — hidden by default; toggle to show/hide with a count badge
- **Older sessions pagination** — the Older group loads 30 at a time to keep the sidebar fast

### Search
- **Full-text search** — FTS5 search across all conversations with highlighted snippets and total match count
- **Scroll to match** — clicking a search result opens the conversation and jumps to the matching message
- **Plans search** — separate search tab for `~/.claude/plans/` markdown files with highlighted snippets

### Conversation view
- **Session referencing** — every conversation gets a stable `#N` number and a human-readable codename (e.g. `#42 · amber-falcon`), shown in both the sidebar and the detail view and included in exported Markdown — use these to refer to a session unambiguously across notes or with Claude
- **Session metadata** — start date, working directory, message count
- **Resumed indicator** — sessions with a >2h gap between messages are labelled "resumed"; gap markers appear inline in the transcript
- **Per-session notes** — add private notes to any conversation, autosaved
- **Export as Markdown** — download any session as a `.md` file with full metadata header
- **Copy all** — copy the entire conversation transcript to the clipboard
- **Copy individual messages** — per-message copy button

### Plans
- **Browse plans** — list all `~/.claude/plans/*.md` files sorted by recency, with line count
- **Rendered markdown** — plans are rendered as formatted markdown in the detail view
- **Copy raw** — copy the raw markdown source of any plan

### Data & privacy
- **Persistent archive** — sessions stay in the local SQLite index even after Claude prunes the source JSONL files (30-day window)
- **Incremental indexing** — only changed or new files are re-parsed on startup
- **Live re-index** — new conversations appear automatically as Claude writes them
- **In-app auto-update** — notified when a new version is available; installs with one click
- **Completely passive** — no writes to your Claude data, ever; subagent sessions are excluded automatically

## Installation

Download the latest release from the [Releases page](https://github.com/sinful1992/total-recall/releases):

| Platform | File |
|----------|------|
| Linux (Debian/Ubuntu) | `.deb` |
| Linux (universal) | `.AppImage` |
| Windows | `.exe` (NSIS installer) |

### Linux `.deb`

```sh
sudo dpkg -i total-recall_*.deb
```

### Linux `.AppImage`

```sh
chmod +x total-recall_*.AppImage
./total-recall_*.AppImage
```

### Windows

Run the `.exe` installer. The binary is Authenticode-signed via [SignPath Foundation](https://signpath.org), so Windows SmartScreen will recognise it. If you are on an older release (pre-v1.4.6) and see a SmartScreen warning, click **"More info" → "Run anyway"** — then update to the latest version in-app.

## In-app updates

From v1.4.6 onward, Total Recall checks for new releases at startup. When one is available, a banner appears at the top of the window:

> ↑ v1.x.x available — **Install & Restart** / ✕

Clicking **Install & Restart** downloads the new installer, verifies its signature, runs it silently, and restarts the app. No browser or manual download needed.

> **Note:** Versions before v1.4.6 had a bug where the Authenticode-signed installer and the Tauri update signature were generated from different binaries, causing the update to fail. Install v1.4.6 manually once; auto-update will work for all future releases.

## Building from source

Requires: [Rust](https://rustup.rs) stable, [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/)

```sh
git clone https://github.com/sinful1992/total-recall.git
cd total-recall
cargo tauri build
```

For a quick dev run without bundling:

```sh
cargo run
```

## How it works

Total Recall scans `~/.claude/projects/` for Claude Code JSONL session files and indexes them into a local SQLite cache at `~/.cache/total-recall/index.sqlite` (Windows: `%LOCALAPPDATA%\total-recall\index.sqlite`). The index is rebuilt incrementally on startup, then kept live via a filesystem watcher that picks up new conversations as Claude writes them.

## Tech stack

- **Backend:** Rust + [axum](https://github.com/tokio-rs/axum)
- **Frontend:** HTML/CSS/JS + [HTMX](https://htmx.org)
- **Desktop shell:** [Tauri 2](https://tauri.app)
- **Storage:** SQLite (via rusqlite) with FTS5 full-text search

## License

MIT
