# APEX Terminal — Live Audit Report

Date: 2025-09-21 (session audit)
App: `apex-tauri` (debug build, `cargo run --bin apex-tauri`) — WebKitGTK window on `:0`, Vite dev server at `http://localhost:3000`. Backend had `OPEN_ROUTER` set. Working tree contained the session's edits (`withGlobalTauri`, CSP fix, new tabs incl. MARKET, CommandBar).
Window geometry during audit: 1600×1127 (maximized, below the intended ~1920 design width).

Method: live UI-driven testing via screenshot/mouse/keyboard on the real Tauri window, plus targeted code reading to root-cause failures. Screen recording of the session captured with pass/fail annotations.

---

## Verified working

| Area | Result | Evidence |
|---|---|---|
| Real market data (withGlobalTauri fix) | Live Yahoo quotes flow — RELIANCE.NS ₹1,247.40, TCS ₹2,128.70, AAPL $336.13 etc. — not the old mock 150.20 | watchlist + chart header |
| Tab sweep | All 13 center tabs render without crashing: Chart, Market, Blotter, Book, News, Scanner, Graph, Strategy IDE, ML Workbench, Stored Data, Notebook, Copilot, Health | |
| News | Real headlines from CoinDesk / MarketWatch Top Stories / CNBC Markets populate and keep growing (75→77 items during session) | News tab |
| CommandBar | Space opens palette; `:NEWS` + Enter switches to News tab with "Switched to NEWS" toast | screenshot `ss_669563b3.png` |
| Copilot | **Real OpenRouter reply**: asked "what do you see in my positions?" → "No open positions currently. Session P&L: 0.00" with model badge shown — honest, context-aware, not mock | `ss_8670e843.png` |
| Market tab (new) | Renders: index cards (S&P/NASDAQ/DOW/NIFTY/BTC/ETH/GOLD/WTI/EUR-USD/USD-INR) show `--` empty state, Watchlist Breadth 0/0/7, Latest Headlines populated | `ss_cc5b386a.png` |
| Health tab | Adapter rows render: yahoo **healthy**, robinhood healthy, paper healthy; **binance unhealthy** (HTTP 451), **coinbase degraded** (WS reset without close), **polymarket unhealthy** (404). Uptime ~1h58m, Subscriptions 7 | `ss_92a7403a.png` |
| Strategy IDE / ML Workbench / Stored Data / Notebook | All render. Stored Data shows honest empty state "No stored OHLCV rows returned for the current query" (consistent with empty `ohlcv` table) | `ss_32192f2b.png`, `ss_7eafdb7c.png`, `ss_ad016181.png`, `ss_3482dd1a.png` |
| Scanner | Scan ran and returned rows; row click jumps to chart (verified earlier in session) | |
| Book (equity) | Synthetic book path renders for equities | verified earlier in session |

---

## Defects found

### CRITICAL — Graph "Compute from watchlist" kills the whole webview
Clicking **Compute from watchlist** on the Graph tab turns the **entire window black** — sidebar, tabs, statusbar, everything. `WebKitWebProcess` stays alive but pegged at ~66% CPU; window must be **reloaded** (right-click → Reload) to recover.
**Reproduced 2/2 times** (once on fresh state, once after reload). Likely a frontend renderer hang — e.g. a layout/render loop that never settles on empty/NaN correlation data (sqlite `ohlcv` is empty so correlations have no data). Backend `apex-tauri` process stayed alive.
Impact: user-facing crash-level failure reachable from one click.

### CRITICAL — Paper orders never fill (order pipeline dead)
Placing a MARKET BUY returns **"No quote available for symbol"**. Root cause (code): `PaperTradingAdapter::update_quote` is never called in the production wiring, so the paper adapter has no quote to fill against. Consequences: Orders table stays empty, Positions stays "0 open", Session P&L permanently 0.00, Blotter empty. The entire paper-trading loop is dead despite a healthy `paper` adapter.
(Verified earlier in session; still reproducible.)

### HIGH — Chart tab shows no candles
Chart area renders black with only the TradingView watermark — no historical candles. Two compounding causes (code-verified): `Workspace.tsx` passes no `ohlcvData` to the chart component, and sqlite `ohlcv` table is empty (0 rows) — `get_historical_data` is never called, so there is no history source at all. Only 1s live ticks would draw, and even those don't visibly render.

