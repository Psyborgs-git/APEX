use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::domain::models::*;

/// Risk engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum value of a single order
    pub max_order_value: f64,
    /// Maximum position size as a fraction of portfolio (0.0 - 1.0)
    pub max_position_pct: f64,
    /// Maximum daily loss (absolute value) — HARD STOP
    pub max_daily_loss: f64,
    /// Window in ms to detect duplicate orders
    pub duplicate_window_ms: u64,
    /// Whether to strictly reject orders outside market hours
    pub strict_market_hours: bool,
    /// Conservative default price estimate used for market orders with no price
    pub market_order_price_estimate: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_order_value: 500_000.0,
            max_position_pct: 0.20,
            max_daily_loss: 50_000.0,
            duplicate_window_ms: 500,
            strict_market_hours: false,
            market_order_price_estimate: 100.0,
        }
    }
}

/// Result of a risk check
#[derive(Debug, Clone, PartialEq)]
pub enum RiskVerdict {
    /// Order passed all risk checks
    Pass,
    /// Order rejected with reason
    Reject(String),
}

/// Recent order info for duplicate detection (used in check_duplicate)
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RecentOrder {
    symbol: String,
    side: OrderSide,
    quantity: f64,
    price: Option<f64>,
    timestamp_ms: i64,
}

/// Pre-trade risk engine — all checks are synchronous and run in < 10μs
pub struct RiskEngine {
    config: RiskConfig,
    /// Session P&L in fixed-point (multiplied by 100 for 2 decimal precision)
    session_pnl: Arc<AtomicI64>,
    /// Hard halt flag — when true, ALL orders are rejected
    pub(crate) trading_halted: Arc<AtomicBool>,
    /// Recent orders for duplicate detection
    recent_orders: Arc<Mutex<Vec<RecentOrder>>>,
}

impl RiskEngine {
    /// Create a new risk engine with the given configuration
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            session_pnl: Arc::new(AtomicI64::new(0)),
            trading_halted: Arc::new(AtomicBool::new(false)),
            recent_orders: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Pre-trade risk check — MUST be called before every order submission.
    /// This is synchronous and does no I/O. Target: < 10μs.
    #[tracing::instrument(skip(self, order, account), fields(symbol = %order.symbol.0))]
    pub fn check(&self, order: &NewOrderRequest, account: &AccountBalance) -> RiskVerdict {
        self.check_at(order, account, Utc::now())
    }

    fn check_at(
        &self,
        order: &NewOrderRequest,
        account: &AccountBalance,
        now: DateTime<Utc>,
    ) -> RiskVerdict {
        // 1. FIRST CHECK: Is trading halted? This is UNBYPASSABLE.
        if self.trading_halted.load(Ordering::SeqCst) {
            return RiskVerdict::Reject(
                "Max daily loss reached. Trading halted. Reset required via UI.".into(),
            );
        }

        // 2. Reject outside market hours when configured
        if self.config.strict_market_hours && !Self::is_market_open_utc(now) {
            return RiskVerdict::Reject("Order rejected outside configured market hours".into());
        }

        // 3. Reject duplicate orders in the configured window
        if self.is_duplicate(order, now.timestamp_millis()) {
            return RiskVerdict::Reject("Duplicate order rejected within risk window".into());
        }

        // 4. Check order value
        let price = self.price_for_risk(order);
        let order_value = order.quantity * price;
        if price > 0.0 && order_value > self.config.max_order_value {
            return RiskVerdict::Reject(format!(
                "Order value {:.2} exceeds max allowed {:.2}",
                order_value, self.config.max_order_value
            ));
        }

        // 5. Check resulting position size as % of portfolio
        if account.total_value > 0.0 && price > 0.0 {
            let position_value = order.quantity * price;
            let position_pct = position_value / account.total_value;
            if position_pct > self.config.max_position_pct {
                return RiskVerdict::Reject(format!(
                    "Position would be {:.1}% of portfolio, max allowed is {:.1}%",
                    position_pct * 100.0,
                    self.config.max_position_pct * 100.0
                ));
            }
        }

        // 6. Check session P&L against max daily loss
        let current_pnl = self.session_pnl.load(Ordering::SeqCst) as f64 / 100.0;
        if current_pnl < -self.config.max_daily_loss {
            // Set the halt flag — this is permanent until manual reset
            self.trading_halted.store(true, Ordering::SeqCst);
            return RiskVerdict::Reject(
                "Max daily loss reached. Trading halted. Reset required via UI.".into(),
            );
        }

        self.record_recent_order(order, now.timestamp_millis());
        RiskVerdict::Pass
    }

    fn price_for_risk(&self, order: &NewOrderRequest) -> f64 {
        order.price.or(order.stop_price).unwrap_or_else(|| {
            if order.order_type == OrderType::Market {
                self.config.market_order_price_estimate
            } else {
                0.0
            }
        })
    }

