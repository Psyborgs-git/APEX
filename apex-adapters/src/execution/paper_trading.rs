use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use apex_core::domain::models::*;
use apex_core::ports::execution::ExecutionPort;
use apex_core::ports::market_data::AdapterHealth;

/// Default slippage in basis points
const DEFAULT_SLIPPAGE_BPS: f64 = 2.0;
/// Default commission in basis points
const DEFAULT_COMMISSION_BPS: f64 = 3.0;

/// Internal state for the paper trading engine
#[derive(Debug, Clone)]
pub struct PaperState {
    pub cash: f64,
    pub initial_capital: f64,
    pub orders: HashMap<String, Order>,
    pub positions: HashMap<String, Position>,
    pub realized_pnl: f64,
    pub order_counter: u64,
}

impl PaperState {
    fn new(initial_capital: f64) -> Self {
        Self {
            cash: initial_capital,
            initial_capital,
            orders: HashMap::new(),
            positions: HashMap::new(),
            realized_pnl: 0.0,
            order_counter: 0,
        }
    }
}

/// Paper trading adapter — always available, simulates fills locally
pub struct PaperTradingAdapter {
    #[allow(dead_code)]
    initial_capital: f64,
    currency: String,
    slippage_bps: f64,
    commission_bps: f64,
    state: Arc<RwLock<PaperState>>,
    quote_cache: Arc<DashMap<String, Quote>>,
}

impl PaperTradingAdapter {
    /// Create a new paper trading adapter with defaults
    pub fn new() -> Self {
        Self::with_config(
            1_000_000.0,
            "INR".into(),
            DEFAULT_SLIPPAGE_BPS,
            DEFAULT_COMMISSION_BPS,
        )
    }

    /// Create a new paper trading adapter with custom configuration
    pub fn with_config(
        initial_capital: f64,
        currency: String,
        slippage_bps: f64,
        commission_bps: f64,
    ) -> Self {
        Self {
            initial_capital,
            currency,
            slippage_bps,
            commission_bps,
            state: Arc::new(RwLock::new(PaperState::new(initial_capital))),
            quote_cache: Arc::new(DashMap::new()),
        }
    }

    /// Update the quote cache (called by the market data aggregator)
    pub fn update_quote(&self, quote: &Quote) {
        self.quote_cache
            .insert(quote.symbol.0.clone(), quote.clone());
    }

    /// Get the quote cache (for external access)
    pub fn quote_cache(&self) -> Arc<DashMap<String, Quote>> {
        self.quote_cache.clone()
    }

    /// Calculate fill price with slippage
    fn calculate_fill_price(&self, last_price: f64, side: &OrderSide) -> f64 {
        let slippage = last_price * (self.slippage_bps / 10_000.0);
        match side {
            OrderSide::Buy => last_price + slippage,
            OrderSide::Sell => last_price - slippage,
        }
    }

    /// Calculate commission for a trade
    fn calculate_commission(&self, value: f64) -> f64 {
        value * (self.commission_bps / 10_000.0)
    }

    /// Process a market order fill immediately
    async fn fill_market_order(&self, order_id: &str, request: &NewOrderRequest) -> Result<()> {
        let symbol_key = &request.symbol.0;
        let quote = self
            .quote_cache
            .get(symbol_key)
            .map(|q| q.clone())
            .ok_or_else(|| anyhow!("No quote available for symbol {}", symbol_key))?;

        let fill_price = self.calculate_fill_price(quote.last, &request.side);
        let trade_value = fill_price * request.quantity;
        let commission = self.calculate_commission(trade_value);

        let mut state = self.state.write().await;

        // Check sufficient funds for buy
        if request.side == OrderSide::Buy && (trade_value + commission) > state.cash {
            if let Some(order) = state.orders.get_mut(order_id) {
                order.status = OrderStatus::Rejected;
                order.updated_at = Utc::now();
            }
            return Err(anyhow!(
                "Insufficient funds: need {:.2}, have {:.2}",
                trade_value + commission,
                state.cash
            ));
        }

        // Execute the fill
        match request.side {
            OrderSide::Buy => {
                state.cash -= trade_value + commission;
            }
            OrderSide::Sell => {
                state.cash += trade_value - commission;
            }
        }

        // Update order status
        if let Some(order) = state.orders.get_mut(order_id) {
            order.status = OrderStatus::Filled;
            order.filled_qty = request.quantity;
            order.avg_price = fill_price;
            order.updated_at = Utc::now();
        }

        // Update position
        self.update_position(&mut state, request, fill_price);

        Ok(())
    }