### HIGH — Creating an alert fails: "Unable to save alert"
Alerts panel "+" → fill AAPL / Price Above / 1 → Save → red **"Unable to save alert"** shown in the rules area; rule list stays "No active alerts" (nothing persisted). The submit path runs and the `add_alert` invoke rejects. The actual error reason is **swallowed** — UI only shows a generic string, so the real failure (likely a Tauri arg-name mismatch `rule_json` vs expected `ruleJson`, or `AlertRule` deserialization of `{"PriceAbove":{"symbol","threshold"}}`) can't be seen in-app.
Compounding layout bug: the Alerts panel is `h-40` (160px) fixed-height; the create form (~170px+) overflows so the **Save button renders below the panel's bottom edge** — only a ~10px sliver is clickable and it sits flush against the statusbar.

### HIGH — Crypto order book broken (BTCUSDT)
BTCUSDT book errors instead of showing live depth: Binance returns **HTTP 451 (geo-restricted)** — confirmed by the Health tab adapter row — and the synthetic fallback also fails because there is no Yahoo quote for the symbol. No graceful "feed unavailable" empty state; the panel just errors.

### MEDIUM — Watchlist CHG% stuck at +0.00% for every symbol
Aggregator hardcodes `change_pct: 0.0`. Cascades into the new Market tab: **Top Gainers** shows "No advancing symbols", **Top Losers** empty, **Watchlist Breadth** permanently "0 advancing / 0 declining / 7 flat" — those widgets can never show real breadth.

### MEDIUM — News headlines leak raw HTML entities
Headlines render `&#x2018;` `&#x2019;` `&apos;` etc. literally (e.g. "&#x2018;&#x2019;m burned out&#x2019;", "bitcoin &apos;crypto winter&apos;"). RSS titles need entity-decoding before display.

### MEDIUM — Keyboard: Space hijacks focused buttons
`CommandBar.tsx` opens the palette on Space whenever the focused element isn't an input/textarea — **buttons aren't excluded**. So Space on a focused button opens the CommandBar instead of activating the button. (Observed while working around the clipped alert Save button.)

### LOW / notes
- **Health tab "Memory: 0 MB"** — memory metric reads 0 (probably not wired).
- Right-column Alerts panel is cramped into `h-40` at this window height; create form doesn't fit (see alert defect).
- WebKit devtools unusable in this environment: Inspect Element spawns a hidden `WebKitWebProcess` window that reloads the page and swallows clicks — console-error capture not possible; only visible symptoms reported.
- Binance 451 / Coinbase WS reset / Polymarket 404 are environmental (geo-blocked host) — but the app surfaces raw errors with no degradation path (e.g. crypto book has no fallback).
- Alerts are in-memory only (`AlertEngine.rules: RwLock<Vec>`), not persisted — restart loses them; also `evaluate_quote` fires on every matching tick with no dedup/cooldown, so a hit alert would spam `alert-fired` each tick (unverified at runtime because creation fails).

---

