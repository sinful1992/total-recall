#!/usr/bin/env python3
"""
Conv Browser — Claude conversation history browser
Native window (PyWebView + FastHTML). Run: python3 conv-browser.py
"""

import json
import os
import platform
import re
import socket
import sqlite3
import sys
import threading
import time
from datetime import datetime, timezone, timedelta
from pathlib import Path

import uvicorn
import webview
from fasthtml.common import *

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
PROJECTS_DIR = Path("~/.claude/projects/").expanduser()
PLANS_DIR    = Path("~/.claude/plans/").expanduser()

if platform.system() == "Windows":
    CACHE_DIR = Path(os.environ.get("LOCALAPPDATA", "~")) / "conv-browser"
else:
    CACHE_DIR = Path("~/.cache/conv-browser/").expanduser()

DB_PATH = CACHE_DIR / "index.sqlite"

# Prefixes that indicate a system-injected first message, not a real user prompt
SYSTEM_PREFIXES = (
    "<local-command", "<system", "<command-name", "<user-prompt",
    "You are a", "You are an", "I am a", "I am an",
    "you are a", "you are an",
    "As a homelab", "As an AI",
)

def _clean_assistant_title(text: str) -> str:
    """Extract a readable title from an assistant message as last-resort fallback."""
    lines = text.strip().splitlines()
    for line in lines:
        line = line.strip()
        # skip code fences, json braces, blank lines
        if not line or line.startswith("```") or line in ("{", "}"):
            continue
        # strip markdown bold/italic
        line = line.lstrip("#").strip(" *_`")
        if len(line) > 8:
            return line[:120]
    return text[:120].replace("\n", " ")

def build_fts_query(q: str) -> str:
    """Convert a plain-text search query into an FTS5 MATCH expression.

    Rules:
    - Quoted phrases (e.g. "exact phrase") are passed through unchanged.
    - Bare words are AND-ed together.
    - The last bare token gets a trailing * for prefix matching (good for
      incremental / live search as the user types).
    - FTS5 special characters are stripped from bare tokens to avoid
      OperationalError on partial input.
    """
    parts: list[str] = []
    for m in re.finditer(r'"[^"]*"|\S+', q):
        tok = m.group()
        if tok.startswith('"'):
            parts.append(tok)
        else:
            safe = re.sub(r'[^\w\-]', '', tok, flags=re.UNICODE)
            if safe:
                parts.append(safe)

    if not parts:
        return ''

    result = []
    for i, p in enumerate(parts):
        if i == len(parts) - 1 and not p.startswith('"'):
            result.append(p + '*')
        else:
            result.append(p)
    return ' AND '.join(result)

