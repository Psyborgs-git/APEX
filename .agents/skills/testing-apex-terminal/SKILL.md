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
- **Stale vite transforms**: vite can serve an outdated module transform after file edits — the running webview then lacks newly-added components even though the code looks right on disk. If a component "should be there but isn't", verify what vite actually serves with `curl -s localhost:3000/src/App.tsx | grep <Symbol>`; `touch` the file + `location.reload()` in the webview console forces re-transform. Don't trust a running page as proof the latest code is live.
- **WebKit inspector (debug build) works**: right-click → Inspect Element opens a docked devtools with a working Console. `window.__TAURI__.core.invoke('<cmd>', {camelCaseArgs})` can drive IPC directly — useful for `add_alert`/`get_alert_rules`/`get_quote`/`get_historical_data` verification. Caveats: (a) typing into the console can leak into a focused page input — click the console prompt line (bottom, near y≈722 tool-px) first and confirm focus before typing; (b) `elementFromPoint` takes *webview CSS* coords, not screen coords (webview top ≈ real y=67); (c) dispatching `new KeyboardEvent` tests handlers but React state updates flush async — poll with `setTimeout` before asserting DOM.
- **Recover a crashed/black webview**: right-click the window → Reload. The whole content area can go black after a renderer crash/hang; the backend process stays alive.
- **Space key**: opens CommandBar unless the focused element is editable (input/textarea/button/select/a are excluded). Note WebKitGTK does NOT give buttons keyboard focus on click, so Space-on-button is unreachable via mouse — verify the code path or use Tab navigation.
- Fixed-height right-side panels (`h-40` etc.) can clip inner content at this window size — a button may render below the panel border, overlapping the statusbar, making only a thin sliver clickable.

## CommandBar grammar (verified)
Space activates. `:PANEL` switches tabs (e.g. `:NEWS` → "Switched to NEWS" toast). `SYMBOL` selects chart symbol. `SYMBOL:PANEL` combines. `BUY|SELL SYM qty [LIMIT px]` enters orders.

## Quick verification anchors
- Watchlist left column shows real Yahoo quotes when IPC works.
- Health tab lists adapters: yahoo/paper healthy; binance 451, coinbase WS reset, polymarket 404 on this host.
- Copilot reply arrives ~10-20s after Send with a model badge.
- News tab: real CNBC/MarketWatch/CoinDesk headlines; count grows over time.
- Positions panel shows "N open" + fills after a paper order; Blotter tab lists Filled rows — both persist across reload.
- `add_alert` signature needs BOTH `id` and `ruleJson` (camelCase). Alert latch caveat: `evaluate_quote` re-arms `triggered[id]` whenever a non-matching symbol's quote arrives — a symbol-scoped alert re-fires on every matching tick while other symbols keep polling.
- Index symbols (`^GSPC` etc. for TickerTape/Market cards) never get quotes: the aggregator skips re-subscribing already-started adapters AND the yahoo poll loop only iterates the symbols captured at first subscribe. Expect permanent "Awaiting market data…" / `--` until that's fixed.
