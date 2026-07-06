pub const CSS: &str = r#"
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

::-webkit-scrollbar { width: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border-hi); border-radius: 2px; }
::-webkit-scrollbar-thumb:hover { background: var(--text-dim); }

#shell {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-height: 0;
}

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

.app-version {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  flex-shrink: 0;
  margin-right: 4px;
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

#left-panel {
  width: var(--left-w, 280px);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-mid);
  border-right: 1px solid var(--border);
  overflow: hidden;
}

#resize-handle {
  flex-shrink: 0;
  width: 4px;
  cursor: col-resize;
  background: transparent;
  transition: background 0.12s;
  z-index: 5;
}
#resize-handle:hover, #resize-handle.dragging { background: var(--accent); }
body.resizing { cursor: col-resize !important; user-select: none; }
body.resizing * { user-select: none !important; }

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
  margin-bottom: 3px;
}
.s-item.active .s-title { color: var(--text); }

.s-codename {
  display: block;
  font-family: var(--mono);
  font-size: 9.5px;
  color: #7a3030;
  letter-spacing: 0.08em;
  margin-bottom: 5px;
  padding-left: 7px;
  border-left: 2px solid #4a1a1a;
  line-height: 1.2;
  transition: color 0.1s, border-color 0.1s;
}
.s-item:hover .s-codename {
  color: #b84848;
  border-left-color: #8a2a2a;
}
.s-item.active .s-codename {
  color: #c85858;
  border-left-color: #9e3535;
}

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
.s-ref {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--accent);
  font-weight: 600;
  flex-shrink: 0;
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
  flex: 1;
}
.sess-title-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
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
.sess-meta-chip.ref-chip {
  color: var(--accent-hi);
  background: var(--accent-lo);
  border-color: #3d2510;
  font-weight: 600;
}
.copy-all-btn {
  padding: 5px 14px;
  background: var(--bg-el);
  border: 1px solid var(--border-hi);
  border-radius: var(--radius);
  color: var(--text-mid);
  cursor: pointer;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.copy-all-btn:hover { color: var(--text); background: var(--bg-hover); border-color: var(--accent); }
.back-btn {
  padding: 4px 10px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-dim);
  cursor: pointer;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  transition: all 0.12s;
  margin-bottom: 10px;
}
.back-btn:hover { color: var(--text); border-color: var(--border-hi); }
.msg-target { background: var(--accent-lo); border-radius: var(--radius); padding: 8px; }
.msg-target .msg-body mark,
.sr-snip mark { background: var(--mark-bg); color: var(--mark-text); padding: 1px 2px; border-radius: 2px; }

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

.search-filters {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-el);
}
.sfilter {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-mid);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 4px 8px;
  max-width: 220px;
}
.sfilter:focus { border-color: var(--border-hi); outline: none; }
.sfilter-toggle {
  display: flex;
  align-items: center;
  gap: 5px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-mid);
  cursor: pointer;
  user-select: none;
}
.sfilter-toggle input { accent-color: var(--accent); cursor: pointer; }

.sgroup { border-bottom: 1px solid var(--border); }
.sgroup-hd {
  padding: 12px 16px 8px;
  cursor: pointer;
  transition: background 0.1s;
}
.sgroup-hd:hover { background: var(--bg-el); }
.sgroup-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 4px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sgroup-meta {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--text-dim);
  display: flex;
  gap: 8px;
}
.sgroup-ref { color: var(--accent); }
.sgroup-count { margin-left: auto; }
.sr-grouped {
  padding: 8px 16px 8px 28px;
  border-bottom: none;
}
.sr-more {
  padding: 6px 16px 12px 28px;
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--text-dim);
  cursor: pointer;
}
.sr-more:hover { color: var(--text-mid); }

.htmx-request #main { opacity: 0.6; transition: opacity 0.15s; }

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

.sess-hd-actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}
.export-btn {
  padding: 5px 14px;
  background: var(--bg-el);
  border: 1px solid var(--border-hi);
  border-radius: var(--radius);
  color: var(--text-mid);
  cursor: pointer;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.export-btn:hover { color: var(--text); background: var(--bg-hover); border-color: var(--accent); }

.fav-btn {
  padding: 4px 9px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-dim);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
  transition: all 0.12s;
  flex-shrink: 0;
}
.fav-btn:hover { color: var(--accent); border-color: var(--accent); background: var(--accent-lo); }
.fav-btn.active { color: var(--accent-hi); border-color: var(--accent); background: var(--accent-lo); }

