# Adapter Guide — Implementing a New Broker Adapter

This guide walks through implementing a custom broker adapter for APEX.
Adapters are the bridge between APEX's order management and your broker's API.

## Architecture Overview

APEX uses a **hexagonal architecture** (ports & adapters). The core engine
defines port traits in `apex-core/src/ports/`, and adapters implement them in
`apex-adapters/src/`.

```
┌──────────────────────────────────────────────┐
│                  APEX Core                   │
│                                              │
│  ┌──────────┐    ┌───────────────────────┐   │
│  │ Risk     │    │ Order Trade Manager   │   │
│  │ Engine   │───▶│ (OTM)                │   │
│  └──────────┘    └──────────┬────────────┘   │
│                             │                │
│              ┌──────────────┴──────────┐     │
│              │     ExecutionPort       │     │
│              │     (trait / port)      │     │
│              └──────────────┬──────────┘     │
└─────────────────────────────┼────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
         ┌────▼───┐    ┌─────▼────┐    ┌─────▼────┐
         │ Paper  │    │ MockBrkr │    │ Your    │
         │Trading │    │ Adapter  │    │ Adapter │
         └────────┘    └──────────┘    └─────────┘
```

## Port Traits

### ExecutionPort

Every broker adapter must implement `ExecutionPort`:

```rust
// apex-core/src/ports/execution.rs

#[async_trait]
pub trait ExecutionPort: Send + Sync {
    /// Place a new order — returns the broker-assigned order ID
    async fn place_order(&self, order: &NewOrderRequest) -> Result<OrderId>;

    /// Cancel an existing order
    async fn cancel_order(&self, order_id: &OrderId) -> Result<()>;

    /// Modify an existing order (price, quantity, etc.)
    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()>;

    /// Get the current status of an order
    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order>;

    /// Get all current positions
    async fn get_positions(&self) -> Result<Vec<Position>>;

    /// Get account balance
    async fn get_account_balance(&self) -> Result<AccountBalance>;

    /// Unique broker identifier (e.g., "mock_broker")
    fn broker_id(&self) -> &'static str;

    /// Supported order types for this broker
    fn supported_order_types(&self) -> &[OrderType];

    /// Current health status
    fn health(&self) -> AdapterHealth;
}
```

### MarketDataPort (optional)

If your broker also provides market data:

```rust
// apex-core/src/ports/market_data.rs

#[async_trait]
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
```

## Step-by-Step: MockBroker Adapter

Let's build a complete `MockBroker` adapter from scratch.

### Step 1 — Create the adapter file

Create `apex-adapters/src/execution/mock_broker.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use apex_core::domain::models::*;
use apex_core::ports::execution::ExecutionPort;
use apex_core::ports::market_data::AdapterHealth;

/// A mock broker adapter for testing and development.
/// Simulates order placement with configurable fill behavior.
pub struct MockBrokerAdapter {
    state: Arc<RwLock<MockState>>,
    fill_probability: f64,
}

struct MockState {
    orders: HashMap<String, Order>,
    positions: HashMap<String, Position>,
    balance: AccountBalance,
}

impl MockBrokerAdapter {
    /// Create a new MockBrokerAdapter.
    ///
    /// `fill_probability` — probability (0.0–1.0) that an order fills
    /// immediately on placement.
    pub fn new(fill_probability: f64) -> Self {
        Self {
            state: Arc::new(RwLock::new(MockState {
                orders: HashMap::new(),
                positions: HashMap::new(),
                balance: AccountBalance {
                    total_value: 1_000_000.0,
                    cash: 1_000_000.0,
                    margin_used: 0.0,
                    margin_available: 1_000_000.0,
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    currency: "INR".into(),
                },
            })),
            fill_probability,
        }
    }
}

#[async_trait]
impl ExecutionPort for MockBrokerAdapter {
    async fn place_order(&self, order: &NewOrderRequest) -> Result<OrderId> {
        let id = Uuid::new_v4().to_string();
        let mut state = self.state.write().await;

        let status = if rand_fill(self.fill_probability) {
            OrderStatus::Filled
        } else {
            OrderStatus::Pending
        };

        let new_order = Order {
            id: id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            quantity: order.quantity,
            price: order.price,
            stop_price: order.stop_price,
            status,
            filled_qty: if status == OrderStatus::Filled {
                order.quantity
            } else {
                0.0
            },
            avg_price: order.price.unwrap_or(100.0),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "mock_broker".into(),
            source: order.source.clone(),
        };

        state.orders.insert(id.clone(), new_order);
        Ok(id)
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<()> {
        let mut state = self.state.write().await;
        let order = state
            .orders
            .get_mut(order_id)
            .ok_or_else(|| anyhow!("Order not found: {order_id}"))?;
        order.status = OrderStatus::Cancelled;
        order.updated_at = Utc::now();
        Ok(())
    }

    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()> {
        let mut state = self.state.write().await;
        let order = state
            .orders
            .get_mut(order_id)
            .ok_or_else(|| anyhow!("Order not found: {order_id}"))?;

        if let Some(price) = params.new_price {
            order.price = Some(price);
        }
        if let Some(qty) = params.new_quantity {
            order.quantity = qty;
        }
        order.updated_at = Utc::now();
        Ok(())
    }

    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order> {
        let state = self.state.read().await;
        state
            .orders
            .get(order_id)
            .cloned()
            .ok_or_else(|| anyhow!("Order not found: {order_id}"))
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        let state = self.state.read().await;
        Ok(state.positions.values().cloned().collect())
    }

    async fn get_account_balance(&self) -> Result<AccountBalance> {
        let state = self.state.read().await;
        Ok(state.balance.clone())
    }

    fn broker_id(&self) -> &'static str {
        "mock_broker"
    }

    fn supported_order_types(&self) -> &[OrderType] {
        &[OrderType::Market, OrderType::Limit]
    }

    fn health(&self) -> AdapterHealth {
        AdapterHealth::Healthy
    }
}

fn rand_fill(probability: f64) -> bool {
    // Simple deterministic mock — always fills if probability >= 0.5
    probability >= 0.5
}
```