    /// Update position after a fill
    fn update_position(
        &self,
        state: &mut PaperState,
        request: &NewOrderRequest,
        fill_price: f64,
    ) {
        let symbol_key = request.symbol.0.clone();

        if let Some(pos) = state.positions.get_mut(&symbol_key) {
            match (&pos.side, &request.side) {
                // Adding to position
                (OrderSide::Buy, OrderSide::Buy) | (OrderSide::Sell, OrderSide::Sell) => {
                    let total_qty = pos.quantity + request.quantity;
                    pos.avg_price = (pos.avg_price * pos.quantity
                        + fill_price * request.quantity)
                        / total_qty;
                    pos.quantity = total_qty;
                }
                // Reducing or closing position
                _ => {
                    if request.quantity >= pos.quantity {
                        // Close the position
                        let pnl = match pos.side {
                            OrderSide::Buy => (fill_price - pos.avg_price) * pos.quantity,
                            OrderSide::Sell => (pos.avg_price - fill_price) * pos.quantity,
                        };
                        state.realized_pnl += pnl;

                        let remaining = request.quantity - pos.quantity;
                        if remaining > 0.0 {
                            // Open a new position in the opposite direction
                            state.positions.insert(
                                symbol_key,
                                Position {
                                    symbol: request.symbol.clone(),
                                    quantity: remaining,
                                    avg_price: fill_price,
                                    side: request.side.clone(),
                                    pnl: 0.0,
                                    pnl_pct: 0.0,
                                    broker_id: "paper".into(),
                                },
                            );
                        } else {
                            state.positions.remove(&request.symbol.0);
                        }
                        return;
                    } else {
                        // Partial close
                        let pnl = match pos.side {
                            OrderSide::Buy => (fill_price - pos.avg_price) * request.quantity,
                            OrderSide::Sell => (pos.avg_price - fill_price) * request.quantity,
                        };
                        state.realized_pnl += pnl;
                        pos.quantity -= request.quantity;
                    }
                }
            }

            // Update unrealized P&L based on last price
            if let Some(quote) = self.quote_cache.get(&request.symbol.0) {
                let pos = state.positions.get_mut(&symbol_key).unwrap();
                pos.pnl = match pos.side {
                    OrderSide::Buy => (quote.last - pos.avg_price) * pos.quantity,
                    OrderSide::Sell => (pos.avg_price - quote.last) * pos.quantity,
                };
                if pos.avg_price > 0.0 {
                    pos.pnl_pct = pos.pnl / (pos.avg_price * pos.quantity) * 100.0;
                }
            }
        } else {
            // New position
            state.positions.insert(
                symbol_key,
                Position {
                    symbol: request.symbol.clone(),
                    quantity: request.quantity,
                    avg_price: fill_price,
                    side: request.side.clone(),
                    pnl: 0.0,
                    pnl_pct: 0.0,
                    broker_id: "paper".into(),
                },
            );
        }
    }
}

