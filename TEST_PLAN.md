# APEX Terminal Live Audit — Test Plan

App under test: Tauri desktop app "APEX Terminal" (apex-tauri, debug build) on display :0,
webview loading Vite dev server http://localhost:3000. Backend has OPEN_ROUTER set.
Working tree contains session edits (withGlobalTauri fix confirmed live: real Yahoo prices shown).

All tests are UI-driven via computer tool on :0, recorded with annotations.

## T1. Tab sweep — all 12 center tabs render
Click each tab button (Chart, Blotter, Book, News, Scanner, Graph, Strategy IDE, ML Workbench,
Stored Data, Notebook, Copilot, Health). Screenshot each.
- PASS: each shows its panel (not blank white/crash). Note empty-state vs broken-state per tab.
- FAIL: tab content area is entirely blank/black with no UI, or window crashes.

## T2. Chart tab renders candles?
On Chart tab with RELIANCE.NS selected, observe chart area over ~5s.
- Code finding: Workspace.tsx:838 passes no ohlcvData → only live 1s tick updates draw.
  sqlite `ohlcv` table is empty (0 rows) → no history source anyway.
- PASS (documented bug): black/empty chart or only a thin live candle at right edge → report as
  defect: chart never calls get_historical_data.
- If real candles render: unexpected pass — note it.

## T3. Paper order end-to-end
On Chart tab (Order Entry visible): symbol RELIANCE.NS, side BUY, type Market, qty 1 → click
"BUY RELIANCE.NS".
- PASS: green confirmation "Order placed on Paper..."; then Blotter tab (click) shows ≥1 order
  row RELIANCE.NS, status Filled, qty 1 within ~4s; Positions panel shows "▲ RELIANCE.NS" qty 1;
  status bar Session P&L becomes non-zero or stays near 0 (real fill price ~1246).
- FAIL: error text, no confirmation, blotter empty after 10s, position missing.

## T4. Scanner run + row click jump
Scanner tab → criterion "Price above" value 100 → Run Scan.
- PASS: "7/7 matched" (all watchlist prices >100), result rows show real prices.
  Click row "AAPL" → app jumps to Chart tab and selected symbol = AAPL.
- FAIL: error string, 0 matches despite prices>100, row click does nothing.

## T5. Order book — synthetic equity vs live Binance crypto
Book tab: select RELIANCE.NS → expect "SYNTHETIC" label + spread shown + bid/ask ladder.
Then add BTCUSDT to watchlist (+ button → type BTCUSDT → confirm) → select BTCUSDT row →
Book tab select BTCUSDT.
- PASS crypto: label "LIVE L2 · BINANCE", prices in BTC range (~100k+), non-zero spread.
- PASS equity: label "SYNTHETIC", prices near RELIANCE.NS last (~1246).
- FAIL: error message, empty ladder, SYNTHETIC label for BTCUSDT, NaN prices.

## T6. Copilot real OpenRouter reply
Copilot tab → type "what do you see in my positions?" → Send.
- PASS: assistant bubble with coherent reply referencing RELIANCE.NS position (qty 1) or
  honestly saying none; model label shown (not "mock").
- FAIL-REPORTABLE: red error bubble — capture exact text (e.g. OpenRouter 404 model error).

## T7. News feed populated
News tab: items should already be cached (first poll fires at startup).
- PASS: ≥1 real headlines (CNBC/MarketWatch/CoinDesk sources, today's dates) — NOT mock
  headlines ("Sensex, Nifty hit fresh highs...").
- If empty: wait ≤60s watching; report whether feeds are unreachable.
- Search box: type "fed" → list filters.

## T8. CommandBar keyboard nav
Click neutral area → press Space → type "MSFT" → Enter → chart + MSFT selected.
Space → ":BLOTTER" → Enter → Blotter tab opens.
Space → "AAPL:BOOK" → Enter → Book tab with AAPL symbol selected.
- PASS: feedback toast text + correct tab/switch each time.
- FAIL: Space does nothing, wrong tab, no feedback.

## T9. Live alert firing
Alerts panel "+" → symbol AAPL, condition Price Above, value 1 → Save.
- PASS: rule listed in Alerts panel; within ~5-10s News tab shows "Fired alerts" banner with
  AAPL message (alert engine evaluates live quotes). Then remove alert to stop repeat firing.
- FAIL: no banner after 15s with quotes flowing.

## T10. Graph tab
Graph tab → "Compute from watchlist".
- Expected: sqlite ohlcv EMPTY → likely nodes visible but "0 edges" or error — document actual.
- PASS: panel doesn't crash, shows node count / honest empty state.
- FAIL: crash/blank.

## T11. Stored Data + Notebook + ML + Strategy + Health render checks
(Part of sweep, deeper check) Stored Data: try fetch for RELIANCE.NS → likely empty (ohlcv 0 rows)
— document empty-state text. Health: adapter rows shown (yahoo_finance, paper). ML/Strategy/
Notebook: render without crash.

## T12. Console errors / general audit
Attempt WebKitGTK devtools (right-click → Inspect Element) or check vite/browser console for
red errors; report visible symptoms. Layout audit at current resolution vs 1920 min —
note truncation/overlap, NaN rendering, dead buttons.

---

# Re-verification pass (post-fix round)

Lead fixed all CRITICAL/HIGH/MEDIUM findings; app restarted fresh (PID ~108265, vite :1420 inside Tauri webview). Verify each fix live:

- T-FIX1 Chart: candles + volume + axes render for RELIANCE.NS on 1D; timeframe switch repaints.
- T-FIX2 Watchlist: CHG% shows real non-zero values.
- T-FIX3 Order Entry: BUY 1 RELIANCE.NS market on Paper fills → appears in Positions + Blotter.
- T-FIX4 Alerts: "+" → RELIANCE.NS PriceAbove 1000 → Save succeeds, rule appears in list.
- T-FIX5 Graph: "Compute from watchlist" does NOT black out the app (graph or error card).
- T-FIX6 News: headlines render entities decoded (no &#39;/&apos;/&#x27;).
- T-FIX7 MARKET tab renders + TickerTape visible; "?" opens KeyboardHud; Space focuses CommandBar.