.s-fav-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 11px;
  color: var(--text-dim);
  padding: 0 1px;
  line-height: 1;
  flex-shrink: 0;
  opacity: 0;
  transition: color 0.1s, opacity 0.1s;
}
.s-item:hover .s-fav-btn { opacity: 1; }
.s-fav-btn.active { opacity: 1; color: var(--accent); }
.s-fav-btn:hover { color: var(--accent-hi) !important; }
.s-item.is-fav { background: var(--accent-lo); border-left-color: var(--accent); }
.s-item.is-fav:hover { background: var(--bg-hover); }

.starred-hd {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px 6px;
  position: sticky;
  top: 0;
  background: var(--bg-mid);
  z-index: 1;
  border-bottom: 1px solid var(--accent-lo);
}
.starred-hd-icon { font-size: 11px; color: var(--accent); }
.starred-hd-label {
  font-family: var(--display);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--accent);
  flex: 1;
}
.starred-count {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--accent);
  background: var(--accent-lo);
  border: 1px solid #3d2510;
  padding: 1px 5px;
  border-radius: 3px;
}
.starred-body { display: block; margin-bottom: 4px; }

.sess-notes {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.notes-input {
  width: 100%;
  min-height: 64px;
  padding: 8px 10px;
  background: var(--bg-el);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-mid);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  transition: border-color 0.15s;
}
.notes-input::placeholder { color: var(--text-dim); opacity: 0.6; }
.notes-input:focus { border-color: var(--border-hi); }
.notes-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
}
.notes-save-btn {
  padding: 4px 14px;
  background: var(--bg-el);
  border: 1px solid var(--accent);
  border-radius: var(--radius);
  color: var(--accent);
  cursor: pointer;
  font-family: var(--display);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  transition: all 0.12s;
}
.notes-save-btn:hover { background: var(--accent-lo); color: var(--accent-hi); border-color: var(--accent-hi); }
.notes-status {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--asst-c);
  min-height: 14px;
}
.notes-saved {
  font-family: var(--mono);
  font-size: 10px;
  color: var(--asst-c);
}

.load-more-btn {
  display: block;
  width: calc(100% - 24px);
  margin: 6px 12px 8px;
  padding: 6px 10px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-dim);
  cursor: pointer;
  font-family: var(--mono);
  font-size: 10px;
  text-align: center;
  transition: all 0.12s;
}
.load-more-btn:hover { border-color: var(--border-hi); color: var(--text-mid); }

#update-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  background: var(--accent-lo);
  border-bottom: 1px solid var(--accent);
  font-size: 12px;
  font-family: var(--ui);
  color: var(--accent-hi);
  flex-shrink: 0;
}
#update-banner span { flex: 1; }
#update-banner button {
  padding: 3px 10px;
  background: transparent;
  border: 1px solid var(--accent);
  border-radius: var(--radius);
  color: var(--accent);
  cursor: pointer;
  font-family: var(--display);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  transition: all 0.12s;
}
#update-banner button:hover { color: var(--accent-hi); border-color: var(--accent-hi); }
#update-banner button:last-child { color: var(--text-dim); border-color: var(--border); }
#update-banner button:last-child:hover { color: var(--text); border-color: var(--border-hi); }

/* ── Tool call messages ──────────────────────────────────── */
:root {
  --tool-c:    #7a9fbe;
  --tool-lo:   #0d1a26;
  --tool-border: #1e3a52;
}