impl Default for PaperTradingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionPort for PaperTradingAdapter {
    async fn place_order(&self, request: &NewOrderRequest) -> Result<OrderId> {
        let mut state = self.state.write().await;
        state.order_counter += 1;
        let order_id = format!("PAPER-{}", Uuid::new_v4());

        let order = Order {
            id: OrderId(order_id.clone()),
            symbol: request.symbol.clone(),
            side: request.side.clone(),
            order_type: request.order_type.clone(),
            quantity: request.quantity,
            price: request.price,
            stop_price: request.stop_price,
            status: OrderStatus::Pending,
            filled_qty: 0.0,
            avg_price: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "paper".into(),
            source: request.tag.clone().unwrap_or_else(|| "manual".into()),
        };

        state.orders.insert(order_id.clone(), order);
        drop(state); // Release write lock before fill

        // For market orders, fill immediately
        if request.order_type == OrderType::Market {
            self.fill_market_order(&order_id, request).await?;
        } else {
            // For limit/stop orders, mark as Open (waiting for price trigger)
            let mut state = self.state.write().await;
            if let Some(order) = state.orders.get_mut(&order_id) {
                order.status = OrderStatus::Open;
                order.updated_at = Utc::now();
            }
        }

        Ok(OrderId(order_id))
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<()> {
        let mut state = self.state.write().await;
        let order = state
            .orders
            .get_mut(&order_id.0)
            .ok_or_else(|| anyhow!("Order {} not found", order_id.0))?;

        match order.status {
            OrderStatus::Pending | OrderStatus::Open => {
                order.status = OrderStatus::Cancelled;
                order.updated_at = Utc::now();
                Ok(())
            }
            _ => Err(anyhow!("Cannot cancel order in {:?} status", order.status)),
        }
    }

    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()> {
        let mut state = self.state.write().await;
        let order = state
            .orders
            .get_mut(&order_id.0)
            .ok_or_else(|| anyhow!("Order {} not found", order_id.0))?;

        match order.status {
            OrderStatus::Pending | OrderStatus::Open => {
                if let Some(qty) = params.quantity {
                    order.quantity = qty;
                }
                if let Some(price) = params.price {
                    order.price = Some(price);
                }
                if let Some(stop_price) = params.stop_price {
                    order.stop_price = Some(stop_price);
                }
                order.updated_at = Utc::now();
                Ok(())
            }
            _ => Err(anyhow!("Cannot modify order in {:?} status", order.status)),
        }
    }

    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order> {
        let state = self.state.read().await;
        state
            .orders
            .get(&order_id.0)
            .cloned()
            .ok_or_else(|| anyhow!("Order {} not found", order_id.0))
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        let state = self.state.read().await;
        Ok(state.positions.values().cloned().collect())
    }

    async fn get_account_balance(&self) -> Result<AccountBalance> {
        let state = self.state.read().await;
        let unrealized_pnl: f64 = state.positions.values().map(|p| p.pnl).sum();
        let positions_value: f64 = state
            .positions
            .values()
            .map(|p| p.quantity * p.avg_price)
            .sum();

        Ok(AccountBalance {
            total_value: state.cash + positions_value + unrealized_pnl,
            cash: state.cash,
            margin_used: positions_value,
            margin_available: state.cash,
            unrealized_pnl,
            realized_pnl: state.realized_pnl,
            currency: self.currency.clone(),
        })
    }

    fn broker_id(&self) -> &'static str {
        "paper"
    }

    fn supported_order_types(&self) -> &[OrderType] {
        &[
            OrderType::Market,
            OrderType::Limit,
            OrderType::Stop,
            OrderType::StopLimit,
            OrderType::TrailingStop,
        ]
    }

