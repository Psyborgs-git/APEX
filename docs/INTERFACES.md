# Interfaces & Contracts

## Internal Communication: Message Bus

APEX uses a topic-based publish/subscribe message bus (**MessageBus**) powered by `tokio::sync::broadcast` channels. This decouples event producers (e.g., adapters) from consumers (e.g., UI, Strategy Engine).

### Bus Topics
The `Topic` enum defines the available communication channels:

| Topic | Description |
| :--- | :--- |
| `Tick(String)` | Raw, validated price tick for a specific symbol (e.g., `Tick("AAPL")`). |
| `Quote(String)` | Aggregated quote snapshot update for a specific symbol. |
| `OrderUpdate(String)` | Lifecycle state change for a specific order ID. |
| `PositionUpdate` | Broadcast whenever a position size or P&L changes. |
| **NewsItem** | A new, enriched news article from the **NewsEngine**. |
| `StrategySignal(String)` | A trading signal emitted by a specific python strategy. |
| `Alert` | System alerts fired by the **AlertEngine**. |
| **SystemHealth** | Periodic adapter status and systemic health events. |

### Message Payloads
The bus transmits the `BusMessage` enum, ensuring consumers receive strongly-typed data:
```rust
pub enum BusMessage {
    TickData(Tick),
    QuoteData(Quote),
    OrderData(Order),
    PositionData(Position),
    News(NewsItem),
    Signal(TradingSignal),
    AlertFired(AlertMessage),
    Health(HealthMessage),
}
```

## External Communication: Port Contracts

The application core interacts with external services (brokers, databases, feeds) exclusively through Rust traits located in `apex-core/src/ports/`. This Hexagonal Architecture allows the core to remain agnostic of the underlying implementations.

### 1. MarketDataPort
Contract for real-time and historical market data feeds (e.g., Zerodha, Yahoo, Binance).
*   `subscribe(symbols: &[Symbol]) -> Result<TickStream>`: Returns an asynchronous receiver stream of **Tick** structs.
*   `unsubscribe(symbols: &[Symbol]) -> Result<()>`
*   `get_snapshot(symbol: &Symbol) -> Result<Quote>`: Fetches the current state synchronously.
*   `get_historical_ohlcv(...) -> Result<Vec<OHLCV>>`: Fetches historical bars for indicator calculation and backtesting.
*   `health() -> AdapterHealth`: Returns `Healthy`, `Degraded`, or `Unhealthy`.

### 2. ExecutionPort
Contract for broker adapters managing order routing and account state (e.g., IBKR, Alpaca).
*   `place_order(order: &NewOrderRequest) -> Result<OrderId>`
*   `cancel_order(order_id: &OrderId) -> Result<()>`
*   `modify_order(order_id: &OrderId, params: &ModifyParams) -> Result<()>`
*   `get_order_status(order_id: &OrderId) -> Result<Order>`
*   `get_positions() -> Result<Vec<Position>>`: Used by the periodic reconciliation loop.
*   `get_account_balance() -> Result<AccountBalance>`: Evaluated by the **RiskEngine** before every trade.
*   `supported_order_types() -> &[OrderType]`: Advertises broker capabilities.

### 3. NewsPort
Contract for retrieving unstructured text data from the world.
*   `subscribe(filters: NewsFilter) -> Result<NewsStream>`: Returns an asynchronous stream of raw news events.
*   `search(query: &str, since: DateTime<Utc>, limit: usize) -> Result<Vec<NewsItem>>`

### 4. StoragePort
Contract for local data persistence (e.g., TimescaleDB, DuckDB).
*   `write_ticks(ticks: &[Tick]) -> Result<()>`: Invoked every 100ms by the **MarketDataAggregator**.
*   `write_ohlcv(bars: &[OHLCV]) -> Result<()>`
*   `query_ohlcv(params: OHLCVQuery) -> Result<Vec<OHLCV>>`
*   `write_order(order: &Order) -> Result<()>`
*   `update_order(order: &Order) -> Result<()>`
*   `query_orders(params: OrderQuery) -> Result<Vec<Order>>`
*   `write_position(pos: &Position) -> Result<()>`
*   `query_positions(broker_id: &str) -> Result<Vec<Position>>`