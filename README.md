# conv-browser

Browse your [Claude Code](https://claude.ai/code) conversation history in a desktop window.

![conv-browser screenshot](https://github.com/sinful1992/conv-browser/releases/download/v0.2.0/screenshot.png)

## Features

- Full-text search across all conversations and plans
- Browse individual sessions with message threading
- View plans (thinking blocks) inline
- Keyboard-friendly navigation

## Installation

Download the latest release for your platform from the [Releases page](https://github.com/sinful1992/conv-browser/releases):

| Platform | File |
|----------|------|
| Linux (Debian/Ubuntu) | `.deb` |
| Linux (universal) | `.AppImage` |
| Windows | `.msi` or `.exe` |

### Linux `.deb`

```sh
sudo dpkg -i conv-browser_*.deb
```

### Linux `.AppImage`

```sh
chmod +x conv-browser_*.AppImage
./conv-browser_*.AppImage
```

## Building from source

Requires: [Rust](https://rustup.rs) stable, [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/)

```sh
git clone https://github.com/sinful1992/conv-browser.git
cd conv-browser
cargo tauri build
```

The built binary is at `target/release/conv-browser`.

For a quick dev run without bundling:

```sh
cargo run
```

## How it works

conv-browser scans `~/.claude/projects/` for Claude Code JSONL session files, indexes them into a local SQLite cache at `~/.cache/conv-browser/index.sqlite`, and serves a local HTTP interface rendered in a Tauri webview. The index is rebuilt incrementally on each launch.

## Tech stack

- **Backend:** Rust + [axum](https://github.com/tokio-rs/axum)
- **Frontend:** HTML/CSS/JS + [HTMX](https://htmx.org)
- **Desktop shell:** [Tauri 2](https://tauri.app)
- **Storage:** SQLite (via rusqlite) with FTS5 full-text search

## License

MIT
