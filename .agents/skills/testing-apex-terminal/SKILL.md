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
- **WebKit inspector (debug build) works but is finicky**: right-click → Inspect Element opens a docked devtools. `window.__TAURI__.core.invoke('<cmd>', {camelCaseArgs})` drives IPC directly. Caveats: (a) the FIRST Inspect Element may spawn a hidden ~10px `WebKitWebProcess` toplevel — it stays mapped and **invisibly eats clicks** where it overlaps; find via `xdotool search --class WebKit`, `windowunmap` it, refocus the main window, and re-trigger Inspect Element to get the docked one; (b) while the docked inspector is open it **holds X11 keyboard focus** — Space/typing go to it, not the page (keyboard tests look broken until you close it); (c) the × close is the leftmost toolbar icon (~tool 27,551 bottom-docked); misclicks hit the adjacent dock-position toggles; (d) console input is the `>` line near the window's bottom edge — clicks BELOW tool y≈734 miss the Tauri window and hit KDE (its type-to-search calculator opens); (e) autocomplete popup intercepts Return — press Escape-then-Return or End first; (f) `elementFromPoint` uses webview CSS coords (real y−67).
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
- `add_alert` signature needs BOTH `id` and `ruleJson` (camelCase). Alert latch is symbol-scoped (foreign-symbol ticks early-continue) — a fired rule shows ONE banner and persists across reload.
- TickerTape + Market index cards (`^GSPC`, `^NSEI`, BTC, ETH…) populate at boot with real quotes — poll loop now iterates `subscribed_symbols`. If they show "--"/"Awaiting market data…" again, the post-startup-subscribe regression is back.
- **Quant/indicator verification** (CHART→IND, ANALYTICS tab): `compute_indicator{symbol,indicator,timeframe}` → `{overlay,series:[{name,points}]}`; `get_quant_stats{symbol,timeframe}`; `get_regression{xSymbol,ySymbol,timeframe}`. The AnalyticsPanel symbol input syncs from the workspace `selectedSymbol` — change it by clicking a watchlist row when keyboard focus is trapped in the inspector. `get_regression` aligns daily bars on calendar **date** (fixed for cross-market).
- **New-surface testids** (d5af093): Backtest tab → `backtest-panel/run/equity-chart/drawdown-chart/metrics/trades`, IPC `run_strategy_backtest{request:{path,symbol,timeframe,from,to,quantity,initial_capital}}`, `list_strategy_files` → `[{name,path}]` (path e.g. `strategies/my_strategy.py`). Copilot → `copilot-tool-trace` lists tool calls (get_quote, run_backtest). Settings gear `open-settings` → modal `settings-panel` (`settings-theme`/`settings-density`, `llm-settings-section`/`llm-add-provider`/`llm-active-provider`, `kite-*`, `save-app-settings`).
- **Webview CSS coords = real screen px** (dpr=1; viewport 1600×780). `getBoundingClientRect`/`elementFromPoint` return CSS px = real px, with webview-y ≈ screen-real-y − 67. So screen-tool(x,y) ≈ (cssX/1.5625, (cssY+67)/1.5625). When a click "should" hit a control but doesn't, get its rect via console and convert — don't trust the screenshot label position (e.g. the backtest "Backtest" text is a title; the real `backtest-run` button is the tiny icon right of the capital spinner).
- **Settings Save is currently broken** (d5af093): `settings.rs::ensure_table` panics `index not found` on missing `[appearance]`/`[acp]`/`[llm]` tables → button hangs "Saving…", nothing persists, theme reverts on reload. Until fixed, verify appearance/provider changes as session-only.