### Step 2 — Register in the module

Add the adapter to `apex-adapters/src/execution/mod.rs`:

```rust
pub mod paper_trading;
pub mod mock_broker;  // ← add this line
```

### Step 3 — Add configuration

Add a section to `config/apex.toml`:

```toml
[execution.mock_broker]
fill_probability = 0.8
```

### Step 4 — Wire it up

In your application bootstrap (e.g., `apex-tauri/src/main.rs`), add a match
arm for the new adapter:

```rust
let execution: Arc<dyn ExecutionPort> = match config.execution.adapter.as_str() {
    "paper" => Arc::new(PaperTradingAdapter::new()),
    "mock_broker" => Arc::new(MockBrokerAdapter::new(0.8)),
    other => panic!("Unknown execution adapter: {other}"),
};
```

## Key Domain Types

The core domain models live in `apex-core/src/domain/models.rs`. Here are the
ones your adapter will interact with:

| Type               | Purpose                                    |
| ------------------ | ------------------------------------------ |
| `NewOrderRequest`  | Incoming order from user or strategy       |
| `Order`            | Full order record with status tracking     |
| `OrderId`          | `String` alias for order identifiers       |
| `OrderStatus`      | Enum: Pending, Filled, Cancelled, Rejected |
| `OrderType`        | Enum: Market, Limit, Stop, StopLimit       |
| `Position`         | Current position in a symbol               |
| `AccountBalance`   | Cash, margin, P&L summary                  |
| `ModifyParams`     | Price/quantity change request              |
| `AdapterHealth`    | Healthy, Degraded(reason), Unhealthy(err)  |

## Testing Your Adapter

Write integration tests in `apex-adapters/tests/`:

```rust
#[tokio::test]
async fn test_mock_broker_place_and_cancel() {
    let adapter = MockBrokerAdapter::new(1.0); // always fill

    let request = NewOrderRequest {
        symbol: "TEST.NS".into(),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: 10.0,
        price: None,
        stop_price: None,
        source: "test".into(),
    };

    let order_id = adapter.place_order(&request).await.unwrap();
    let order = adapter.get_order_status(&order_id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);

    // Cancel should fail on filled orders (or succeed if your broker allows it)
    adapter.cancel_order(&order_id).await.unwrap();
    let updated = adapter.get_order_status(&order_id).await.unwrap();
    assert_eq!(updated.status, OrderStatus::Cancelled);
}
```

## Checklist

- [ ] Implement `ExecutionPort` trait
- [ ] Return meaningful errors (not panics) for invalid operations
- [ ] Implement `health()` to report adapter status
- [ ] Add to `execution/mod.rs`
- [ ] Add configuration section to `apex.toml`
- [ ] Wire up in the application bootstrap
- [ ] Write integration tests
- [ ] Handle reconnection for network-based brokers
- [ ] Log all order state transitions with `tracing`
