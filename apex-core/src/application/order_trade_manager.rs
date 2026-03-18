use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use tracing::{info, warn};

use crate::bus::message_bus::{BusMessage, MessageBus, Topic};
use crate::domain::models::*;
use crate::ports::execution::ExecutionPort;

use super::risk_engine::{RiskEngine, RiskVerdict};

/// Order & Trade Manager — manages all order lifecycle
pub struct OrderTradeManager {
    risk_engine: Arc<RiskEngine>,
    execution: HashMap<String, Box<dyn ExecutionPort>>,
    bus: Arc<MessageBus>,
    open_orders: Arc<DashMap<String, Order>>,
    positions: Arc<DashMap<String, Position>>,
}

impl OrderTradeManager {
    /// Create a new Order & Trade Manager
    pub fn new(
        risk_engine: Arc<RiskEngine>,
        bus: Arc<MessageBus>,
    ) -> Self {
        Self {
            risk_engine,
            execution: HashMap::new(),
            bus,
            open_orders: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
        }
    }

    /// Register an execution adapter
    pub fn register_execution(&mut self, broker_id: String, adapter: Box<dyn ExecutionPort>) {
        info!("Registering execution adapter: {}", broker_id);
        self.execution.insert(broker_id, adapter);
    }

    /// Submit a new order — risk check → journal → dispatch → update
    pub async fn submit_order(
        &self,
        request: NewOrderRequest,
        broker_id: &str,
    ) -> Result<OrderId> {
        // 1. Risk check (sync, no I/O) — halt check is FIRST inside check()
        let account = self.get_account_balance(broker_id).await?;
        let verdict = self.risk_engine.check(&request, &account);
        match verdict {
            RiskVerdict::Pass => {}
            RiskVerdict::Reject(reason) => {
                warn!("Order rejected by risk engine: {}", reason);
                return Err(anyhow!("Risk check failed: {}", reason));
            }
        }

        // 2. Get the execution adapter
        let adapter = self.execution.get(broker_id)
            .ok_or_else(|| anyhow!("No execution adapter found for broker: {}", broker_id))?;

        // 3. Dispatch to broker adapter
        let order_id = adapter.place_order(&request).await?;

        // 4. Get order status and track it
        if let Ok(order) = adapter.get_order_status(&order_id).await {
            self.open_orders.insert(order_id.0.clone(), order.clone());

            // 5. Publish OrderUpdate to message bus
            self.bus.publish(
                Topic::OrderUpdate(order_id.0.clone()),
                BusMessage::OrderData(order),
            );
        }

        Ok(order_id)
    }

    /// Cancel an order
    pub async fn cancel_order(&self, order_id: &OrderId, broker_id: &str) -> Result<()> {
        let adapter = self.execution.get(broker_id)
            .ok_or_else(|| anyhow!("No execution adapter found for broker: {}", broker_id))?;

        adapter.cancel_order(order_id).await?;

        // Update local tracking
        if let Some(mut order) = self.open_orders.get_mut(&order_id.0) {
            order.status = OrderStatus::Cancelled;
        }

        self.bus.publish(
            Topic::OrderUpdate(order_id.0.clone()),
            BusMessage::OrderData(
                self.open_orders.get(&order_id.0)
                    .map(|o| o.clone())
                    .unwrap_or_else(|| Order {
                        id: order_id.clone(),
                        symbol: Symbol("".into()),
                        side: OrderSide::Buy,
                        order_type: OrderType::Market,
                        quantity: 0.0,
                        price: None,
                        stop_price: None,
                        status: OrderStatus::Cancelled,
                        filled_qty: 0.0,
                        avg_price: 0.0,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        broker_id: broker_id.to_string(),
                        source: "manual".into(),
                    })
            ),
        );

        Ok(())
    }

    /// Handle a fill event — update positions and P&L
    pub async fn handle_fill(&self, fill: FillEvent) {
        // 1. Update order status
        if let Some(mut order) = self.open_orders.get_mut(&fill.order_id.0) {
            if fill.quantity >= order.quantity {
                order.status = OrderStatus::Filled;
            } else {
                order.status = OrderStatus::PartiallyFilled;
            }
            order.filled_qty = fill.quantity;
            order.avg_price = fill.price;
            order.updated_at = chrono::Utc::now();

            self.bus.publish(
                Topic::OrderUpdate(fill.order_id.0.clone()),
                BusMessage::OrderData(order.clone()),
            );
        }

        // 2. Update position
        let symbol_key = fill.symbol.0.clone();
        let pnl = if let Some(mut pos) = self.positions.get_mut(&symbol_key) {
            // Existing position — calculate P&L
            let pnl = match (&pos.side, &fill.side) {
                (OrderSide::Buy, OrderSide::Sell) => {
                    (fill.price - pos.avg_price) * fill.quantity
                }
                (OrderSide::Sell, OrderSide::Buy) => {
                    (pos.avg_price - fill.price) * fill.quantity
                }
                _ => 0.0, // Adding to position
            };
            pos.pnl += pnl;
            pnl
        } else {
            // New position
            self.positions.insert(symbol_key.clone(), Position {
                symbol: fill.symbol.clone(),
                quantity: fill.quantity,
                avg_price: fill.price,
                side: fill.side,
                pnl: 0.0,
                pnl_pct: 0.0,
                broker_id: fill.broker_id,
            });
            0.0
        };

        // 3. Update session P&L in risk engine
        if pnl != 0.0 {
            self.risk_engine.update_pnl(pnl - fill.commission);
        }

        // 4. Publish position update
        self.bus.publish(Topic::PositionUpdate, BusMessage::PositionData(
            self.positions.get(&fill.symbol.0)
                .map(|p| p.clone())
                .unwrap_or_else(|| Position {
                    symbol: fill.symbol,
                    quantity: 0.0,
                    avg_price: 0.0,
                    side: OrderSide::Buy,
                    pnl: 0.0,
                    pnl_pct: 0.0,
                    broker_id: String::new(),
                })
        ));
    }