.msg-tool .msg-role-label { color: var(--tool-c); }
.tool-role-label {
  font-family: var(--mono);
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.tool-call-box {
  background: var(--tool-lo);
  border: 1px solid var(--tool-border);
  border-left: 2px solid var(--tool-c);
  border-radius: var(--radius);
  overflow: hidden;
  font-family: var(--mono);
}

details.tool-call-box > summary {
  list-style: none;
}
details.tool-call-box > summary::-webkit-details-marker { display: none; }

.tool-call-summary {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 10px;
  cursor: pointer;
  user-select: none;
  color: var(--tool-c);
  font-size: 12px;
  transition: background 0.1s;
}
.tool-call-summary:hover { background: rgba(122, 159, 190, 0.07); }

.tool-call-icon {
  font-size: 11px;
  flex-shrink: 0;
  color: var(--tool-c);
  opacity: 0.8;
}

.tool-file {
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--tool-c);
  word-break: break-all;
}

.tool-call-compact {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 10px;
}
.tool-call-extras {
  font-family: var(--mono);
  font-size: 10.5px;
  color: var(--text-dim);
}

.tool-call-desc {
  padding: 5px 10px;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-mid);
  border-bottom: 1px solid var(--tool-border);
  background: rgba(122, 159, 190, 0.04);
}

.tool-bash-cmd {
  margin: 0;
  padding: 7px 10px;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-all;
  background: transparent;
  border: none;
  line-height: 1.55;
}

.tool-generic-row {
  display: flex;
  gap: 6px;
  padding: 2px 10px;
  font-size: 11.5px;
}
.tool-generic-row:first-child { padding-top: 6px; }
.tool-generic-row:last-child  { padding-bottom: 6px; }
.tool-generic-key { color: var(--text-dim); flex-shrink: 0; }
.tool-generic-val { color: var(--text-mid); word-break: break-all; }

/* ── Diff viewer ─────────────────────────────────────────── */
.diff-block {
  overflow-x: auto;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.5;
  max-height: 480px;
  overflow-y: auto;
}

