# APEX Trading Terminal — Full-Stack Build Prompt

## CONTEXT & OBJECTIVE

You are building **APEX** — a locally-hosted, full-stack trading terminal application. This is a serious, production-grade system targeting active traders, quants, and algo developers. The architecture is defined in README.md. Your task is to implement the entire system end-to-end, working through it systematically phase by phase.

Before writing any code, read the full specification below. Do not deviate from the architectural decisions described. When in doubt, favour performance and correctness over convenience.

-----

## SYSTEM SUMMARY

APEX is a desktop application built with:

- **Tauri 2** as the desktop shell (Rust backend + web frontend)
- **Rust** (Tokio async) for the core engine — market data, order management, risk, strategy orchestration
- **React 18 + TypeScript** for the UI
- **Python 3.11+** sidecar for ML model training/inference and strategy script execution
- **TimescaleDB** (local PostgreSQL) for tick/OHLCV time-series storage
- **DuckDB** (embedded) for fast analytical queries
- **Redis** (local) for sub-millisecond hot state
- **SQLite** for configuration, watchlists, trades, and alert rules

The architecture follows the **Hexagonal (Ports & Adapters)** pattern. All external integrations (brokers, data feeds, databases) implement defined Rust traits. Adding a new broker requires only implementing two traits and registering one adapter — zero changes to core business logic.

-----

## PHASE 1: PROJECT SCAFFOLDING & FOUNDATION

### 1.1 — Workspace Setup

Create a Cargo workspace with the following members:

- `apex-core` — business logic only, no I/O, no framework dependencies
- `apex-adapters` — all adapter implementations (broker, database, feeds)
- `apex-tauri` — Tauri app shell, IPC command handlers, app state

Create a pnpm workspace at the root with `apex-ui` as the frontend package.

Create `apex-python/` as a Poetry-managed Python package.

**Cargo.toml (workspace root):**

```toml
[workspace]
members = ["apex-core", "apex-adapters", "apex-tauri"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
anyhow = "1"
thiserror = "1"
crossbeam = "0.8"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio-tungstenite = { version = "0.23", features = ["rustls-tls-native-roots"] }
```

### 1.2 — Domain Models (`apex-core/src/domain/models.rs`)