    fn is_duplicate(&self, order: &NewOrderRequest, now_ms: i64) -> bool {
        let Ok(mut recent) = self.recent_orders.lock() else {
            return false;
        };
        let price = Self::price_for_duplicate(order);
        let window = self.config.duplicate_window_ms as i64;
        recent.retain(|entry| now_ms - entry.timestamp_ms <= window);
        recent.iter().any(|entry| {
            entry.symbol == order.symbol.0
                && entry.side == order.side
                && (entry.quantity - order.quantity).abs() < f64::EPSILON
                && Self::prices_equal(entry.price, price)
        })
    }

    fn record_recent_order(&self, order: &NewOrderRequest, now_ms: i64) {
        let Ok(mut recent) = self.recent_orders.lock() else {
            return;
        };
        recent.push(RecentOrder {
            symbol: order.symbol.0.clone(),
            side: order.side.clone(),
            quantity: order.quantity,
            price: Self::price_for_duplicate(order),
            timestamp_ms: now_ms,
        });
    }

    fn price_for_duplicate(order: &NewOrderRequest) -> Option<f64> {
        order.price.or(order.stop_price)
    }

    fn prices_equal(left: Option<f64>, right: Option<f64>) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => (left - right).abs() < 0.000_001,
            (None, None) => true,
            _ => false,
        }
    }

    fn is_market_open_utc(now: DateTime<Utc>) -> bool {
        if matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
            return false;
        }
        let minutes = now.hour() * 60 + now.minute();
        let open = 3 * 60 + 45;
        let close = 10 * 60;
        minutes >= open && minutes <= close
    }

    /// Update session P&L (called after each fill)
    pub fn update_pnl(&self, pnl_change: f64) {
        let change_fixed = (pnl_change * 100.0) as i64;
        let new_pnl = self.session_pnl.fetch_add(change_fixed, Ordering::SeqCst) + change_fixed;

        // Check if we've breached max daily loss
        let current_pnl = new_pnl as f64 / 100.0;
        if current_pnl < -self.config.max_daily_loss {
            warn!(
                "MAX DAILY LOSS BREACHED: {:.2}. Halting all trading.",
                current_pnl
            );
            self.trading_halted.store(true, Ordering::SeqCst);
        }
    }

    /// Get current session P&L
    pub fn session_pnl(&self) -> f64 {
        self.session_pnl.load(Ordering::SeqCst) as f64 / 100.0
    }

    /// Check if trading is halted
    pub fn is_halted(&self) -> bool {
        self.trading_halted.load(Ordering::SeqCst)
    }

    /// Reset the trading halt — ONLY callable from explicit UI action
    pub fn reset_halt(&self) {
        warn!("Trading halt reset by user action");
        self.trading_halted.store(false, Ordering::SeqCst);
        // Reset session P&L
        self.session_pnl.store(0, Ordering::SeqCst);
    }

    /// Get the risk configuration
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn default_account() -> AccountBalance {
        AccountBalance {
            total_value: 1_000_000.0,
            cash: 500_000.0,
            margin_used: 500_000.0,
            margin_available: 500_000.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            currency: "INR".into(),
        }
    }

    fn market_buy(symbol: &str, qty: f64) -> NewOrderRequest {
        NewOrderRequest {
            symbol: Symbol(symbol.into()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: qty,
            price: None,
            stop_price: None,
            tag: None,
        }
    }

    fn limit_buy(symbol: &str, qty: f64, price: f64) -> NewOrderRequest {
        NewOrderRequest {
            symbol: Symbol(symbol.into()),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: qty,
            price: Some(price),
            stop_price: None,
            tag: None,
        }
    }

    #[test]
    fn test_basic_order_passes() {
        let engine = RiskEngine::new(RiskConfig::default());
        let account = default_account();
        let order = limit_buy("AAPL", 10.0, 150.0);
        assert_eq!(engine.check(&order, &account), RiskVerdict::Pass);
    }

    #[test]
    fn test_market_order_no_price_passes() {
        let engine = RiskEngine::new(RiskConfig::default());
        let account = default_account();
        let order = market_buy("AAPL", 10.0);
        // Market orders use the configured price estimate for value checks.
        assert_eq!(engine.check(&order, &account), RiskVerdict::Pass);
    }

    #[test]
    fn test_market_order_uses_estimated_price_for_value_check() {
        let engine = RiskEngine::new(RiskConfig {
            max_order_value: 1_000.0,
            market_order_price_estimate: 100.0,
            ..RiskConfig::default()
        });
        let account = default_account();
        let order = market_buy("AAPL", 11.0);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("exceeds max")),
            _ => panic!("Expected market order value rejection"),
        }
    }

    #[test]
    fn test_max_order_value_rejected() {
        let engine = RiskEngine::new(RiskConfig {
            max_order_value: 100_000.0,
            ..RiskConfig::default()
        });
        let account = default_account();
        // Order value = 1000 * 150 = 150,000 > 100,000
        let order = limit_buy("AAPL", 1000.0, 150.0);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("exceeds max")),
            _ => panic!("Expected rejection"),
        }
    }

    #[test]
    fn test_max_position_pct_rejected() {
        let engine = RiskEngine::new(RiskConfig {
            max_position_pct: 0.10, // 10%
            ..RiskConfig::default()
        });
        let account = default_account(); // total_value = 1,000,000
                                         // Order value = 500 * 250 = 125,000 = 12.5% > 10%
        let order = limit_buy("RELIANCE", 500.0, 250.0);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("portfolio")),
            _ => panic!("Expected rejection"),
        }
    }

    #[test]
    fn test_max_daily_loss_halts_trading() {
        let engine = RiskEngine::new(RiskConfig {
            max_daily_loss: 1000.0,
            ..RiskConfig::default()
        });

        // Simulate a loss
        engine.update_pnl(-1500.0);

        assert!(engine.is_halted());
        assert!(engine.session_pnl() < -1000.0);

        // All subsequent orders should be rejected
        let account = default_account();
        let order = market_buy("AAPL", 1.0);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("halted")),
            _ => panic!("Expected halt rejection"),
        }
    }

    #[test]
    fn test_halt_is_unbypassable() {
        let engine = RiskEngine::new(RiskConfig {
            max_daily_loss: 100.0,
            ..RiskConfig::default()
        });

        engine.update_pnl(-200.0);
        assert!(engine.is_halted());

        // Even a tiny order should be rejected
        let account = default_account();
        let order = limit_buy("AAPL", 0.001, 1.0);
        assert!(matches!(
            engine.check(&order, &account),
            RiskVerdict::Reject(_)
        ));
    }

    #[test]
    fn test_reset_halt() {
        let engine = RiskEngine::new(RiskConfig {
            max_daily_loss: 100.0,
            ..RiskConfig::default()
        });

        engine.update_pnl(-200.0);
        assert!(engine.is_halted());

        // Reset via UI action
        engine.reset_halt();
        assert!(!engine.is_halted());
        assert!((engine.session_pnl() - 0.0).abs() < 0.01);

        // Orders should pass again
        let account = default_account();
        let order = market_buy("AAPL", 1.0);
        assert_eq!(engine.check(&order, &account), RiskVerdict::Pass);
    }

    #[test]
    fn test_pnl_accumulation() {
        let engine = RiskEngine::new(RiskConfig::default());
        engine.update_pnl(100.0);
        engine.update_pnl(-50.0);
        engine.update_pnl(25.0);
        assert!((engine.session_pnl() - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_duplicate_order_rejected() {
        let engine = RiskEngine::new(RiskConfig {
            duplicate_window_ms: 1_000,
            ..RiskConfig::default()
        });
        let account = default_account();
        let order = limit_buy("AAPL", 10.0, 150.0);
        assert_eq!(engine.check(&order, &account), RiskVerdict::Pass);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("Duplicate")),
            _ => panic!("Expected duplicate rejection"),
        }
    }

    #[test]
    fn test_strict_market_hours_rejected_when_closed() {
        let engine = RiskEngine::new(RiskConfig {
            strict_market_hours: true,
            ..RiskConfig::default()
        });
        let account = default_account();
        let order = limit_buy("AAPL", 10.0, 150.0);
        let sunday = Utc.with_ymd_and_hms(2026, 4, 26, 4, 0, 0).unwrap();
        match engine.check_at(&order, &account, sunday) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("market hours")),
            _ => panic!("Expected market-hours rejection"),
        }
    }

    #[test]
    fn test_halt_checked_first() {
        // The halt flag must be checked FIRST, before any other logic
        let engine = RiskEngine::new(RiskConfig {
            max_daily_loss: 100.0,
            ..RiskConfig::default()
        });

        // Halt trading
        engine.trading_halted.store(true, Ordering::SeqCst);

        // Even with a perfectly valid order and large account, it should reject
        let account = AccountBalance {
            total_value: 10_000_000.0,
            cash: 10_000_000.0,
            margin_used: 0.0,
            margin_available: 10_000_000.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            currency: "INR".into(),
        };
        let order = limit_buy("AAPL", 1.0, 1.0);
        match engine.check(&order, &account) {
            RiskVerdict::Reject(msg) => assert!(msg.contains("halted")),
            _ => panic!("Expected halt rejection even with valid order"),
        }
    }
}