.diff-line {
  display: block;
  padding: 0 10px;
  white-space: pre;
  min-width: max-content;
}
.diff-add  { color: #4ec994; background: rgba(78, 201, 148, 0.07); }
.diff-del  { color: #e06c75; background: rgba(224, 108, 117, 0.07); }
.diff-hunk { color: #7aa2f7; background: rgba(122, 162, 247, 0.06); }
.diff-file { color: var(--text-dim); }
.diff-ctx  { color: var(--text-dim); }
"#;

pub const JS: &str = r#"
(function initResize() {
  const KEY = 'total-recall:left-w';
  const MIN = 180, MAX = 600;
  const root = document.documentElement;
  const saved = parseInt(localStorage.getItem(KEY) || '', 10);
  if (!isNaN(saved) && saved >= MIN && saved <= MAX) {
    root.style.setProperty('--left-w', saved + 'px');
  }
  document.addEventListener('mousedown', (e) => {
    const handle = e.target.closest('#resize-handle');
    if (!handle) return;
    e.preventDefault();
    const startX = e.clientX;
    const panel = document.getElementById('left-panel');
    const startW = panel.getBoundingClientRect().width;
    handle.classList.add('dragging');
    document.body.classList.add('resizing');
    const onMove = (ev) => {
      let w = startW + (ev.clientX - startX);
      if (w < MIN) w = MIN;
      if (w > MAX) w = MAX;
      root.style.setProperty('--left-w', w + 'px');
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      handle.classList.remove('dragging');
      document.body.classList.remove('resizing');
      const w = parseInt(root.style.getPropertyValue('--left-w'), 10);
      if (!isNaN(w)) localStorage.setItem(KEY, String(w));
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  });
})();

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
  arrow.textContent = open ? '\u25BC' : '\u25B6';
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

function searchUrl() {
  var input = document.getElementById('search-input');
  var q = input ? input.value.trim() : '';
  var base = input ? (input.getAttribute('hx-get') || '/search') : '/search';
  return base + '?q=' + encodeURIComponent(q);
}

function runSearch() {
  htmx.ajax('GET', searchUrl(), {target: '#main', swap: 'outerHTML'});
}

// Attach active filter values to every conversation-search request, whether
// triggered by typing (input hx-get) or by a filter control (runSearch).
document.body.addEventListener('htmx:configRequest', function(e) {
  if (e.detail.path.indexOf('/search') !== 0 || e.detail.path.indexOf('/search/plans') === 0) return;
  var proj = document.getElementById('sf-project');
  var fav  = document.getElementById('sf-fav');
  var sub  = document.getElementById('sf-sub');
  if (proj && proj.value) e.detail.parameters['project'] = proj.value;
  if (fav && fav.checked) e.detail.parameters['fav'] = '1';
  if (sub && sub.checked) e.detail.parameters['sub'] = '1';
});

function goBack() {
  var input = document.getElementById('search-input');
  var q = input ? input.value.trim() : '';
  document.querySelectorAll('.s-item').forEach(function(el) { el.classList.remove('active'); });
  if (q.length >= 2) {
    htmx.ajax('GET', searchUrl(), {target: '#main', swap: 'outerHTML'});
  } else {
    document.getElementById('main').innerHTML =
      '<div class="welcome"><div class="welcome-icon">\u25C8</div>' +
      '<p class="welcome-text">Select a conversation</p>' +
      '<p class="welcome-sub">or search above to find something</p></div>';
  }
}

function scrollToSearchResult(seq) {
  var el = document.getElementById('msg-' + seq);
  if (!el) return;
  el.classList.add('msg-target');
  setTimeout(function() { el.scrollIntoView({ behavior: 'smooth', block: 'center' }); }, 50);
  var input = document.getElementById('search-input');
  var query = input ? input.value.trim() : '';
  if (!query) return;
  var body = el.querySelector('.msg-body');
  if (!body) return;
  var words = query.split(/\s+/).filter(function(w) { return w.length > 0; });
  var text = body.textContent;
  text = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  words.forEach(function(w) {
    w = w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    text = text.replace(new RegExp('(' + w + ')', 'gi'), '<mark>$1</mark>');
  });
  body.innerHTML = text;
}

document.addEventListener('keydown', function(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === 'c') {
    const sel = window.getSelection();
    if (sel && sel.toString().length > 0) {
      navigator.clipboard.writeText(sel.toString());
    }
  }
});

async function toggleSidebarFav(event, sid) {
  event.stopPropagation();
  await fetch('/session/' + sid + '/favourite', { method: 'POST' });
  const item = event.currentTarget.closest('.s-item');
  const isFav = item.classList.toggle('is-fav');
  const btn = event.currentTarget;
  btn.classList.toggle('active', isFav);
  btn.title = isFav ? 'Remove from favourites' : 'Mark as favourite';
}

async function exportMarkdown(sid, codename) {
  const resp = await fetch('/session/' + sid + '/markdown');
  const text = await resp.text();
  const blob = new Blob([text], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = codename + '.md';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function onDetailFavClick(event, sid) {
  var sItem = document.querySelector('[data-sid="' + sid + '"]');
  if (sItem) {
    var isFav = !sItem.classList.contains('is-fav');
    sItem.classList.toggle('is-fav', isFav);
    var sfavBtn = sItem.querySelector('.s-fav-btn');
    if (sfavBtn) {
      sfavBtn.classList.toggle('active', isFav);
      sfavBtn.title = isFav ? 'Remove from favourites' : 'Mark as favourite';
    }
  }
  setTimeout(function() {
    var activeNav = document.querySelector('#sidenav .snav-btn.active');
    if (!activeNav || activeNav.id !== 'snav-projects') {
      htmx.ajax('GET', '/drawer?by=timeline', {target: '#sidebar', swap: 'outerHTML'});
    }
  }, 400);
}

function showUpdateBanner(version) {
  if (document.getElementById('update-banner')) return;
  var b = document.createElement('div');
  b.id = 'update-banner';
  b.innerHTML =
    '<span>&#x2B06; v' + version + ' available</span>' +
    '<button onclick="installUpdate(this)">Install &amp; Restart</button>' +
    '<button onclick="this.parentElement.remove()">&#x2715;</button>';
  document.body.insertBefore(b, document.body.firstChild);
}

async function installUpdate(btn) {
  btn.textContent = 'Downloading…';
  btn.disabled = true;
  try {
    await window.__TAURI__.core.invoke('install_update');
  } catch(e) {
    btn.textContent = 'failed: ' + String(e);
    btn.disabled = false;
  }
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

(async function() {
  try {
    var v = await window.__TAURI__.core.invoke('check_update');
    if (v) showUpdateBanner(v);
  } catch(e) {}
})();
"#;
