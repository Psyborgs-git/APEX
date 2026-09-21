---
name: testing-apex-terminal
description: How to live-test the APEX Terminal Tauri desktop app on display :0 — webview quirks, inspector trap, CommandBar grammar, known-good verification points.
---

# Testing APEX Terminal (Tauri desktop app)

The app is a Rust backend (`cargo run --bin apex-tauri`) + React frontend (`apex-ui`, Vite dev server on http://localhost:3000) rendered in a WebKitGTK webview on display `:0`. Test it through the real window with the computer tool — NOT headless Playwright.

## Devin Secrets Needed
- `OPEN_ROUTER` — must be in the backend process env for Copilot (OpenRouter) to work. Check with `tr '\0' '\n' < /proc/<pid>/environ | grep OPEN_ROUTER`.

## Environment facts
- Real display is 1600×1200; the computer tool works in 1024×768 space (~1.5625 scale). The `zoom` action renders regions at REAL resolution — zoomed image pixels map 1:1 to real px, so true positions are deeper than the tool-space screenshot suggests. When a click near the bottom edge misses, re-estimate coordinates from a zoomed crop.
- sqlite DB at `data/apex.db`. No `sqlite3` CLI — use `python3 -c "import sqlite3; ..."`.
- `IS_TAURI` gate (`'__TAURI__' in window`): the same UI in a normal browser silently falls back to MOCK data (all prices 150.20). Only the real webview exercises real IPC.
- Binance API is geo-blocked from this box (HTTP 451). Crypto order books cannot be verified live here; the Health tab shows adapter status to confirm.

## Pitfalls
- **Do NOT use Inspect Element / WebKit devtools.** It spawns an invisible `WebKitWebProcess` window that RELOADS the page (wiping UI state) and swallows clicks while mapped. If it appears: `xdotool windowunmap <win_id>` then `wmctrl -i -a <main_window_id>` (main window id from `xdotool search --name "APEX Terminal"`). Console errors are effectively unreachable — report visible symptoms only.
- **Recover a crashed/black webview**: right-click the window → Reload. The whole content area can go black after a renderer crash/hang; the backend process stays alive.
- **Space key**: opens CommandBar from almost anywhere — it is NOT suppressed when a `<button>` has focus (only inputs/textareas are excluded). Use Tab+Return (not Space) to activate focused buttons.
- Fixed-height right-side panels (`h-40` etc.) can clip inner content at this window size — a button may render below the panel border, overlapping the statusbar, making only a thin sliver clickable.

## CommandBar grammar (verified)
Space activates. `:PANEL` switches tabs (e.g. `:NEWS` → "Switched to NEWS" toast). `SYMBOL` selects chart symbol. `SYMBOL:PANEL` combines. `BUY|SELL SYM qty [LIMIT px]` enters orders.

## Quick verification anchors
- Watchlist left column shows real Yahoo quotes when IPC works.
- Health tab lists adapters: yahoo/paper healthy; binance 451, coinbase WS reset, polymarket 404 on this host.
- Copilot reply arrives ~10-20s after Send with a model badge.
- News tab: real CNBC/MarketWatch/CoinDesk headlines; count grows over time.