    fn health(&self) -> AdapterHealth {
        AdapterHealth::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_quote(symbol: &str, last: f64) -> Quote {
        Quote {
            symbol: Symbol(symbol.into()),
            bid: last - 0.05,
            ask: last + 0.05,
            last,
            open: last,
            high: last + 1.0,
            low: last - 1.0,
            volume: 10000,
            change_pct: 0.0,
            vwap: last,
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_paper_trading_default_creation() {
        let adapter = PaperTradingAdapter::new();
        assert_eq!(adapter.broker_id(), "paper");
        assert_eq!(adapter.health(), AdapterHealth::Healthy);
        assert!(adapter.supported_order_types().contains(&OrderType::Market));
    }

    #[tokio::test]
    async fn test_place_market_buy_order() {
        let adapter = PaperTradingAdapter::new();
        adapter.update_quote(&create_test_quote("AAPL", 150.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };

        let order_id = adapter.place_order(&request).await.unwrap();
        let order = adapter.get_order_status(&order_id).await.unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
        assert_eq!(order.filled_qty, 10.0);
        assert!(order.avg_price > 0.0);

        let positions = adapter.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, Symbol("AAPL".into()));
        assert_eq!(positions[0].quantity, 10.0);
    }

    #[tokio::test]
    async fn test_place_market_sell_order() {
        let adapter = PaperTradingAdapter::new();
        adapter.update_quote(&create_test_quote("AAPL", 150.0));

        let buy_req = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };
        adapter.place_order(&buy_req).await.unwrap();

        let sell_req = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };
        adapter.place_order(&sell_req).await.unwrap();

        let positions = adapter.get_positions().await.unwrap();
        assert_eq!(positions.len(), 0);
    }

    #[tokio::test]
    async fn test_account_balance_after_trade() {
        let adapter = PaperTradingAdapter::with_config(100_000.0, "USD".into(), 0.0, 0.0);
        adapter.update_quote(&create_test_quote("AAPL", 100.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };
        adapter.place_order(&request).await.unwrap();

        let balance = adapter.get_account_balance().await.unwrap();
        assert_eq!(balance.currency, "USD");
        assert!((balance.cash - 99_000.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_cancel_open_limit_order() {
        let adapter = PaperTradingAdapter::new();
        adapter.update_quote(&create_test_quote("AAPL", 150.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: 10.0,
            price: Some(140.0),
            stop_price: None,
            tag: None,
        };

        let order_id = adapter.place_order(&request).await.unwrap();
        let order = adapter.get_order_status(&order_id).await.unwrap();
        assert_eq!(order.status, OrderStatus::Open);

        adapter.cancel_order(&order_id).await.unwrap();
        let order = adapter.get_order_status(&order_id).await.unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_modify_order() {
        let adapter = PaperTradingAdapter::new();
        adapter.update_quote(&create_test_quote("AAPL", 150.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: 10.0,
            price: Some(140.0),
            stop_price: None,
            tag: None,
        };

        let order_id = adapter.place_order(&request).await.unwrap();

        let modify = ModifyParams {
            quantity: Some(20.0),
            price: Some(145.0),
            stop_price: None,
        };
        adapter.modify_order(&order_id, &modify).await.unwrap();

        let order = adapter.get_order_status(&order_id).await.unwrap();
        assert_eq!(order.quantity, 20.0);
        assert_eq!(order.price, Some(145.0));
    }

    #[tokio::test]
    async fn test_slippage_applied() {
        let adapter = PaperTradingAdapter::with_config(1_000_000.0, "USD".into(), 10.0, 0.0);
        adapter.update_quote(&create_test_quote("AAPL", 100.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 1.0,
            price: None,
            stop_price: None,
            tag: None,
        };

        let order_id = adapter.place_order(&request).await.unwrap();
        let order = adapter.get_order_status(&order_id).await.unwrap();
        assert!((order.avg_price - 100.10).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_insufficient_funds_rejected() {
        let adapter = PaperTradingAdapter::with_config(100.0, "USD".into(), 0.0, 0.0);
        adapter.update_quote(&create_test_quote("AAPL", 150.0));

        let request = NewOrderRequest {
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 10.0,
            price: None,
            stop_price: None,
            tag: None,
        };

        let result = adapter.place_order(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_paper_always_healthy() {
        let adapter = PaperTradingAdapter::new();
        assert_eq!(adapter.health(), AdapterHealth::Healthy);
    }
}