# ---------------------------------------------------------------------------
# Fonts & CSS
# ---------------------------------------------------------------------------
FONT_LINK = Link(
    rel="stylesheet",
    href="https://fonts.googleapis.com/css2?family=Syne:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&family=Instrument+Sans:wght@400;500&display=swap"
)
HL_CSS  = Link(rel="stylesheet", href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css")
HL_JS   = Script(src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js")
MARKED_JS = Script(src="https://cdn.jsdelivr.net/npm/marked/marked.min.js")

CSS = """
:root {
  --bg:        #0d0c0a;
  --bg-mid:    #111009;
  --bg-el:     #191714;
  --bg-hover:  #1e1c18;
  --border:    #252219;
  --border-hi: #3a3526;
  --text:      #e0d8cc;
  --text-dim:  #6e6658;
  --text-mid:  #a89e8e;
  --accent:    #c8922a;
  --accent-lo: #2c1f08;
  --accent-hi: #e8ae50;
  --user-c:    #c8922a;
  --asst-c:    #6b9e7a;
  --asst-lo:   #0e1f14;
  --mark-bg:   #2c1f08;
  --mark-text: #e8ae50;
  --radius:    5px;
  --mono:      'JetBrains Mono', 'Cascadia Code', monospace;
  --ui:        'Instrument Sans', system-ui, sans-serif;
  --display:   'Syne', system-ui, sans-serif;
}

*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

html { height: 100%; }

body {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  color: var(--text);
  font-family: var(--ui);
  font-size: 14px;
  line-height: 1.5;
  overflow: hidden;
}

/* ── scrollbar ─────────────────────────────────────────── */
::-webkit-scrollbar { width: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border-hi); border-radius: 2px; }
::-webkit-scrollbar-thumb:hover { background: var(--text-dim); }

/* ── layout ─────────────────────────────────────────────── */
#shell {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-height: 0;
}

/* ── topbar ─────────────────────────────────────────────── */
#topbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px 0 14px;
  background: var(--bg-mid);
  border-bottom: 1px solid var(--border);
  height: 48px;
  flex-shrink: 0;
}

.wordmark {
  font-family: var(--display);
  font-size: 13px;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  white-space: nowrap;
  flex-shrink: 0;
}

#search-wrap {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
}
.search-icon {
  position: absolute;
  left: 10px;
  color: var(--text-dim);
  font-size: 12px;
  pointer-events: none;
}
#search-input {
  width: 100%;
  padding: 7px 12px 7px 30px;
  background: var(--bg-el);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  font-family: var(--ui);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}
#search-input::placeholder { color: var(--text-dim); }
#search-input:focus { border-color: var(--border-hi); }

#refresh-btn {
  padding: 5px 9px;
  background: var(--bg-el);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-dim);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  transition: all 0.12s;
  flex-shrink: 0;
}
#refresh-btn:hover { color: var(--accent); border-color: var(--border-hi); }

/* ── left panel (nav + sidebar content) ─────────────────── */
#left-panel {
  width: 280px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-mid);
  border-right: 1px solid var(--border);
  overflow: hidden;
}

/* ── sidebar nav tabs ────────────────────────────────────── */
#sidenav {
  display: flex;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.snav-btn {
  flex: 1;
  padding: 9px 4px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-dim);
  cursor: pointer;
  font-family: var(--display);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.snav-btn:last-child { border-right: none; }
.snav-btn:hover:not(.active) { color: var(--text-mid); background: var(--bg-hover); }
.snav-btn.active { color: var(--accent); background: var(--bg-el); }

/* ── sidebar content (scrollable area below nav) ─────────── */
#sidebar {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
}

.drawer-group { }

.drawer-hd {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 10px 12px 6px;
  cursor: pointer;
  user-select: none;
  position: sticky;
  top: 0;
  background: var(--bg-mid);
  z-index: 1;
}
.drawer-hd-label {
  font-family: var(--display);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-dim);
  flex: 1;
}
.drawer-count {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  background: var(--bg-el);
  padding: 1px 5px;
  border-radius: 3px;
}
.drawer-arr {
  font-size: 8px;
  color: var(--text-dim);
  transition: transform 0.15s;
}
.drawer-hd:hover .drawer-hd-label { color: var(--text-mid); }
.drawer-hd:hover .drawer-arr { color: var(--text-mid); }

.drawer-body { display: none; }
.drawer-body.open { display: block; }

/* ── session items ───────────────────────────────────────── */
.s-item {
  padding: 8px 12px;
  cursor: pointer;
  border-left: 2px solid transparent;
  transition: background 0.1s, border-color 0.1s;
  position: relative;
}
.s-item:hover { background: var(--bg-hover); }
.s-item.active {
  background: var(--bg-hover);
  border-left-color: var(--accent);
}
.s-item + .s-item { border-top: 1px solid var(--border); }

.s-title {
  font-size: 12.5px;
  color: var(--text);
  line-height: 1.35;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  margin-bottom: 5px;
}
.s-item.active .s-title { color: var(--text); }

.s-foot {
  display: flex;
  align-items: center;
  gap: 6px;
}
.s-time {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
}
.s-badge {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  background: var(--bg-el);
  border: 1px solid var(--border);
  padding: 0 4px;
  border-radius: 3px;
}
.s-resumed {
  font-family: var(--display);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--accent);
  background: var(--accent-lo);
  border: 1px solid #3d2510;
  padding: 0 4px;
  border-radius: 3px;
}

/* message density bar */
.s-density {
  flex: 1;
  height: 3px;
  background: var(--border);
  border-radius: 2px;
  overflow: hidden;
  min-width: 20px;
  max-width: 60px;
}
.s-density-fill {
  height: 100%;
  background: var(--border-hi);
  border-radius: 2px;
  transition: background 0.1s;
}
.s-item.active .s-density-fill,
.s-item:hover .s-density-fill { background: var(--accent); opacity: 0.5; }

/* ── main pane ───────────────────────────────────────────── */
#main {
  flex: 1;
  overflow-y: auto;
  background: var(--bg);
  min-width: 0;
}
#main-inner {
  max-width: 820px;
  margin: 0 auto;
  padding: 28px 36px 60px;
}

/* ── welcome ─────────────────────────────────────────────── */
.welcome {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-dim);
}
.welcome-icon {
  font-size: 32px;
  opacity: 0.3;
}
.welcome-text {
  font-family: var(--display);
  font-size: 13px;
  font-weight: 500;
  letter-spacing: 0.04em;
}
.welcome-sub {
  font-size: 12px;
  color: var(--text-dim);
  opacity: 0.7;
}

/* ── session header ──────────────────────────────────────── */
.sess-hd {
  margin-bottom: 28px;
  padding-bottom: 18px;
  border-bottom: 1px solid var(--border);
}
.sess-hd-title {
  font-family: var(--display);
  font-size: 17px;
  font-weight: 600;
  color: var(--text);
  line-height: 1.4;
  margin-bottom: 8px;
}
.sess-hd-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.sess-meta-chip {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-dim);
  background: var(--bg-el);
  border: 1px solid var(--border);
  padding: 2px 7px;
  border-radius: 3px;
}
.sess-meta-chip.accent {
  color: var(--accent);
  background: var(--accent-lo);
  border-color: #3d2510;
}
.copy-all-btn {
  margin-top: 10px;
  padding: 5px 12px;
  background: transparent;
  border: 1px solid var(--border-hi);
  border-radius: var(--radius);
  color: var(--text-dim);
  cursor: pointer;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.copy-all-btn:hover { color: var(--text); border-color: var(--text-dim); }

/* ── messages ────────────────────────────────────────────── */
.msg {
  margin-bottom: 24px;
  display: grid;
  grid-template-columns: 72px 1fr;
  gap: 0 12px;
  align-items: start;
}

.msg-role-col {
  padding-top: 3px;
  text-align: right;
}
.msg-role-label {
  font-family: var(--display);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  display: block;
  margin-bottom: 2px;
}
.msg-user .msg-role-label  { color: var(--user-c); }
.msg-asst .msg-role-label { color: var(--asst-c); }

.msg-ts {
  font-family: var(--mono);
  font-size: 9px;
  color: var(--text-dim);
  display: block;
}

.msg-content-col { min-width: 0; }

.msg-body {
  background: var(--bg-el);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 11px 13px;
  font-family: var(--mono);
  font-size: 12.5px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text);
  position: relative;
}
.msg-user .msg-body {
  border-left: 2px solid var(--user-c);
}
.msg-asst .msg-body {
  border-left: 2px solid var(--asst-c);
}

.msg-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 5px;
}
.copy-btn {
  padding: 3px 9px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 3px;
  color: var(--text-dim);
  cursor: pointer;
  font-family: var(--display);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.copy-btn:hover { color: var(--text-mid); border-color: var(--border-hi); }
.copy-btn.ok { color: var(--asst-c); border-color: var(--asst-c); }

/* gap indicator for resumed sessions */
.msg-gap {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
  margin-bottom: 8px;
}
.msg-gap-line {
  flex: 1;
  height: 1px;
  background: var(--border);
  border: none;
}
.msg-gap-label {
  font-family: var(--display);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-dim);
  white-space: nowrap;
}

/* ── search results ──────────────────────────────────────── */
.search-hd {
  padding: 14px 16px;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}
.sr {
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  cursor: pointer;
  transition: background 0.1s;
}
.sr:hover { background: var(--bg-el); }
.sr-title {
  font-size: 13px;
  color: var(--text);
  margin-bottom: 4px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sr-snip {
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--text-mid);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-bottom: 4px;
}
.sr-snip mark { background: var(--mark-bg); color: var(--mark-text); border-radius: 2px; padding: 0 1px; }
.sr-meta {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  display: flex;
  gap: 8px;
}

/* ── htmx loading ────────────────────────────────────────── */
.htmx-request #main { opacity: 0.6; transition: opacity 0.15s; }

/* ── automated sessions chip ─────────────────────────────── */
.auto-chip {
  margin: 8px 10px 10px;
  padding: 6px 10px;
  background: var(--bg-el);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  cursor: pointer;
  text-align: center;
  transition: all 0.12s;
}
.auto-chip:hover { border-color: var(--border-hi); color: var(--text-mid); }
.auto-chip.active { color: var(--accent); border-color: var(--accent); background: var(--accent-lo); }


/* ── plan sidebar items ──────────────────────────────────── */
.plan-item {
  padding: 10px 12px;
  cursor: pointer;
  border-left: 2px solid transparent;
  border-bottom: 1px solid var(--border);
  transition: background 0.1s, border-color 0.1s;
}
.plan-item:hover { background: var(--bg-hover); }
.plan-item.active { background: var(--bg-hover); border-left-color: var(--accent); }
.plan-title {
  font-size: 12.5px;
  color: var(--text);
  line-height: 1.35;
  margin-bottom: 4px;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.plan-meta {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  display: flex;
  gap: 8px;
}

/* ── plan markdown render ────────────────────────────────── */
.plan-view { padding: 28px 36px 60px; max-width: 820px; margin: 0 auto; }
.plan-view h1 {
  font-family: var(--display); font-size: 22px; font-weight: 700;
  color: var(--text); margin-bottom: 16px; line-height: 1.3;
}
.plan-view h2 {
  font-family: var(--display); font-size: 15px; font-weight: 600;
  color: var(--text-mid); margin: 28px 0 10px;
  padding-bottom: 6px; border-bottom: 1px solid var(--border);
  text-transform: uppercase; letter-spacing: 0.05em;
}
.plan-view h3 {
  font-family: var(--display); font-size: 13px; font-weight: 600;
  color: var(--text-mid); margin: 18px 0 8px;
}
.plan-view p { font-size: 13.5px; color: var(--text); margin-bottom: 12px; line-height: 1.7; }
.plan-view ul, .plan-view ol {
  margin: 0 0 12px 20px; color: var(--text); font-size: 13.5px; line-height: 1.7;
}
.plan-view li { margin-bottom: 4px; }
.plan-view code {
  font-family: var(--mono); font-size: 12px;
  background: var(--bg-el); border: 1px solid var(--border);
  padding: 1px 5px; border-radius: 3px; color: var(--accent-hi);
}
.plan-view pre {
  background: #0a0908; border: 1px solid var(--border);
  border-radius: var(--radius); margin-bottom: 16px; overflow-x: auto;
}
.plan-view pre code {
  background: transparent; border: none; padding: 14px 16px;
  display: block; font-size: 12px; color: var(--text); line-height: 1.6;
}
.plan-view blockquote {
  border-left: 3px solid var(--accent); margin: 0 0 12px;
  padding: 6px 14px; background: var(--accent-lo); border-radius: 0 var(--radius) var(--radius) 0;
}
.plan-view blockquote p { margin: 0; color: var(--text-mid); }
.plan-view table {
  width: 100%; border-collapse: collapse; margin-bottom: 16px; font-size: 13px;
}
.plan-view th {
  font-family: var(--display); font-size: 11px; font-weight: 700;
  letter-spacing: 0.05em; text-transform: uppercase;
  color: var(--text-dim); background: var(--bg-el);
  padding: 8px 12px; border: 1px solid var(--border); text-align: left;
}
.plan-view td {
  padding: 7px 12px; border: 1px solid var(--border); color: var(--text); vertical-align: top;
}
.plan-view tr:nth-child(even) td { background: var(--bg-el); }
.plan-view a { color: var(--accent); text-decoration: none; }
.plan-view a:hover { text-decoration: underline; }
.plan-view hr { border: none; border-top: 1px solid var(--border); margin: 24px 0; }
.plan-view strong { color: var(--text); font-weight: 600; }
.plan-plan-hd {
  display: flex; align-items: flex-start; justify-content: space-between;
  margin-bottom: 20px; padding-bottom: 16px; border-bottom: 1px solid var(--border);
  gap: 16px;
}
.plan-copy-btn {
  padding: 5px 12px; background: transparent;
  border: 1px solid var(--border-hi); border-radius: var(--radius);
  color: var(--text-dim); cursor: pointer;
  font-family: var(--display); font-size: 11px; font-weight: 600;
  letter-spacing: 0.04em; text-transform: uppercase; transition: all 0.12s;
  flex-shrink: 0;
}
.plan-copy-btn:hover { color: var(--text); border-color: var(--text-dim); }
"""

JS = r"""
function copyMsg(btn, text) {
  navigator.clipboard.writeText(text).then(() => {
    btn.classList.add('ok');
    const orig = btn.textContent;
    btn.textContent = 'copied';
    setTimeout(() => { btn.classList.remove('ok'); btn.textContent = orig; }, 1600);
  });
}

function copyAll(btn) {
  const parts = [];
  document.querySelectorAll('#main .msg').forEach(m => {
    const role = m.classList.contains('msg-user') ? 'USER' : 'ASSISTANT';
    const body = m.querySelector('.msg-body');
    if (body) parts.push(role + ':\n' + body.textContent);
  });
  navigator.clipboard.writeText(parts.join('\n\n---\n\n')).then(() => {
    const orig = btn.textContent;
    btn.textContent = 'copied!';
    setTimeout(() => btn.textContent = orig, 1600);
  });
}

function toggleDrawer(id) {
  const body  = document.getElementById('db-' + id);
  const arrow = document.getElementById('da-' + id);
  const open  = body.classList.toggle('open');
  arrow.textContent = open ? '▼' : '▶';
}

function activateSession(sid) {
  document.querySelectorAll('.s-item').forEach(el => el.classList.remove('active'));
  const el = document.querySelector('[data-sid="' + sid + '"]');
  if (el) {
    el.classList.add('active');
    el.scrollIntoView({ block: 'nearest' });
  }
}

function setNav(which) {
  ['timeline','projects','plans'].forEach(n => {
    const btn = document.getElementById('snav-' + n);
    if (btn) btn.classList.toggle('active', n === which);
  });
  // Update search target and placeholder
  const input = document.getElementById('search-input');
  if (!input) return;
  if (which === 'plans') {
    input.setAttribute('hx-get', '/search/plans');
    input.setAttribute('placeholder', 'Search plans\u2026');
  } else {
    input.setAttribute('hx-get', '/search');
    input.setAttribute('placeholder', 'Search conversations\u2026');
  }
  htmx.process(input);
}

function activatePlan(slug) {
  document.querySelectorAll('.plan-item').forEach(el => el.classList.remove('active'));
  const el = document.querySelector('[data-slug="' + slug + '"]');
  if (el) el.classList.add('active');
}

function renderPlanMarkdown() {
  const el = document.getElementById('plan-md-content');
  if (!el || typeof marked === 'undefined') return;
  const raw = el.getAttribute('data-raw');
  el.innerHTML = marked.parse(raw);
  el.querySelectorAll('pre code').forEach(block => {
    if (typeof hljs !== 'undefined') hljs.highlightElement(block);
  });
}

function copyPlanRaw(btn) {
  const el = document.getElementById('plan-md-content');
  if (!el) return;
  navigator.clipboard.writeText(el.getAttribute('data-raw')).then(() => {
    btn.textContent = 'copied!';
    setTimeout(() => btn.textContent = 'copy raw', 1600);
  });
}

document.addEventListener('htmx:afterSwap', function(e) {
  if (e.target.id === 'main') {
    renderPlanMarkdown();
    const hash = window.location.hash;
    if (hash) {
      const el = document.querySelector(hash);
      if (el) setTimeout(() => el.scrollIntoView({ behavior: 'smooth', block: 'center' }), 50);
    } else {
      e.target.scrollTop = 0;
    }
  }
});
"""

# ---------------------------------------------------------------------------
# Database
# ---------------------------------------------------------------------------
_local = threading.local()

def get_db():
    if not hasattr(_local, "conn"):
        _local.conn = sqlite3.connect(DB_PATH, check_same_thread=False)
        _local.conn.row_factory = sqlite3.Row
    return _local.conn

def init_db():
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.executescript("""
        CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY, mtime REAL, size INT, session_id TEXT
        );
        CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            file_path TEXT, project_dir TEXT, cwd TEXT,
            started_at TEXT, ended_at TEXT,
            msg_count INT, first_user_text TEXT,
            is_resumed    INT DEFAULT 0,
            is_automated  INT DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY,
            session_id TEXT, seq INT, role TEXT, ts TEXT, text TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS msg_fts USING fts5(
            text, session_id UNINDEXED, seq UNINDEXED,
            tokenize='porter unicode61', content='messages', content_rowid='id'
        );
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY, value TEXT
        );
        CREATE INDEX IF NOT EXISTS ix_msg_session ON messages(session_id, seq);
        CREATE INDEX IF NOT EXISTS ix_sessions_ended ON sessions(ended_at DESC);
    """)
    # Migrate existing DBs — add column if missing
    try:
        conn.execute("ALTER TABLE sessions ADD COLUMN is_automated INT DEFAULT 0")
        conn.commit()
    except sqlite3.OperationalError:
        pass  # column already exists
    conn.commit()
    conn.close()

# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------
def extract_text(content):
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        return "\n\n".join(
            b["text"].strip()
            for b in content
            if isinstance(b, dict) and b.get("type") == "text" and b.get("text")
        )
    return ""

def is_system_injected(text: str) -> bool:
    return any(text.startswith(p) for p in SYSTEM_PREFIXES)

def parse_session(path: Path):
    messages = []
    cwd = None
    seq = 0

    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue

                t = obj.get("type")
                if t not in ("user", "assistant"):
                    continue

                content = obj.get("message", {}).get("content", "")

                # Skip tool-result echoes (user rows with list content)
                if t == "user" and isinstance(content, list):
                    continue

                text = extract_text(content)
                if not text:
                    continue

                ts = obj.get("timestamp", "")
                if cwd is None:
                    cwd = obj.get("cwd")

                messages.append({"seq": seq, "role": t, "ts": ts, "text": text})
                seq += 1
    except OSError:
        return None, []

    if not messages:
        return None, []

    # Find first meaningful user message (skip system-injected lines)
    first_user_text = ""
    for m in messages:
        if m["role"] == "user" and not is_system_injected(m["text"]):
            first_user_text = m["text"][:120].replace("\n", " ")
            break

    # Fallback: use first assistant message if no human user text found
    if not first_user_text:
        for m in messages:
            if m["role"] == "assistant":
                first_user_text = _clean_assistant_title(m["text"])
                break

    # Detect if this session was resumed (gap > 2h between consecutive messages)
    is_resumed = 0
    prev_ts = None
    for m in messages:
        dt = _parse_ts(m["ts"])
        if dt and prev_ts:
            if (dt - prev_ts).total_seconds() > 7200:
                is_resumed = 1
                break
        if dt:
            prev_ts = dt

    # Detect automated sessions (n8n monitoring, aborted/clear echoes)
    # A session is automated if every user message is system-injected,
    # or if the first real user message looks like a structured monitoring payload
    AUTOMATED_PREFIXES = ("host:", "container:", "disk:", "temp:", "backup:")
    has_human_user = any(
        m["role"] == "user" and not is_system_injected(m["text"])
        for m in messages
    )
    first_real_user = next(
        (m["text"] for m in messages
         if m["role"] == "user" and not is_system_injected(m["text"])),
        ""
    )
    is_automated = int(
        not has_human_user
        or any(first_real_user.startswith(p) for p in AUTOMATED_PREFIXES)
    )

    session_id = path.stem
    project_dir = path.parent.name

    started_at = messages[0]["ts"] if messages else ""
    ended_at   = messages[-1]["ts"] if messages else ""

    meta = {
        "session_id":      session_id,
        "file_path":       str(path),
        "project_dir":     project_dir,
        "cwd":             cwd or "",
        "started_at":      started_at,
        "ended_at":        ended_at,
        "msg_count":       len(messages),
        "first_user_text": first_user_text,
        "is_resumed":      is_resumed,
        "is_automated":    is_automated,
    }
    return meta, messages

# ---------------------------------------------------------------------------
# Indexer
# ---------------------------------------------------------------------------
def _index_incremental(conn, path, cached_size, new_mtime, new_size, sid):
    """Read only newly appended bytes, insert new messages, update session metadata."""
    new_msgs = []
    last_row = conn.execute(
        "SELECT seq, ts FROM messages WHERE session_id=? ORDER BY seq DESC LIMIT 1", [sid]
    ).fetchone()
    seq      = (last_row["seq"] + 1) if last_row else 0
    last_ts  = last_row["ts"]        if last_row else None

    with open(path, encoding="utf-8", errors="replace") as f:
        f.seek(cached_size)
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            t = obj.get("type")
            if t not in ("user", "assistant"):
                continue
            content = obj.get("message", {}).get("content", "")
            if t == "user" and isinstance(content, list):
                continue
            text = extract_text(content)
            if not text:
                continue
            new_msgs.append({"seq": seq, "role": t, "ts": obj.get("timestamp", ""), "text": text})
            seq += 1

    conn.execute("BEGIN")
    try:
        for m in new_msgs:
            cur = conn.execute(
                "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?,?,?,?,?)",
                [sid, m["seq"], m["role"], m["ts"], m["text"]],
            )
            conn.execute(
                "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?,?,?,?)",
                [cur.lastrowid, m["text"], sid, m["seq"]],
            )

        if new_msgs:
            new_ended_at  = new_msgs[-1]["ts"]
            new_msg_count = seq  # seq is next-to-use, equals total count

            # Detect new resume gap between old last message and first new message
            existing_resumed = conn.execute(
                "SELECT is_resumed FROM sessions WHERE session_id=?", [sid]
            ).fetchone()
            is_resumed = existing_resumed["is_resumed"] if existing_resumed else 0
            if not is_resumed and last_ts and new_msgs:
                dt_last = _parse_ts(last_ts)
                dt_new  = _parse_ts(new_msgs[0]["ts"])
                if dt_last and dt_new and (dt_new - dt_last).total_seconds() > 7200:
                    is_resumed = 1

            conn.execute(
                "UPDATE sessions SET ended_at=?, msg_count=?, is_resumed=? WHERE session_id=?",
                [new_ended_at, new_msg_count, is_resumed, sid],
            )

        conn.execute(
            "UPDATE files SET mtime=?, size=? WHERE path=?",
            [new_mtime, new_size, str(path)],
        )
        conn.commit()
    except Exception:
        conn.execute("ROLLBACK")
        raise


def build_or_refresh_index():
    print("Indexing conversations...", end=" ", flush=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    count_new = 0

    try:
        row = conn.execute("SELECT value FROM meta WHERE key='last_scan_at'").fetchone()
        last_scan_at = float(row["value"]) if row else 0.0

        # One bulk read — path → (mtime, size, session_id)
        known = {
            r["path"]: (r["mtime"], r["size"], r["session_id"])
            for r in conn.execute("SELECT path, mtime, size, session_id FROM files").fetchall()
        }

        # NOTE: we intentionally never delete sessions/messages whose source
        # JSONL files have been removed.  The DB serves as a permanent archive.
        for path in sorted(PROJECTS_DIR.rglob("*.jsonl")):
            if "subagents" in path.parts:
                continue

            stat        = path.stat()
            mtime, size = stat.st_mtime, stat.st_size

            if mtime < last_scan_at:
                continue  # guaranteed unchanged since last scan

            cached = known.get(str(path))
            if cached and cached[0] == mtime and cached[1] == size:
                continue  # mtime+size identical — skip

            # ── incremental append path ──────────────────────────────────
            if cached and size > cached[1]:
                # File grew — read only new bytes from old size offset
                try:
                    _index_incremental(conn, path, cached[1], mtime, size, cached[2])
                    count_new += 1
                    continue
                except Exception as e:
                    print(f"\nIncremental failed for {path.name}, falling back: {e}")
                    # fall through to full re-index

            # ── full re-index (new file, shrunk, or incremental failed) ──
            meta, messages = parse_session(path)
            if not meta:
                continue

            sid = meta["session_id"]
            conn.execute("BEGIN")
            try:
                old_ids = [r[0] for r in conn.execute(
                    "SELECT id FROM messages WHERE session_id=?", [sid]
                ).fetchall()]
                if old_ids:
                    ph = ",".join("?" * len(old_ids))
                    conn.execute(f"DELETE FROM msg_fts WHERE rowid IN ({ph})", old_ids)
                conn.execute("DELETE FROM messages WHERE session_id=?", [sid])
                conn.execute("DELETE FROM sessions  WHERE session_id=?", [sid])
                conn.execute("DELETE FROM files      WHERE path=?",       [str(path)])

                conn.execute("""
                    INSERT INTO sessions
                    (session_id,file_path,project_dir,cwd,started_at,ended_at,msg_count,first_user_text,is_resumed,is_automated)
                    VALUES (?,?,?,?,?,?,?,?,?,?)
                """, [meta["session_id"], meta["file_path"], meta["project_dir"],
                      meta["cwd"], meta["started_at"], meta["ended_at"],
                      meta["msg_count"], meta["first_user_text"], meta["is_resumed"],
                      meta["is_automated"]])

                for m in messages:
                    cur = conn.execute(
                        "INSERT INTO messages (session_id,seq,role,ts,text) VALUES (?,?,?,?,?)",
                        [sid, m["seq"], m["role"], m["ts"], m["text"]],
                    )
                    conn.execute(
                        "INSERT INTO msg_fts(rowid,text,session_id,seq) VALUES (?,?,?,?)",
                        [cur.lastrowid, m["text"], sid, m["seq"]],
                    )

                conn.execute(
                    "INSERT INTO files (path,mtime,size,session_id) VALUES (?,?,?,?)",
                    [str(path), mtime, size, sid],
                )
                conn.commit()
                count_new += 1
            except Exception as e:
                conn.execute("ROLLBACK")
                print(f"\nError indexing {path.name}: {e}")

        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_scan_at', ?)",
            [str(time.time())],
        )
        conn.commit()
    finally:
        conn.close()

    print(f"done. {count_new} updated.")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def _parse_ts(ts: str):
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except ValueError:
        return None

def date_bucket(ts_str: str) -> str:
    dt = _parse_ts(ts_str)
    if not dt:
        return "Older"
    now   = datetime.now(timezone.utc)
    delta = now - dt
    if delta.days == 0:   return "Today"
    if delta.days == 1:   return "Yesterday"
    if delta.days <= 7:   return "This week"
    if delta.days <= 31:  return "This month"
    return "Older"

BUCKET_ORDER = ["Today", "Yesterday", "This week", "This month", "Older"]

def fmt_rel(ts_str: str) -> str:
    dt = _parse_ts(ts_str)
    if not dt:
        return ""
    now   = datetime.now(timezone.utc)
    delta = now - dt
    if delta.days == 0:
        h = delta.seconds // 3600
        m = (delta.seconds % 3600) // 60
        if h:  return f"{h}h ago"
        if m:  return f"{m}m ago"
        return "just now"
    if delta.days < 7:
        return f"{delta.days}d ago"
    return dt.strftime("%b %d")

def fmt_date(ts_str: str) -> str:
    dt = _parse_ts(ts_str)
    if not dt:
        return ""
    return dt.astimezone().strftime("%Y-%m-%d %H:%M")

def fmt_short_date(ts_str: str) -> str:
    dt = _parse_ts(ts_str)
    if not dt:
        return ""
    return dt.astimezone().strftime("%b %d")

def cwd_label(cwd: str) -> str:
    if not cwd:
        return "unknown"
    parts = Path(cwd).parts
    return "/".join(parts[-2:]) if len(parts) >= 2 else cwd

def density_pct(msg_count: int) -> int:
    """0-100 based on message count, capped at ~50 msgs as 'full'."""
    return min(100, int(msg_count / 50 * 100))

def fmt_ts_short(ts: str) -> str:
    dt = _parse_ts(ts)
    if not dt:
        return ""
    return dt.astimezone().strftime("%H:%M")

def gap_label(ts1: str, ts2: str) -> str:
    d1, d2 = _parse_ts(ts1), _parse_ts(ts2)
    if not d1 or not d2:
        return ""
    delta = d2 - d1
    days  = delta.days
    hours = delta.seconds // 3600
    if days:
        d2_date = d2.astimezone().strftime("%b %d")
        return f"resumed {d2_date} · {days}d later"
    return f"resumed · {hours}h later"

# ---------------------------------------------------------------------------
# FastHTML app
# ---------------------------------------------------------------------------
app, rt = fast_app(
    live=False,
    pico=False,
    hdrs=(FONT_LINK, HL_CSS, Style(CSS), Script(JS),
          Script(src="https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js"),
          HL_JS, MARKED_JS),
)

# ── session item component ────────────────────────────────────────────────
def session_item(s, active_sid=None):
    sid      = s["session_id"]
    title    = s["first_user_text"] or "—"
    rel_time = fmt_rel(s["ended_at"])
    count    = s["msg_count"]
    resumed  = s["is_resumed"]

    foot_children = [
        Span(rel_time, cls="s-time"),
        Div(Div(style=f"width:{density_pct(count)}%", cls="s-density-fill"), cls="s-density"),
        Span(str(count), cls="s-badge"),
    ]
    if resumed:
        foot_children.append(Span("resumed", cls="s-resumed"))

    return Div(
        P(title, cls="s-title"),
        Div(*foot_children, cls="s-foot"),
        cls="s-item" + (" active" if sid == active_sid else ""),
        data_sid=sid,
        hx_get=f"/session/{sid}",
        hx_target="#main",
        hx_push_url="true",
        onclick=f"activateSession('{sid}')",
    )

# ── drawer by timeline ────────────────────────────────────────────────────
def drawer_timeline(active_sid=None, show_automated=False):
    db            = get_db()
    auto_count    = db.execute("SELECT COUNT(*) FROM sessions WHERE is_automated=1").fetchone()[0]
    where         = "" if show_automated else "WHERE is_automated=0"
    sessions      = db.execute(f"SELECT * FROM sessions {where} ORDER BY ended_at DESC").fetchall()

    buckets = {}
    for s in sessions:
        b = date_bucket(s["ended_at"])
        buckets.setdefault(b, []).append(s)

    groups = []
    for bkt in BUCKET_ORDER:
        items = buckets.get(bkt, [])
        if not items:
            continue
        gid    = bkt.replace(" ", "_")
        is_open = bkt in ("Today", "Yesterday")
        groups.append(Div(
            Div(
                Span("▼" if is_open else "▶", id=f"da-{gid}", cls="drawer-arr"),
                Span(bkt, cls="drawer-hd-label"),
                Span(str(len(items)), cls="drawer-count"),
                cls="drawer-hd",
                onclick=f"toggleDrawer('{gid}')",
            ),
            Div(*[session_item(s, active_sid) for s in items],
                id=f"db-{gid}",
                cls="drawer-body" + (" open" if is_open else "")),
            cls="drawer-group",
        ))
    # Automated sessions toggle chip at bottom
    auto_chip = []
    if auto_count:
        if show_automated:
            auto_chip = [Div(
                Span(f"Hiding {auto_count} automated", style="flex:1"),
                cls="auto-chip active",
                hx_get="/drawer?by=timeline&auto=0", hx_target="#sidebar", hx_swap="outerHTML",
            )]
        else:
            auto_chip = [Div(
                Span(f"+ {auto_count} automated sessions"),
                cls="auto-chip",
                hx_get="/drawer?by=timeline&auto=1", hx_target="#sidebar", hx_swap="outerHTML",
            )]

    return Div(*groups, *auto_chip, id="sidebar")

# ── drawer by project ─────────────────────────────────────────────────────
def drawer_projects(active_sid=None, show_automated=False):
    db            = get_db()
    where         = "" if show_automated else "WHERE is_automated=0"
    sessions      = db.execute(f"SELECT * FROM sessions {where} ORDER BY ended_at DESC").fetchall()

    projects = {}
    for s in sessions:
        key = s["cwd"] or s["project_dir"] or "unknown"
        projects.setdefault(key, []).append(s)

    sorted_proj = sorted(projects.items(), key=lambda kv: kv[1][0]["ended_at"], reverse=True)

    groups = []
    for i, (proj, items) in enumerate(sorted_proj):
        label   = cwd_label(proj)
        gid     = f"proj_{i}"
        is_open = False  # all collapsed by default — user expands what they want
        groups.append(Div(
            Div(
                Span("▼" if is_open else "▶", id=f"da-{gid}", cls="drawer-arr"),
                Span(label, cls="drawer-hd-label"),
                Span(str(len(items)), cls="drawer-count"),
                cls="drawer-hd",
                onclick=f"toggleDrawer('{gid}')",
            ),
            Div(*[session_item(s, active_sid) for s in items],
                id=f"db-{gid}",
                cls="drawer-body" + (" open" if is_open else "")),
            cls="drawer-group",
        ))
    return Div(*groups, id="sidebar")

# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------
@rt("/")
def index():
    topbar = Div(
        Span("Conv Browser", cls="wordmark"),
        Div(
            Span("⌕", cls="search-icon"),
            Input(id="search-input", placeholder="Search conversations…",
                  name="q",
                  hx_get="/search", hx_trigger="input changed delay:250ms",
                  hx_target="#main"),
            id="search-wrap",
        ),
        Button("⟳", id="refresh-btn", title="Refresh index",
               hx_post="/refresh", hx_target="#sidebar", hx_swap="outerHTML"),
        id="topbar",
    )

    sidenav = Div(
        Button("Timeline", id="snav-timeline", cls="snav-btn active",
               hx_get="/drawer?by=timeline", hx_target="#sidebar", hx_swap="outerHTML",
               onclick="setNav('timeline')"),
        Button("Projects", id="snav-projects", cls="snav-btn",
               hx_get="/drawer?by=projects", hx_target="#sidebar", hx_swap="outerHTML",
               onclick="setNav('projects')"),
        Button("Plans", id="snav-plans", cls="snav-btn",
               hx_get="/plans/sidebar", hx_target="#sidebar", hx_swap="outerHTML",
               onclick="setNav('plans')"),
        id="sidenav",
    )

    sidebar_content = drawer_timeline()

    left_panel = Div(sidenav, sidebar_content, id="left-panel")

    main = Div(
        Div(
            Div("◈", cls="welcome-icon"),
            P("Select a conversation", cls="welcome-text"),
            P("or search above to find something", cls="welcome-sub"),
            cls="welcome",
        ),
        id="main",
    )

    return (
        Title("Conv Browser"),
        topbar,
        Div(left_panel, main, id="shell"),
    )

@rt("/drawer")
def drawer_route(by: str = "timeline", auto: str = "0"):
    show = (auto == "1")
    if by == "projects":
        return drawer_projects(show_automated=show)
    return drawer_timeline(show_automated=show)

@rt("/session/{sid}")
def session_view(sid: str):
    db   = get_db()
    s    = db.execute("SELECT * FROM sessions WHERE session_id=?", [sid]).fetchone()
    if not s:
        return Div(P("Session not found."), id="main")

    msgs = db.execute(
        "SELECT * FROM messages WHERE session_id=? ORDER BY seq", [sid]
    ).fetchall()

    # Build chips
    started = fmt_date(s["started_at"])
    ended   = fmt_date(s["ended_at"]) if s["is_resumed"] else ""
    cwd     = cwd_label(s["cwd"])

    chips = [
        Span(started, cls="sess-meta-chip"),
        Span(cwd, cls="sess-meta-chip"),
        Span(f"{s['msg_count']} messages", cls="sess-meta-chip"),
    ]
    if s["is_resumed"]:
        chips.append(Span(f"resumed · last {fmt_rel(s['ended_at'])}", cls="sess-meta-chip accent"))

    sess_hd = Div(
        H2(s["first_user_text"] or "Conversation", cls="sess-hd-title"),
        Div(*chips, cls="sess-hd-meta"),
        Button("copy all", cls="copy-all-btn", onclick="copyAll(this)"),
        cls="sess-hd",
    )

    # Render messages with gap indicators for resumed sessions
    msg_els = []
    prev_ts = None
    for m in msgs:
        cur_dt  = _parse_ts(m["ts"])
        prev_dt = _parse_ts(prev_ts) if prev_ts else None

        # Insert gap indicator if gap > 2 hours
        if prev_dt and cur_dt and (cur_dt - prev_dt).total_seconds() > 7200:
            msg_els.append(Div(
                Hr(cls="msg-gap-line"),
                Span(gap_label(prev_ts, m["ts"]), cls="msg-gap-label"),
                Hr(cls="msg-gap-line"),
                cls="msg-gap",
            ))

        role      = m["role"]
        text      = m["text"]
        data_text = json.dumps(text)
        ts_label  = fmt_ts_short(m["ts"])

        msg_els.append(Div(
            Div(
                Span(role.upper() if role == "user" else "Claude", cls="msg-role-label"),
                Span(ts_label, cls="msg-ts"),
                cls="msg-role-col",
            ),
            Div(
                Pre(text, cls="msg-body"),
                Div(
                    Button("copy", cls="copy-btn",
                           onclick=f"copyMsg(this, {data_text})"),
                    cls="msg-actions",
                ),
                cls="msg-content-col",
            ),
            id=f"msg-{m['seq']}",
            cls=f"msg msg-{'user' if role == 'user' else 'asst'}",
        ))
        prev_ts = m["ts"]

    return Div(
        Div(sess_hd, *msg_els, id="main-inner"),
        id="main",
    )

@rt("/search")
def search_route(q: str = ""):
    q = q.strip()
    if len(q) < 2:
        return Div(
            Div(
                Div("◈", cls="welcome-icon"),
                P("Select a conversation", cls="welcome-text"),
                P("or search above to find something", cls="welcome-sub"),
                cls="welcome",
            ),
            id="main",
        )

    db    = get_db()
    fts_q = build_fts_query(q)
    rows  = []
    if fts_q:
        try:
            rows = db.execute("""
                SELECT m.session_id, m.seq, m.role,
                       s.first_user_text, s.ended_at, s.cwd,
                       snippet(msg_fts, 0, '<mark>', '</mark>', '…', 20) AS hit,
                       msg_fts.rank AS bm25
                FROM msg_fts
                JOIN messages m ON m.id  = msg_fts.rowid
                JOIN sessions s ON s.session_id = m.session_id
                WHERE msg_fts MATCH ?
                ORDER BY msg_fts.rank, s.ended_at DESC LIMIT 60
            """, [fts_q]).fetchall()
        except sqlite3.OperationalError:
            rows = []

    if not rows:
        return Div(
            Div(
                Div("◈", cls="welcome-icon"),
                P(f'No results for \u201c{q}\u201d', cls="welcome-text"),
                cls="welcome",
            ),
            id="main",
        )

    result_els = [Div(f"{len(rows)} results for \u201c{q}\u201d", cls="search-hd")]
    for r in rows:
        sid  = r["session_id"]
        seq  = r["seq"]
        role = r["role"]
        result_els.append(Div(
            P(r["first_user_text"] or "—", cls="sr-title"),
            P(NotStr(r["hit"]), cls="sr-snip"),
            Div(
                Span(fmt_rel(r["ended_at"])),
                Span(cwd_label(r["cwd"])),
                Span(role),
                cls="sr-meta",
            ),
            cls="sr",
            hx_get=f"/session/{sid}",
            hx_target="#main",
            hx_push_url=f"/session/{sid}",
            onclick=f"activateSession('{sid}'); setTimeout(()=>{{window.location.hash='msg-{seq}';}}, 50)",
        ))

    return Div(*result_els, id="main")

@rt("/plans/sidebar")
def plans_sidebar():
    if not PLANS_DIR.exists():
        return Div(P("No plans directory found.", style="padding:12px;color:var(--text-dim)"), id="sidebar")

    plans = sorted(PLANS_DIR.glob("*.md"), key=lambda p: p.stat().st_mtime, reverse=True)

    items = []
    for p in plans:
        title, size_lines = "Untitled", 0
        try:
            lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
            size_lines = len(lines)
            for line in lines:
                if line.startswith("# "):
                    title = line[2:].strip()
                    break
        except OSError:
            pass
        mtime  = p.stat().st_mtime
        dt     = datetime.fromtimestamp(mtime)
        age    = fmt_rel(dt.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"))
        items.append(Div(
            P(title, cls="plan-title"),
            Div(Span(age), Span(f"{size_lines} lines"), cls="plan-meta"),
            cls="plan-item",
            data_slug=p.stem,
            hx_get=f"/plans/{p.stem}",
            hx_target="#main",
            hx_push_url="true",
            onclick=f"activatePlan('{p.stem}')",
        ))

    return Div(
        Div(
            Span("Plans", cls="drawer-hd-label"),
            Span(str(len(plans)), cls="drawer-count"),
            cls="drawer-hd", style="cursor:default",
        ),
        *items,
        id="sidebar",
    )

@rt("/plans/{slug}")
def plan_view(slug: str):
    path = PLANS_DIR / f"{slug}.md"
    if not path.exists():
        return Div(P("Plan not found."), id="main")

    raw = path.read_text(encoding="utf-8", errors="replace")
    mtime = path.stat().st_mtime
    dt    = datetime.fromtimestamp(mtime).astimezone()

    # Extract title for header
    title = slug
    for line in raw.splitlines():
        if line.startswith("# "):
            title = line[2:].strip()
            break

    return Div(
        Div(
            Div(
                H2(title, cls="sess-hd-title"),
                Div(
                    Span(dt.strftime("%Y-%m-%d %H:%M"), cls="sess-meta-chip"),
                    Span(f"{len(raw.splitlines())} lines", cls="sess-meta-chip"),
                    cls="sess-hd-meta",
                ),
                cls="",
            ),
            Button("copy raw", cls="plan-copy-btn", onclick="copyPlanRaw(this)"),
            cls="plan-plan-hd",
        ),
        # data-raw holds the markdown; JS renders it after swap
        Div(id="plan-md-content", data_raw=raw, cls="plan-view",
            **{"data-raw": raw}),
        Script("renderPlanMarkdown();"),
        cls="plan-view",
        id="main",
    )

@rt("/search/plans")
def search_plans(q: str = ""):
    q = q.strip()
    if len(q) < 2:
        return Div(
            Div(Div("◈", cls="welcome-icon"),
                P("Search your plans above", cls="welcome-text"), cls="welcome"),
            id="main",
        )

    if not PLANS_DIR.exists():
        return Div(Div(P("No plans directory."), cls="welcome"), id="main")

    # Split query into words; all must be present (AND semantics)
    words = [w for w in q.lower().split() if w]
    results = []
    for p in sorted(PLANS_DIR.glob("*.md"), key=lambda x: x.stat().st_mtime, reverse=True):
        try:
            text = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        tl = text.lower()
        if not all(w in tl for w in words):
            continue
        # Extract title
        title = p.stem
        for line in text.splitlines():
            if line.startswith("# "):
                title = line[2:].strip()
                break
        # Find a snippet around the first word match
        first_word = words[0]
        idx   = tl.find(first_word)
        start = max(0, idx - 60)
        end   = min(len(text), idx + len(first_word) + 120)
        raw   = text[start:end].replace("\n", " ").strip()
        # Highlight all matched words in the snippet
        def _highlight(s: str, ws: list[str]) -> str:
            for w in ws:
                s = re.sub(f'(?i)({re.escape(w)})', r'<mark>\1</mark>', s)
            return s
        snippet_html = _highlight(raw, words)
        age = fmt_rel(datetime.fromtimestamp(p.stat().st_mtime).astimezone(timezone.utc).isoformat().replace("+00:00","Z"))
        results.append(Div(
            P(title, cls="sr-title"),
            P(NotStr(snippet_html), cls="sr-snip"),
            Div(Span(age), cls="sr-meta"),
            cls="sr",
            hx_get=f"/plans/{p.stem}",
            hx_target="#main",
            hx_push_url=f"/plans/{p.stem}",
            onclick=f"activatePlan('{p.stem}')",
        ))

    if not results:
        return Div(
            Div(Div("◈", cls="welcome-icon"),
                P(f'No plans matching \u201c{q}\u201d', cls="welcome-text"), cls="welcome"),
            id="main",
        )

    return Div(
        Div(f"{len(results)} plan{'s' if len(results)!=1 else ''} matching \u201c{q}\u201d", cls="search-hd"),
        *results,
        id="main",
    )

@rt("/refresh", methods=["POST"])
def refresh():
    build_or_refresh_index()
    return drawer_timeline()

# ---------------------------------------------------------------------------
# Server bootstrap
# ---------------------------------------------------------------------------
def pick_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p

def wait_ready(port, timeout=10.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), 0.1).close()
            return True
        except OSError:
            time.sleep(0.05)
    return False

if __name__ == "__main__":
    init_db()
    build_or_refresh_index()

    port = pick_port()
    print(f"Starting on :{port}…")

    threading.Thread(
        target=lambda: uvicorn.run(app, host="127.0.0.1", port=port, log_level="warning"),
        daemon=True,
    ).start()

    if not wait_ready(port):
        print("Server failed to start.", file=sys.stderr)
        sys.exit(1)

    window = webview.create_window(
        "Conv Browser",
        f"http://127.0.0.1:{port}/",
        width=1300, height=860,
        min_size=(800, 600),
    )
    window.events.closed += lambda: sys.exit(0)
    webview.start()