## Coverage / not fully tested
- **ORDER command via CommandBar** — blocked by the paper-quote defect (#2).
- **Alert firing path** (`alert-fired` banner in News) — blocked by alert-creation failure (#4).
- Order Entry fill → Blotter/Positions round trip — blocked by #2.
- Notebook cell **Run**, Strategy **Run/Backtest**, ML **Train Model**, Load Stored Data fetch — UI renders, execution paths not exercised.
- Book tab live crypto depth — blocked by geo-block (no Binance access from this host).
- Console error capture — WebKit inspector unusable (above).

---

# Re-verification pass — post-fix round (2025-09-21, later)

App restarted fresh (PID ~108265); vite dev server on `:3000`. All fixes were claimed verified by the lead; re-tested live below.

## IMPORTANT — stale vite module during the pass

The webview was initially running a **stale `App.tsx`** transform — the served module had no `TickerTape`/`KeyboardHud` imports at all (vite's transform cache had not invalidated). `touch src/App.tsx` + page reload loaded the real bundle. **This means TickerTape and KeyboardHud were NOT actually live in the running app before the reload** — anything the lead "verified" about them earlier was against a stale bundle. After reload: tape mounts (empty state), HUD works.

## Re-verified working

| Item | Result | Evidence |
|---|---|---|
| Chart candles/volume/axes on 1D | Candles, volume histogram, price axis, day axis all render; real Yahoo daily bars | `ss_9166625a.png` |
| Timeframe switch | 5m/15m/1H/1D buttons switch state; `get_historical_data('RELIANCE.NS','15m')` returns real intraday bars (2026-09-15T03:45 15m bars); chart repaints, crosshair shows intraday times | console verification |
| Paper order | BUY 1 RELIANCE.NS market → **Filled**@paper in Blotter; Positions "1 open RELIANCE.NS avg 1,247.65". Persists across reload | `ss_2a32fa5a.png` |
| Alert create/save | `add_alert` succeeds with `{id, ruleJson}` args — camelCase fix confirmed working (earlier `rule_json` mismatch resolved). Remove works via `remove_alert {ruleId}` | console `ADD_OK`, `NOW_RULES: 0` |
| News entities | Headlines render decoded — "Mbappé", "It's", apostrophes clean | News tab |
| Watchlist CHG% | Real non-zero: RELIANCE +1.71%, TCS +1.13%, HDFCBANK +1.16%, INFY −1.23%, AAPL +0.5%, MSFT −0.1%, GOOGL +2.1% | live screenshots |
| MARKET tab | Real breadth **4 advancing / 3 declining / 0 flat**; Top Gainers +1.71/+1.13/+1.16/+0.64; Top Losers −1.23/−0.80/−0.26; headlines populated | `ss_922503a4.png` |
| Graph containment | "Compute from watchlist" → **GRAPH ERROR** card + RETRY instead of blackout — ErrorBoundary fix works | `ss_7d4ba8dc.png` |
| KeyboardHud `?` | Opens "KEYBOARD & COMMANDS" overlay listing shortcuts + command syntax (post-reload) | `ss_157b60cb.png` |
| Space → CommandBar | Space focuses the command input (verified); fix #7 (`isEditable` incl. BUTTON/SELECT/A) is present in code — WebKit doesn't keyboard-focus buttons on click so the hijack path couldn't be exercised by mouse, code-level verified | `ss_7f981b60.png` |

## Defects — still broken / newly found

### HIGH — Alert latch still broken: fires ~once per matching quote (banner spammed to 100+)
Created RELIANCE.NS `PriceAbove 1000` (always true, last≈1247): the FIRED ALERTS banner accumulated **+97 events** and kept growing — it fires on essentially every poll cycle.
**Root cause (code-verified)** in `alert_engine.rs::evaluate_quote`: for every incoming quote, `fired` is computed as `symbol == quote.symbol && cond` — so a quote for a *different* symbol (TCS.NS, INFY.NS…) evaluates `fired=false`, and the `if fired != was_triggered` branch then writes `triggered[id]=false`, **re-arming the latch**. With ~7 symbols polling round-robin, ~6/7 ticks reset it and each RELIANCE.NS tick re-fires. The latch only works in a single-symbol world. Fix: skip latch updates when the rule's symbol doesn't match the quote's symbol (early-continue on symbol mismatch before computing `fired`).
Also observed: `get_alert_rules` returned **3 rules** for one intended alert — `add_rule` blindly pushes with no dedup on id.

### HIGH — Post-startup symbol subscription is a no-op (TickerTape / Market index cards dead forever)
`TickerTape`/`MarketOverview` call `subscribeSymbols(INDEX_SYMBOLS)` on mount, but:
1. `market_data_aggregator.rs:84` — `if self.started_adapters.contains_key(&adapter_id) { continue; }` — an already-started adapter is **never re-subscribed**, so the index symbols never reach Yahoo.
2. `yahoo_finance.rs::subscribe` — the poll loop iterates the **`symbols` Vec captured in the closure**, not `self.subscribed_symbols` — so even if subscribe were re-called, newly-added symbols would never be polled.
Result: tape shows "Awaiting market data…" permanently, Market index cards show `--` permanently, and any symbol added mid-session gets no quotes. Only the 7 initial watchlist symbols ever stream.

### MEDIUM — Chart console spam: "Cannot update oldest data" every second on 1D
`CandleChart.tsx:230` live-tick interval calls `candleSeries.update({time: bucketStart})` where `bucketStart` = today's 00:00-UTC bucket; the last stored daily bar's timestamp is later (exchange-tz-dated), so lightweight-charts throws every 1s tick. Observed 850+ occurrences accumulating. The live last-candle update is dead on 1D; console noise masks real errors.

### LOW — Graph compute still throws (contained, but feature dead)
The `select.prototype.call.bind` nonsense expression at `VectorGraph.tsx:245` still throws "undefined is not an object" on mount — the ErrorBoundary (fix #4) catches it, so it's a graceful dead-feature now rather than a blackout.

### LOW — News item time badge shows "+0.18"
One headline renders `+0.18` where a relative time is expected (visible on the last news row). Minor formatting defect in the timestamp rendering.

### Environmental (unchanged)
- Binance HTTP 451 geo-block persists — crypto L2 book can't be verified from this host.
- WebKit inspector works in this debug build now (used for all console verification) — but note console `type` input can leak into page inputs if focus is off; the `{SYMBOL}` artifacts seen during the pass were my leaked typing, not an app bug.
