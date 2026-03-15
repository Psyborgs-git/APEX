use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::bus::message_bus::{AlertMessage, AlertSeverity, BusMessage, MessageBus, Topic};
use crate::domain::models::*;

/// Alert rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertRule {
    PriceAbove { symbol: String, threshold: f64 },
    PriceBelow { symbol: String, threshold: f64 },
    PctChange { symbol: String, pct: f64, window_secs: u64 },
    VwapCross { symbol: String },
    DailyPnl { threshold: f64 },
    NewsKeyword { pattern: String, symbols: Vec<String> },
}

/// Alert delivery method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertDelivery {
    InApp,
    Sound,
    OsNotification,
    Telegram(String),
}

/// Stored alert rule with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAlert {
    pub id: String,
    pub rule: AlertRule,
    pub delivery: Vec<AlertDelivery>,
    pub enabled: bool,
}

/// Alert Engine — evaluates rules against market data and emits alerts
pub struct AlertEngine {
    bus: Arc<MessageBus>,
    rules: Arc<tokio::sync::RwLock<Vec<StoredAlert>>>,
}

impl AlertEngine {
    /// Create a new alert engine
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            bus,
            rules: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Add a new alert rule
    pub async fn add_rule(&self, alert: StoredAlert) {
        info!("Adding alert rule: {:?}", alert.id);
        self.rules.write().await.push(alert);
    }

    /// Remove an alert rule by ID
    pub async fn remove_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.rules.write().await;
        let len_before = rules.len();
        rules.retain(|r| r.id != rule_id);
        rules.len() < len_before
    }

    /// Get all configured rules
    pub async fn get_rules(&self) -> Vec<StoredAlert> {
        self.rules.read().await.clone()
    }

    /// Evaluate all rules against a quote update
    pub async fn evaluate_quote(&self, quote: &Quote) {
        let rules = self.rules.read().await;
        for stored_alert in rules.iter() {
            if !stored_alert.enabled {
                continue;
            }

            let fired = match &stored_alert.rule {
                AlertRule::PriceAbove { symbol, threshold } => {
                    symbol == &quote.symbol.0 && quote.last > *threshold
                }
                AlertRule::PriceBelow { symbol, threshold } => {
                    symbol == &quote.symbol.0 && quote.last < *threshold
                }
                AlertRule::VwapCross { symbol } => {
                    symbol == &quote.symbol.0 && (quote.last - quote.vwap).abs() < 0.01
                }
                _ => false,
            };

            if fired {
                self.fire_alert(&stored_alert.id, &stored_alert.rule);
            }
        }
    }

    /// Evaluate P&L-based alerts
    pub async fn evaluate_pnl(&self, pnl: f64) {
        let rules = self.rules.read().await;
        for stored_alert in rules.iter() {
            if !stored_alert.enabled {
                continue;
            }

            if let AlertRule::DailyPnl { threshold } = &stored_alert.rule {
                if pnl < *threshold {
                    self.fire_alert(&stored_alert.id, &stored_alert.rule);
                }
            }
        }
    }

    /// Fire an alert — emit to message bus
    fn fire_alert(&self, rule_id: &str, rule: &AlertRule) {
        let message = match rule {
            AlertRule::PriceAbove { symbol, threshold } => {
                format!("{} price above {:.2}", symbol, threshold)
            }
            AlertRule::PriceBelow { symbol, threshold } => {
                format!("{} price below {:.2}", symbol, threshold)
            }
            AlertRule::VwapCross { symbol } => {
                format!("{} crossed VWAP", symbol)
            }
            AlertRule::DailyPnl { threshold } => {
                format!("Daily P&L below {:.2}", threshold)
            }
            AlertRule::PctChange { symbol, pct, .. } => {
                format!("{} changed by {:.2}%", symbol, pct)
            }
            AlertRule::NewsKeyword { pattern, .. } => {
                format!("News keyword match: {}", pattern)
            }
        };

        info!("Alert fired: {}", message);
        self.bus.publish(
            Topic::Alert,
            BusMessage::AlertFired(AlertMessage {
                rule_id: rule_id.to_string(),
                message,
                severity: AlertSeverity::Warning,
            }),
        );
    }

    /// Get the number of configured rules
    pub async fn rule_count(&self) -> usize {
        self.rules.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_quote(symbol: &str, last: f64) -> Quote {
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
    async fn test_add_and_get_rules() {
        let bus = Arc::new(MessageBus::new());
        let engine = AlertEngine::new(bus);

        let alert = StoredAlert {
            id: "test-1".into(),
            rule: AlertRule::PriceAbove { symbol: "AAPL".into(), threshold: 200.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        };

        engine.add_rule(alert).await;
        assert_eq!(engine.rule_count().await, 1);

        let rules = engine.get_rules().await;
        assert_eq!(rules[0].id, "test-1");
    }

    #[tokio::test]
    async fn test_remove_rule() {
        let bus = Arc::new(MessageBus::new());
        let engine = AlertEngine::new(bus);

        engine.add_rule(StoredAlert {
            id: "test-1".into(),
            rule: AlertRule::PriceAbove { symbol: "AAPL".into(), threshold: 200.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        }).await;

        assert!(engine.remove_rule("test-1").await);
        assert_eq!(engine.rule_count().await, 0);
        assert!(!engine.remove_rule("nonexistent").await);
    }

    #[tokio::test]
    async fn test_price_above_alert_fires() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.subscribe(Topic::Alert);
        let engine = AlertEngine::new(bus);

        engine.add_rule(StoredAlert {
            id: "price-above-1".into(),
            rule: AlertRule::PriceAbove { symbol: "AAPL".into(), threshold: 150.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        }).await;

        // Quote above threshold — should fire
        let quote = test_quote("AAPL", 155.0);
        engine.evaluate_quote(&quote).await;

        let msg = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx.recv(),
        ).await;
        assert!(msg.is_ok());
    }

    #[tokio::test]
    async fn test_price_below_no_fire() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.subscribe(Topic::Alert);
        let engine = AlertEngine::new(bus);

        engine.add_rule(StoredAlert {
            id: "price-above-1".into(),
            rule: AlertRule::PriceAbove { symbol: "AAPL".into(), threshold: 200.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        }).await;

        // Quote below threshold — should NOT fire
        let quote = test_quote("AAPL", 150.0);
        engine.evaluate_quote(&quote).await;

        let msg = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx.recv(),
        ).await;
        assert!(msg.is_err()); // Timeout — no message
    }

    #[tokio::test]
    async fn test_disabled_rule_does_not_fire() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.subscribe(Topic::Alert);
        let engine = AlertEngine::new(bus);

        engine.add_rule(StoredAlert {
            id: "disabled-1".into(),
            rule: AlertRule::PriceAbove { symbol: "AAPL".into(), threshold: 100.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: false, // Disabled
        }).await;

        let quote = test_quote("AAPL", 155.0);
        engine.evaluate_quote(&quote).await;

        let msg = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx.recv(),
        ).await;
        assert!(msg.is_err()); // No alert for disabled rule
    }

    #[tokio::test]
    async fn test_pnl_alert() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.subscribe(Topic::Alert);
        let engine = AlertEngine::new(bus);

        engine.add_rule(StoredAlert {
            id: "pnl-1".into(),
            rule: AlertRule::DailyPnl { threshold: -5000.0 },
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        }).await;

        engine.evaluate_pnl(-6000.0).await;

        let msg = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx.recv(),
        ).await;
        assert!(msg.is_ok());
    }
}