    /// Reconcile positions with broker
    pub async fn reconcile_positions(&self, broker_id: &str) -> Result<()> {
        let adapter = self.execution.get(broker_id)
            .ok_or_else(|| anyhow!("No execution adapter found for broker: {}", broker_id))?;

        let broker_positions = adapter.get_positions().await?;

        for broker_pos in broker_positions {
            let key = broker_pos.symbol.0.clone();
            if let Some(mut local_pos) = self.positions.get_mut(&key) {
                if (local_pos.quantity - broker_pos.quantity).abs() > 0.001 {
                    warn!(
                        "Position discrepancy for {}: local={}, broker={}. Updating to broker state.",
                        key, local_pos.quantity, broker_pos.quantity
                    );
                    *local_pos = broker_pos;
                }
            } else {
                info!("New position from broker reconciliation: {} qty={}", key, broker_pos.quantity);
                self.positions.insert(key, broker_pos);
            }
        }

        Ok(())
    }

    /// Get account balance from a broker
    async fn get_account_balance(&self, broker_id: &str) -> Result<AccountBalance> {
        let adapter = self.execution.get(broker_id)
            .ok_or_else(|| anyhow!("No execution adapter found for broker: {}", broker_id))?;
        adapter.get_account_balance().await
    }

    /// Get all open orders
    pub fn open_orders(&self) -> Vec<Order> {
        self.open_orders.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all positions
    pub fn get_positions(&self) -> Vec<Position> {
        self.positions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get the number of registered execution adapters
    pub fn execution_adapter_count(&self) -> usize {
        self.execution.len()
    }

    /// Get the list of registered broker IDs
    pub fn broker_ids(&self) -> Vec<String> {
        self.execution.keys().cloned().collect()
    }

    /// Start a periodic position reconciliation loop.
    ///
    /// Every `interval` seconds, reconcile positions with all registered
    /// brokers. This runs as a background Tokio task and logs any
    /// discrepancies.
    pub fn start_reconciliation_loop(
        otm: Arc<Self>,
        interval: std::time::Duration,
    ) {
        tokio::spawn(async move {
            info!("Starting position reconciliation loop (interval: {:?})", interval);
            loop {
                tokio::time::sleep(interval).await;

                let broker_ids = otm.broker_ids();
                for broker_id in &broker_ids {
                    if let Err(e) = otm.reconcile_positions(broker_id).await {
                        warn!(broker_id = %broker_id, error = %e, "Reconciliation failed");
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::risk_engine::RiskConfig;

    #[test]
    fn test_create_otm() {
        let bus = Arc::new(MessageBus::new());
        let risk = Arc::new(RiskEngine::new(RiskConfig::default()));
        let otm = OrderTradeManager::new(risk, bus);
        assert_eq!(otm.execution_adapter_count(), 0);
        assert_eq!(otm.open_orders().len(), 0);
        assert_eq!(otm.get_positions().len(), 0);
    }

    #[tokio::test]
    async fn test_submit_order_no_adapter() {
        let bus = Arc::new(MessageBus::new());
        let risk = Arc::new(RiskEngine::new(RiskConfig::default()));
        let otm = OrderTradeManager::new(risk, bus);

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };

        let result = otm.submit_order(request, "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_fill_creates_position() {
        let bus = Arc::new(MessageBus::new());
        let risk = Arc::new(RiskEngine::new(RiskConfig::default()));
        let otm = OrderTradeManager::new(risk, bus);

        let fill = FillEvent {
            order_id: OrderId("test-1".into()),
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            quantity: 10.0,
            price: 150.0,
            commission: 0.5,
            filled_at: chrono::Utc::now(),
            broker_id: "paper".into(),
        };

        otm.handle_fill(fill).await;

        let positions = otm.get_positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity, 10.0);
        assert!((positions[0].avg_price - 150.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_handle_fill_updates_risk_pnl() {
        let bus = Arc::new(MessageBus::new());
        let risk = Arc::new(RiskEngine::new(RiskConfig::default()));
        let otm = OrderTradeManager::new(risk.clone(), bus);

        // Create initial position
        let fill1 = FillEvent {
            order_id: OrderId("test-1".into()),
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            quantity: 10.0,
            price: 150.0,
            commission: 0.0,
            filled_at: chrono::Utc::now(),
            broker_id: "paper".into(),
        };
        otm.handle_fill(fill1).await;

        // Sell at a loss
        let fill2 = FillEvent {
            order_id: OrderId("test-2".into()),
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Sell,
            quantity: 10.0,
            price: 140.0,
            commission: 1.0,
            filled_at: chrono::Utc::now(),
            broker_id: "paper".into(),
        };
        otm.handle_fill(fill2).await;

        // P&L should be (140-150) * 10 - 1.0 = -101.0
        assert!(risk.session_pnl() < 0.0);
    }
}