Define these canonical types. ALL adapters must map their native types to these:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol(pub String);  // e.g. "RELIANCE", "AAPL", "BTC/USDT"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub time:     DateTime<Utc>,
    pub symbol:   Symbol,
    pub bid:      f64,
    pub ask:      f64,
    pub last:     f64,
    pub volume:   u64,
    pub source:   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub symbol:      Symbol,
    pub bid:         f64,
    pub ask:         f64,
    pub last:        f64,
    pub open:        f64,
    pub high:        f64,
    pub low:         f64,
    pub volume:      u64,
    pub change_pct:  f64,
    pub vwap:        f64,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCV {
    pub time:    DateTime<Utc>,
    pub symbol:  Symbol,
    pub open:    f64,
    pub high:    f64,
    pub low:     f64,
    pub close:   f64,
    pub volume:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderSide { Buy, Sell }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderType { Market, Limit, Stop, StopLimit, TrailingStop }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    Pending, Open, PartiallyFilled, Filled, Cancelled, Rejected
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id:          OrderId,
    pub symbol:      Symbol,
    pub side:        OrderSide,
    pub order_type:  OrderType,
    pub quantity:    f64,
    pub price:       Option<f64>,
    pub stop_price:  Option<f64>,
    pub status:      OrderStatus,
    pub filled_qty:  f64,
    pub avg_price:   f64,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
    pub broker_id:   String,
    pub source:      String,     // "manual" | strategy_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol:     Symbol,
    pub quantity:   f64,
    pub avg_price:  f64,
    pub side:       OrderSide,
    pub pnl:        f64,
    pub pnl_pct:    f64,
    pub broker_id:  String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id:          Uuid,
    pub headline:    String,
    pub summary:     String,
    pub source:      String,
    pub url:         String,
    pub published:   DateTime<Utc>,
    pub symbols:     Vec<Symbol>,
    pub sentiment:   Option<f32>,   // -1.0 to 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    pub id:          Uuid,
    pub strategy_id: String,
    pub symbol:      Symbol,
    pub action:      SignalAction,
    pub quantity:    f64,
    pub price:       Option<f64>,
    pub confidence:  f32,
    pub reason:      String,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalAction { Buy, Sell, Close }
```

### 1.3 — Port Traits (`apex-core/src/ports/`)

**`market_data.rs`:**

```rust
use crate::domain::models::*;
use anyhow::Result;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AdapterHealth { Healthy, Degraded(String), Unhealthy(String) }

pub type TickStream = mpsc::Receiver<Tick>;

#[async_trait::async_trait]
pub trait MarketDataPort: Send + Sync {
    async fn subscribe(&self, symbols: &[Symbol]) -> Result<TickStream>;
    async fn unsubscribe(&self, symbols: &[Symbol]) -> Result<()>;
    async fn get_snapshot(&self, symbol: &Symbol) -> Result<Quote>;
    async fn get_historical_ohlcv(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<OHLCV>>;
    fn adapter_id(&self) -> &'static str;
    fn health(&self) -> AdapterHealth;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Timeframe { S1, S5, S15, M1, M3, M5, M15, M30, H1, H4, D1, W1 }
```

**`execution.rs`:**

```rust
#[async_trait::async_trait]
pub trait ExecutionPort: Send + Sync {
    async fn place_order(&self, order: &NewOrderRequest) -> Result<OrderId>;
    async fn cancel_order(&self, order_id: &OrderId) -> Result<()>;
    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()>;
    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order>;
    async fn get_positions(&self) -> Result<Vec<Position>>;
    async fn get_account_balance(&self) -> Result<AccountBalance>;
    fn broker_id(&self) -> &'static str;
    fn supported_order_types(&self) -> &[OrderType];
    fn health(&self) -> AdapterHealth;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrderRequest {
    pub symbol:     Symbol,
    pub side:       OrderSide,
    pub order_type: OrderType,
    pub quantity:   f64,
    pub price:      Option<f64>,
    pub stop_price: Option<f64>,
    pub tag:        Option<String>,
}
```

**`storage.rs`:**

```rust
#[async_trait::async_trait]
pub trait StoragePort: Send + Sync {
    async fn write_ticks(&self, ticks: &[Tick]) -> Result<()>;
    async fn write_ohlcv(&self, bars: &[OHLCV]) -> Result<()>;
    async fn query_ohlcv(&self, params: OHLCVQuery) -> Result<Vec<OHLCV>>;
    async fn write_order(&self, order: &Order) -> Result<()>;
    async fn update_order(&self, order: &Order) -> Result<()>;
    async fn query_orders(&self, params: OrderQuery) -> Result<Vec<Order>>;
    async fn write_position(&self, pos: &Position) -> Result<()>;
    async fn query_positions(&self, broker_id: &str) -> Result<Vec<Position>>;
}
```

**`news.rs`:**

```rust
pub type NewsStream = mpsc::Receiver<NewsItem>;

#[async_trait::async_trait]
pub trait NewsPort: Send + Sync {
    async fn subscribe(&self, filters: NewsFilter) -> Result<NewsStream>;
    async fn search(&self, query: &str, since: DateTime<Utc>, limit: usize) -> Result<Vec<NewsItem>>;
    fn adapter_id(&self) -> &'static str;
}
```

### 1.4 — Internal Message Bus (`apex-core/src/bus/`)

Implement a topic-based pub/sub bus. Topics are an enum. Publishers and subscribers communicate via `tokio::sync::broadcast` for fan-out, and `tokio::sync::mpsc` for point-to-point.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    Tick(String),          // Tick(symbol)
    Quote(String),         // Quote(symbol)
    OrderUpdate(String),   // OrderUpdate(order_id)
    PositionUpdate,
    NewsItem,
    StrategySignal(String), // StrategySignal(strategy_id)
    Alert,
    SystemHealth,
}

pub struct MessageBus {
    // Use dashmap for concurrent topic → sender map
    senders: DashMap<Topic, broadcast::Sender<BusMessage>>,
}

impl MessageBus {
    pub fn publish(&self, topic: Topic, message: BusMessage) { ... }
    pub fn subscribe(&self, topic: Topic) -> broadcast::Receiver<BusMessage> { ... }
}
```

### 1.5 — Paper Trading Adapter (`apex-adapters/src/execution/paper_trading.rs`)

The paper trading adapter must be available at all times. It simulates fills at the last available price with configurable slippage and commission:

```rust
pub struct PaperTradingAdapter {
    initial_capital: f64,
    currency:        String,
    // Internal state: positions, orders, filled trades, balance
    state:           Arc<RwLock<PaperState>>,
    quote_cache:     Arc<DashMap<String, Quote>>,
}

// Fill logic:
// - Market order: fill immediately at last + slippage
// - Limit order: fill when last price crosses limit price on next tick
// - Slippage: configurable bps (default 2bps)
// - Commission: configurable bps (default 3bps)
```

### 1.6 — Yahoo Finance Adapter (`apex-adapters/src/market_data/yahoo_finance.rs`)

Implement a polling-based market data adapter using Yahoo Finance’s unofficial API. Poll every 3 seconds. Support symbols in NSE (`.NS`) and NASDAQ formats.

-----

## PHASE 2: CORE APPLICATION SERVICES

### 2.1 — Market Data Aggregator (`apex-core/src/application/market_data_aggregator.rs`)

```rust
pub struct MarketDataAggregator {
    adapters:      Vec<Box<dyn MarketDataPort>>,
    bus:           Arc<MessageBus>,
    quote_cache:   Arc<DashMap<String, Quote>>,      // Redis writes happen here
    tick_buffer:   Arc<Mutex<Vec<Tick>>>,             // Flush to TimescaleDB every 100ms
    symbol_map:    HashMap<String, String>,           // alias → canonical
}
```

Responsibilities:

- Spawn a Tokio task per adapter that reads from the adapter’s `TickStream`
- On each tick: update `quote_cache`, publish to message bus on `Topic::Tick(symbol)`, push to `tick_buffer`
- Flush `tick_buffer` to storage every 100ms in a separate Tokio task (non-blocking main path)
- Circuit breaker per adapter (use `failsafe-rs` or implement manually): 5 failures → OPEN state → try fallback adapter if configured

### 2.2 — Risk Engine (`apex-core/src/application/risk_engine.rs`)

Pre-trade risk checks are synchronous and run in < 10μs. They must not do any I/O:

```rust
pub struct RiskEngine {
    config:     RiskConfig,
    positions:  Arc<RwLock<HashMap<String, Position>>>,
    session_pnl: Arc<AtomicI64>,  // fixed-point, atomically updated
}

pub enum RiskVerdict {
    Pass,
    Reject(String),   // human-readable rejection reason
}

impl RiskEngine {
    pub fn check(&self, order: &NewOrderRequest, account: &AccountBalance) -> RiskVerdict {
        // 1. Check market hours (warn if outside, reject if strict mode)
        // 2. Check order value <= max_order_value
        // 3. Check resulting position size <= max_position_pct * portfolio_value
        // 4. Check session P&L > -max_daily_loss (if breach, reject ALL orders)
        // 5. Check duplicate order (same symbol+side+qty within duplicate_window_ms)
        // All checks inline, no async, no I/O
    }
}
```

The max daily loss check is a **hard circuit breaker**. When breached, `RiskEngine` sets an internal `trading_halted` flag. This flag is checked first in every subsequent `check()` call and immediately returns `Reject("Max daily loss reached. Trading halted.")`. The only way to clear it is via explicit UI action (`RiskEngine::reset_halt()`).

### 2.3 — Order & Trade Manager (`apex-core/src/application/order_trade_manager.rs`)

```rust
pub struct OrderTradeManager {
    risk_engine:  Arc<RiskEngine>,
    execution:    HashMap<String, Box<dyn ExecutionPort>>,  // broker_id → adapter
    storage:      Arc<Box<dyn StoragePort>>,
    bus:          Arc<MessageBus>,
    open_orders:  Arc<DashMap<String, Order>>,
    positions:    Arc<DashMap<String, Position>>,
}

impl OrderTradeManager {
    pub async fn submit_order(&self, request: NewOrderRequest, broker_id: &str) -> Result<OrderId> {
        // 1. Risk check (sync)
        // 2. Write PENDING order to storage (journal)
        // 3. Dispatch to broker adapter
        // 4. Update order status in storage
        // 5. Publish OrderUpdate to message bus
        // 6. Return OrderId
    }

    pub async fn handle_fill(&self, fill: FillEvent) {
        // 1. Update order status to Filled/PartiallyFilled in storage
        // 2. Recalculate position for that symbol
        // 3. Update session P&L in RiskEngine
        // 4. Publish OrderUpdate + PositionUpdate to bus
    }

    pub async fn reconcile_positions(&self, broker_id: &str) {
        // Fetch live positions from broker
        // Compare against local state
        // If discrepancy: log warning, update local state to match broker (broker is authoritative)
    }
}
```

Spawn a Tokio task that calls `reconcile_positions()` every 30 seconds for each active broker adapter.

### 2.4 — Alert Engine (`apex-core/src/application/alert_engine.rs`)

The alert engine subscribes to the message bus and evaluates all configured alert rules on every relevant update. Alert rules are stored in SQLite.

```rust
pub enum AlertRule {
    PriceAbove    { symbol: String, threshold: f64 },
    PriceBelow    { symbol: String, threshold: f64 },
    PctChange     { symbol: String, pct: f64, window_secs: u64 },
    VwapCross     { symbol: String },
    RsiThreshold  { symbol: String, timeframe: Timeframe, rsi: f64, above: bool },
    DailyPnl      { threshold: f64 },
    NewsKeyword   { pattern: String, symbols: Vec<String> },
}

pub enum AlertDelivery { InApp, Sound, OsNotification, Telegram(String) }
```

When an alert fires: emit via `Topic::Alert` on the message bus, and dispatch to all configured delivery methods concurrently.

-----

## PHASE 3: ADAPTERS IMPLEMENTATION

### 3.1 — TimescaleDB Adapter (`apex-adapters/src/storage/timescale.rs`)

Use `sqlx` with connection pooling (pool size 8). Implement the `StoragePort` trait.

Key implementation notes:

- `write_ticks()` must use `COPY` protocol or bulk insert for throughput > 100k rows/sec
- Use `ON CONFLICT DO NOTHING` for idempotent writes
- `query_ohlcv()` should use the materialized view `ohlcv_1m` (and other continuous aggregates) rather than raw tick data wherever possible
- All queries must have query timeouts set (default 5s)

Create the schema on first run using migrations (use `sqlx migrate`).

### 3.2 — Redis State Adapter (`apex-adapters/src/storage/redis_state.rs`)

Use `redis-rs` with connection pooling (bb8). Implement fire-and-forget quote updates using pipelines:

```rust
pub async fn update_quote(&self, quote: &Quote) -> Result<()> {
    let key = format!("QUOTE:{}", quote.symbol.0);
    let mut pipe = redis::pipe();
    pipe.hset_multiple(&key, &[
        ("bid",        quote.bid.to_string()),
        ("ask",        quote.ask.to_string()),
        ("last",       quote.last.to_string()),
        ("volume",     quote.volume.to_string()),
        ("change_pct", quote.change_pct.to_string()),
        ("updated_at", quote.updated_at.timestamp_millis().to_string()),
    ])
    .expire(&key, 86400);  // expire after 24h if not updated
    pipe.query_async(&mut self.conn.get().await?).await?;
    Ok(())
}
```

### 3.3 — Zerodha Kite Adapter (`apex-adapters/src/market_data/zerodha_kite.rs`)

Implement both `MarketDataPort` and `ExecutionPort`.

Market data uses the Kite WebSocket (binary packed format). The binary message format:

- Packet header: 2 bytes (number of packets)
- Per packet: 4 bytes (instrument token) + 4 bytes (last price as uint32 × 100) + additional fields

Parse this binary format efficiently using `bytes::Bytes` and direct pointer arithmetic — do not use serde for this hot path.

Auth uses KiteConnect v3 API. Token refresh logic:

- Store `access_token` in the OS keychain
- On startup: attempt API call. If 403, initiate re-auth flow (open browser to Kite login URL, capture redirect with local HTTP server on port 5000, extract `request_token`, POST to `/session/token`, store new `access_token`)

Execution uses the Kite REST API. All order endpoints are rate-limited to 10 requests/second — implement a token bucket rate limiter.

### 3.4 — DuckDB Adapter (`apex-adapters/src/storage/duckdb.rs`)

Use the `duckdb` Rust crate (embedded). Primary use cases:

- Backtesting data slices (read from Parquet files or TimescaleDB via `postgres_scanner` extension)
- Rolling correlation calculations
- Custom analytical queries from the ML workbench

```rust
pub async fn query_ohlcv_for_backtest(
    &self,
    symbol: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<OHLCV>> {
    // Query from parquet files if available, else TimescaleDB
    // DuckDB can read Parquet directly: "SELECT * FROM 'data/parquet/RELIANCE_2024.parquet'"
}
```

-----

## PHASE 4: TAURI SHELL & IPC

### 4.1 — App State (`apex-tauri/src/state.rs`)

```rust
pub struct AppState {
    pub aggregator: Arc<MarketDataAggregator>,
    pub otm:        Arc<OrderTradeManager>,
    pub alerts:     Arc<AlertEngine>,
    pub bus:        Arc<MessageBus>,
    pub storage:    Arc<Box<dyn StoragePort>>,
}
```

Initialise all services in `main()` before creating the Tauri window. Use `tauri::async_runtime` for all async init.

### 4.2 — IPC Commands (`apex-tauri/src/commands/`)

Commands are the bridge between the React frontend and the Rust backend. Keep commands thin — they validate input, call into the application layer, and return serialisable DTOs.

**`market.rs`:**

```rust
#[tauri::command]
pub async fn get_quote(symbol: String, state: State<'_, AppState>) -> Result<QuoteDto, String> { ... }

#[tauri::command]
pub async fn get_ohlcv(
    symbol: String, timeframe: String,
    from: i64, to: i64,
    state: State<'_, AppState>
) -> Result<Vec<OHLCVDto>, String> { ... }

#[tauri::command]
pub async fn subscribe_symbols(symbols: Vec<String>, state: State<'_, AppState>) -> Result<(), String> { ... }
```

**`orders.rs`:**

```rust
#[tauri::command]
pub async fn place_order(request: NewOrderRequestDto, state: State<'_, AppState>) -> Result<String, String> { ... }

#[tauri::command]
pub async fn cancel_order(order_id: String, state: State<'_, AppState>) -> Result<(), String> { ... }

#[tauri::command]
pub async fn get_positions(broker_id: String, state: State<'_, AppState>) -> Result<Vec<PositionDto>, String> { ... }
```

### 4.3 — Real-Time Push Events

The Rust backend pushes real-time updates to the frontend via Tauri events. Subscribe to the message bus and forward to the UI:

```rust
// In main.rs, after window creation:
let bus_clone = state.bus.clone();
let app_handle_clone = app_handle.clone();

tokio::spawn(async move {
    let mut rx = bus_clone.subscribe(Topic::Quote("*".into())); // wildcard
    while let Ok(msg) = rx.recv().await {
        if let BusMessage::Quote(quote) = msg {
            let _ = app_handle_clone.emit("quote-update", &QuoteDto::from(&quote));
        }
    }
});
```

Emit these events from Rust (frontend listens with `listen()`):

- `quote-update` — payload: `QuoteDto`
- `order-update` — payload: `OrderDto`
- `position-update` — payload: `PositionDto`
- `news-item` — payload: `NewsItemDto`
- `alert-fired` — payload: `AlertDto`
- `strategy-signal` — payload: `SignalDto`
- `adapter-health` — payload: `{ adapter_id: string, status: string, message?: string }`

-----

## PHASE 5: REACT FRONTEND

### 5.1 — Design System & Theming

Create `apex-ui/src/theme/tokens.css` with the complete CSS custom properties from the design language specification. Do NOT use any off-the-shelf component library that will impose its own design — use Tailwind + CSS variables only.

Font loading in `index.html`:

```html
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap" rel="stylesheet">
```

All numeric data (prices, quantities, P&L values) must use `font-family: var(--font-mono)` and `font-feature-settings: "tnum" 1`. This is non-negotiable — misaligned price columns are a usability failure.

Positive values: `color: var(--color-bull)`. Negative values: `color: var(--color-bear)`. Use a `PnlValue` component that applies this automatically.

### 5.2 — Tauri Event Bridge (`apex-ui/src/hooks/`)

Create custom hooks that wrap Tauri event listeners and commands:

```typescript
// useMarketData.ts
export function useQuoteStream(symbols: string[]) {
    const updateQuote = useMarketStore(s => s.updateQuote);

    useEffect(() => {
        const unlisten = listen<QuoteDto>('quote-update', (event) => {
            if (symbols.includes(event.payload.symbol)) {
                updateQuote(event.payload);
            }
        });
        invoke('subscribe_symbols', { symbols });
        return () => { unlisten.then(f => f()); };
    }, [symbols.join(',')]);
}
```

**Performance note**: The quote-update event can fire 1000+ times per second across all subscribed symbols. The `useMarketStore` Zustand store must use a `Map<string, Quote>` internally, and Zustand’s selector must prevent re-renders for components that don’t care about the updated symbol. Do NOT store quotes as a React array — map lookups must be O(1).

### 5.3 — Workspace Layout (`apex-ui/src/components/workspace/`)

The workspace is a 12-column grid using `react-grid-layout`. Each panel is a named slot that can contain one of the registered panel types:

- `CHART` — candlestick chart with indicators
- `WATCHLIST` — streaming quote table
- `ORDER_BOOK` — level 2 depth
- `ORDER_ENTRY` — order ticket form
- `ORDER_BLOTTER` — open/filled orders table
- `POSITIONS` — current positions P&L
- `NEWS` — news feed
- `STRATEGY_IDE` — Monaco editor
- `ML_WORKBENCH` — ML training dashboard
- `VECTOR_GRAPH` — D3 relationship graph
- `ALERT_CONSOLE` — alert log
- `SCANNER` — market scanner

Workspace layouts persist to SQLite via a Tauri command. On startup, load the last-used layout.

### 5.4 — Command Bar (`apex-ui/src/components/workspace/CommandBar.tsx`)

Implement a Bloomberg-style global command bar. It is always visible at the top. Activated by pressing `Space` from any panel (when no text input is focused) or clicking on it.

Command parsing logic:

```typescript
function parseCommand(input: string): ParsedCommand {
    // "RELIANCE" → { type: 'SYMBOL_DEFAULT', symbol: 'RELIANCE' }
    // "RELIANCE:CHART" → { type: 'SYMBOL_PANEL', symbol: 'RELIANCE', panel: 'CHART' }
    // ":ORDERS" → { type: 'SYSTEM_PANEL', panel: 'ORDER_BLOTTER' }
    // "BUY RELIANCE 10" → { type: 'ORDER', side: 'BUY', symbol: 'RELIANCE', qty: 10 }
    // "SELL HDFCBANK 5 LIMIT 1600" → { type: 'ORDER', side: 'SELL', ... }
}
```

Include a fuzzy-search autocomplete dropdown (using `fuse.js`) populated with all watched symbols.

### 5.5 — Candlestick Chart (`apex-ui/src/components/charts/CandleChart.tsx`)

Use `lightweight-charts` v4 from TradingView. Key implementation requirements:

- On mount: invoke `get_ohlcv` to load historical data, then listen to `quote-update` for real-time updates. Update the last bar on every tick using `series.update()` — do NOT recreate the series.
- Indicator overlays: MA, EMA, BBANDS rendered as `LineSeries` overlays on the main pane; RSI, MACD rendered in separate panes below using the `createPane()` API.
- Implement chart-click order entry: when `Ctrl+Click` on the chart at a price level, pre-fill the order entry form with that price as limit price.
- Crosshair: show bid/ask spread as a shaded region following the crosshair.
- Volume bars at the bottom of the main pane — colour them to match their candle (bull/bear).
- Persist user-drawn annotations (trend lines, horizontal levels) in SQLite via a Tauri command.

### 5.6 — Order Book Heatmap (`apex-ui/src/components/charts/OrderBookHeatmap.tsx`)

The order book receives 10–50 updates per second. Using React state for individual DOM elements at this frequency is not viable. Use an HTML5 Canvas element and render the book directly:

```typescript
function renderOrderBook(canvas: HTMLCanvasElement, bids: Level[], asks: Level[], midPrice: number) {
    const ctx = canvas.getContext('2d');
    // Normalise bid/ask sizes to determine bar widths
    // Render bids on left side in green gradient
    // Render asks on right side in red gradient
    // Mark mid price with a horizontal line
    // Use requestAnimationFrame — only call this from an animation loop
}
```

Throttle incoming updates to max 30fps for the canvas render (buffer updates, render on next animation frame).

### 5.7 — Order Entry Form (`apex-ui/src/components/trading/OrderEntry.tsx`)

Critical UX requirements:

- Default to the active chart’s symbol if one is focused
- `Tab` key moves between fields in logical order
- `Enter` submits (with confirmation dialog if order value > configured threshold)
- Show real-time estimated value = qty × price as user types
- Show available margin / buying power from account balance
- Highlight in amber if order would breach a risk limit (without preventing input — user sees the warning and confirms)
- After submission: show order status badge that transitions Pending → Open → Filled in real time via `order-update` events

### 5.8 — Strategy IDE (`apex-ui/src/components/strategy/StrategyIDE.tsx`)

Use `@monaco-editor/react`. Configure:

- Language: Python
- Theme: custom dark theme matching APEX palette
- Custom completions: inject APEX SDK API surface (Strategy class methods, Signal types, Timeframe enum, indicator names) as IntelliSense completions using `monaco.languages.registerCompletionItemProvider`
- File browser on the left: list all `.py` files in `strategies/` directory (fetched via Tauri command)
- Toolbar: Run / Stop / Restart / View Logs buttons per strategy
- Live metrics panel below editor: trades count, win rate, P&L, Sharpe (updated via `strategy-signal` events)

-----

## PHASE 6: PYTHON SIDECAR

### 6.1 — Sidecar Architecture (`apex-python/runtime/sidecar.py`)

The Python sidecar runs as a subprocess launched by the Tauri app on startup. It communicates with the Rust core over a Unix domain socket (or named pipe on Windows) using length-prefixed msgpack frames.

```python
# Message format: 4-byte big-endian length prefix + msgpack payload
# Request: { "id": uuid, "method": str, "params": dict }
# Response: { "id": uuid, "result": any } or { "id": uuid, "error": str }

async def handle_connection(reader, writer):
    while True:
        length_bytes = await reader.readexactly(4)
        length = struct.unpack('>I', length_bytes)[0]
        data = await reader.readexactly(length)
        request = msgpack.unpackb(data, raw=False)
        result = await dispatch(request)
        response = msgpack.packb(result)
        writer.write(struct.pack('>I', len(response)) + response)
        await writer.drain()
```

### 6.2 — Strategy SDK (`apex-python/apex_sdk/strategy.py`)

```python
class Strategy:
    """Base class for all APEX strategies. Users subclass this."""

    def __init__(self, strategy_id: str, ipc_client: IPCClient):
        self._id = strategy_id
        self._ipc = ipc_client
        self._subscriptions = []
        self._indicator_cache = {}

    def subscribe(self, symbols: list[str], timeframe: Timeframe):
        self._subscriptions.append((symbols, timeframe))

    def indicator(self, name: str, symbol: str, *params) -> float:
        """Compute or fetch cached indicator value for current bar."""
        key = (name, symbol, params)
        return self._indicator_cache.get(key, float('nan'))

    def emit(self, signal: Signal):
        """Send a trading signal to the Rust OTM."""
        self._ipc.send('emit_signal', {'strategy_id': self._id, 'signal': signal.to_dict()})

    def log(self, message: str):
        self._ipc.send('strategy_log', {'strategy_id': self._id, 'message': message})

    # These must be overridden:
    def on_init(self, params: dict): pass
    def on_bar(self, symbol: str, bar: Bar): pass
    def on_tick(self, symbol: str, tick: Tick): pass
    def on_stop(self): pass
```

### 6.3 — Strategy Subprocess Isolation (`apex-python/runtime/sandbox.py`)

Each strategy runs in its own subprocess (not just a thread — a subprocess for fault isolation). If a strategy crashes, it does not affect other strategies or the sidecar.

```python
def launch_strategy(script_path: str, strategy_id: str, params: dict) -> StrategyProcess:
    proc = subprocess.Popen(
        [sys.executable, '-m', 'apex_python.runtime.strategy_runner',
         '--id', strategy_id, '--script', script_path],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    return StrategyProcess(proc, strategy_id)
```

Monitor each subprocess: if it crashes (non-zero exit), log the stderr, emit an alert via IPC, and restart up to 3 times before marking as FAILED.

### 6.4 — ML Trainer (`apex-python/ml/trainer.py`)

```python
class ModelTrainer:
    def train(self, config: TrainingConfig) -> TrainedModel:
        # 1. Load data via DuckDB query
        df = self._load_data(config)

        # 2. Feature engineering
        features = self._build_features(df, config.features)

        # 3. TimeSeriesSplit cross-validation
        cv = TimeSeriesSplit(n_splits=5)

        # 4. Train model (dispatches to sklearn/XGBoost/PyTorch based on config.algorithm)
        model = self._build_model(config.algorithm, config.hyperparams)

        # 5. Evaluate
        metrics = self._evaluate(model, features, cv)

        # 6. Register
        return self._register(model, features, metrics, config)

    def _build_features(self, df, feature_config):
        # Apply TA-Lib indicators
        # Apply lag features
        # Apply cross-asset features
        # Return X (features), y (target), feature_names
```

-----

## PHASE 7: VECTOR & RELATIONSHIP GRAPH

### 7.1 — Graph Engine (`apex-core/src/application/graph_engine.rs`)

Use `petgraph::Graph<NodeData, EdgeData>`. Node and edge data:

```rust
pub struct NodeData {
    pub id:          Uuid,
    pub node_type:   NodeType,
    pub label:       String,
    pub symbol:      Option<String>,
    pub properties:  HashMap<String, serde_json::Value>,
}

pub enum NodeType { Instrument, Sector, MacroVariable, NewsEvent, CustomVariable }

pub struct EdgeData {
    pub edge_type:  EdgeType,
    pub weight:     f64,
    pub metadata:   HashMap<String, serde_json::Value>,
}

pub enum EdgeType {
    CorrelatedWith { coefficient: f64, window: String },
    BelongsTo,
    LeadsBy { lag_hours: f64 },
    PricedIn { currency: String },
    Custom(String),
}
```

Expose a Tauri command `compute_correlations(symbols: Vec<String>, window_days: u32)` that:

1. Queries DuckDB for daily returns of all symbols over the window
1. Computes pairwise Pearson correlations
1. Adds/updates correlation edges in the graph
1. Returns the updated graph as a `GraphDto` (nodes + edges arrays) for D3

### 7.2 — D3 Graph Visualisation (`apex-ui/src/components/charts/VectorGraph.tsx`)

Use `d3-force` to lay out the graph. Nodes are SVG circles coloured by `NodeType`. Edges are SVG lines with opacity proportional to edge weight. Interaction:

- Click node: highlights the node and all its direct neighbours, dims others
- Hover node: shows tooltip with node properties
- Double-click node: if it’s an instrument node, opens a chart panel for that symbol
- Edge hover: shows edge type and weight
- Zoom/pan via `d3-zoom`
- Filter controls: show/hide edge types, threshold by minimum correlation strength

-----

## PHASE 8: PERFORMANCE & HARDENING

### 8.1 — Hot Path Profiling

Add tracing spans to the critical path using the `tracing` crate. Every tick should be traced from receipt to strategy signal with microsecond timestamps:

```rust
#[tracing::instrument(skip(self, tick))]
async fn process_tick(&self, tick: Tick) {
    let _span = tracing::info_span!("tick_pipeline",
        symbol = %tick.symbol.0,
        source = %tick.source
    ).entered();

    // ... processing
}
```

Export traces to a local Jaeger instance (optional) or log as structured JSON for offline analysis.

### 8.2 — Circuit Breaker Implementation

Implement a reusable `CircuitBreaker<T>` wrapper in `apex-core`:

```rust
pub struct CircuitBreaker<T> {
    inner:             T,
    state:             Arc<RwLock<CbState>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout:           Duration,
    failure_count:     AtomicU32,
    success_count:     AtomicU32,
    last_failure:      Mutex<Option<Instant>>,
}

impl<T> CircuitBreaker<T> {
    pub async fn call<F, Fut, R, E>(&self, f: F) -> Result<R, CbError<E>>
    where
        F: FnOnce(&T) -> Fut,
        Fut: Future<Output = Result<R, E>>,
    {
        match self.state() {
            CbState::Open => Err(CbError::CircuitOpen),
            CbState::HalfOpen | CbState::Closed => {
                match f(&self.inner).await {
                    Ok(result) => { self.record_success(); Ok(result) }
                    Err(e)     => { self.record_failure(); Err(CbError::Inner(e)) }
                }
            }
        }
    }
}
```

Wrap all `MarketDataPort` and `ExecutionPort` calls through a `CircuitBreaker`.

### 8.3 — Crash Recovery

On `OrderTradeManager` startup:

1. Query SQLite for all orders with status `Pending` or `Open`
1. For each: call `execution_adapter.get_order_status(order_id)`
1. Reconcile: if broker says FILLED but local says OPEN → call `handle_fill()`; if broker says CANCELLED but local says OPEN → update to CANCELLED
1. Log all reconciliation actions

### 8.4 — Testing Strategy

**Unit tests** (in each Rust crate):

- `RiskEngine` checks: test each rule independently with mock position/balance data
- `MarketDataAggregator`: test symbol normalisation, tick buffering, fan-out
- Domain model serialisation roundtrips

**Integration tests** (in `apex-adapters/tests/`):

- All adapter tests use a mock HTTP server (`wiremock-rs`) — never hit real APIs in tests
- Test adapter auth flow, order placement, tick parsing, error handling

**E2E tests** (Playwright):

- Test workspace layout persistence
- Test order entry form submission (against paper trading adapter)
- Test command bar parsing
- Test chart data loading

-----

## PHASE 9: CONFIGURATION & PACKAGING

### 9.1 — Configuration Schema

The complete `config/apex.example.toml` must be generated. It should be fully commented. Every configurable value must have a comment explaining what it does and what the valid range is.

### 9.2 — Setup & Migration Scripts

Create:

- `scripts/setup_db.sh` — creates TimescaleDB database, applies all migrations, creates Redis keys structure
- `scripts/dev.sh` — starts all local services (Redis, TimescaleDB) + launches `cargo tauri dev` in one command
- `migrations/` — SQL migration files for TimescaleDB (use `sqlx migrate`)

### 9.3 — Tauri Bundle Configuration

Configure `tauri.conf.json` for production build:

- App identifier: `io.apex.terminal`
- Window: 1440×900 min size, 1920×1080 default, no frame (custom titlebar)
- Security: enable `dangerous_disable_asset_csp_modification = false`, define strict CSP
- Bundle: include Python interpreter and virtualenv as sidecar binary
- Icons: generate all required sizes from a source SVG (lightning bolt aesthetic)

-----

## PHASE 10: DOCUMENTATION

Generate the following docs files in `docs/`:

**`adapter-guide.md`**: Step-by-step guide to implementing a new broker adapter, with a complete annotated example for a fictional “MockBroker” adapter covering auth, market data subscription binary parsing, order placement, error handling, circuit breaker integration, and test writing.

**`strategy-api.md`**: Full APEX Python SDK reference. Document every method on the `Strategy` base class, every indicator available via `self.indicator()`, the `Signal` and `Bar` types, how to access account state, how to log, and best practices for avoiding common pitfalls (lookahead bias, excessive indicator computation in hot path, uncaught exceptions).

**`ml-guide.md`**: Guide to using the ML workbench. Covers dataset construction, feature engineering principles (with emphasis on preventing lookahead bias), model selection guidance for different prediction tasks, walk-forward validation, and how to deploy a trained model as a live signal source.

-----

## CODING STANDARDS

### Rust

- All public APIs must have doc comments
- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` consistently on all domain types
- No `unwrap()` or `expect()` in production code paths — use `?` operator and return `Result`
- All async functions that touch I/O have a configurable timeout via `tokio::time::timeout()`
- Prefer `Arc<T>` for shared ownership; avoid `Mutex<T>` on hot paths; prefer `RwLock<T>` or atomic types
- Every module has a `#[cfg(test)]` block with at minimum smoke tests

### TypeScript / React

- Strict TypeScript — no `any`, no type assertions except in tests
- All Tauri commands have typed wrappers in `apex-ui/src/lib/tauri.ts`
- Components that receive real-time data (quotes, P&L) must be profiled for render performance — use `React.memo`, `useCallback`, `useMemo` where rendering is measurably affected
- CSS class names: use Tailwind utilities, never inline styles except for dynamically computed values (e.g. chart colours)
- All numeric formatting (prices, percentages, large numbers) through a central `format.ts` utility — never `toString()` on a price

### Python

- All strategy files have a docstring at the top explaining the strategy logic
- All `on_bar()` and `on_tick()` implementations must complete in < 1ms — warn (do not fail) if exceeded
- Use `polars` for DataFrame operations in ML pipelines (not pandas), except when library compatibility requires pandas
- All trained models saved with `joblib` + a companion `metadata.json` with feature names, training date, and validation metrics

-----

## IMPLEMENTATION ORDER

Build in this exact sequence to ensure each phase has the dependencies it needs:

1. Cargo workspace + package.json workspace setup
1. Domain models + port traits + message bus
1. Paper trading adapter + Yahoo Finance adapter
1. TimescaleDB + Redis + SQLite + DuckDB adapters
1. Risk engine + Order Trade Manager + Market Data Aggregator
1. Tauri shell + all IPC commands + real-time event push
1. React design system (theme tokens, typography, colour palette)
1. Zustand stores + Tauri event bridge hooks
1. Workspace layout + Command Bar
1. Candlestick chart + real-time quote feed
1. Order entry form + order blotter + position dashboard
1. Zerodha Kite adapter (market data + execution)
1. Alert engine + alert console UI
1. News RSS adapter + news feed UI
1. Python sidecar IPC + Strategy SDK
1. Strategy IDE (Monaco) + strategy runner
1. Backtest engine + backtest results UI
1. ML trainer + model registry + ML workbench UI
1. Vector/Relationship graph engine + D3 visualisation
1. Circuit breakers + crash recovery + reconciliation
1. Market scanner
1. Historical data downloader
1. Configuration UI + health monitor
1. Documentation + tests + setup scripts
1. Production build + packaging

-----

## IMPORTANT CONSTRAINTS

- **Local only**: No code should make network requests to any external service except the explicitly configured market data feeds and broker APIs. No analytics, no telemetry, no version checks phoning home.
- **No credentials in code**: All API keys/secrets use the OS keychain. Never hardcode, never log.
- **Max daily loss is sacred**: The `trading_halted` flag in `RiskEngine` must be checked in the very first line of `submit_order()` before any other logic. Do not add any bypass, override, or “force” parameter.
- **Paper trading is always available**: The paper trading adapter must be usable even if no broker credentials are configured. It is the default execution adapter.
- **Adapters are independent compilation units**: No adapter may import from another adapter. Adapters may only depend on `apex-core`.
- **The message bus is the only cross-service communication**: Application services do not hold direct references to each other. They communicate exclusively via the bus.

-----

Begin with Phase 1 and work through each phase completely before moving to the next. At each phase boundary, verify that all tests pass and the build is clean before proceeding.
