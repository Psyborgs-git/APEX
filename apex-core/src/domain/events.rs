use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::models::*;

/// Canonical event model shared by the live-trading path and backtesting.
///
/// Every significant thing that happens in the system is represented here so
/// that backtests can replay the same sequence of events that would occur in
/// live trading.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum TradingEvent {
    /// A raw price tick was received from a market data adapter
    TickReceived {
        event_id: Uuid,
        tick: Tick,
        adapter_id: String,
        received_at: DateTime<Utc>,
    },
    /// An OHLCV bar was produced (aggregated or from historical data)
    BarReceived {
        event_id: Uuid,
        bar: OHLCV,
        source: String,
    },
    /// A new order was submitted to a broker
    OrderSubmitted {
        event_id: Uuid,
        order_id: OrderId,
        request: NewOrderRequest,
        broker_id: String,
        submitted_at: DateTime<Utc>,
    },
    /// An order fill was received from the broker
    OrderFilled {
        event_id: Uuid,
        fill: FillEvent,
    },
    /// An order was rejected (by risk engine or broker)
    OrderRejected {
        event_id: Uuid,
        order_id: Option<OrderId>,
        symbol: Symbol,
        side: OrderSide,
        quantity: f64,
        reason: String,
        rejected_at: DateTime<Utc>,
    },
    /// An order was cancelled
    OrderCancelled {
        event_id: Uuid,
        order_id: OrderId,
        broker_id: String,
        cancelled_at: DateTime<Utc>,
    },
    /// A partial fill was received
    OrderPartiallyFilled {
        event_id: Uuid,
        order_id: OrderId,
        fill: FillEvent,
        remaining_qty: f64,
    },
    /// Pre-trade risk engine decision
    RiskDecision {
        event_id: Uuid,
        symbol: Symbol,
        side: OrderSide,
        quantity: f64,
        passed: bool,
        reason: Option<String>,
        decided_at: DateTime<Utc>,
    },
    /// A position was updated
    PositionUpdated {
        event_id: Uuid,
        position: Position,
        updated_at: DateTime<Utc>,
    },
    /// A trading signal was emitted by a strategy
    SignalEmitted {
        event_id: Uuid,
        signal: TradingSignal,
    },
    /// A strategy lifecycle transition occurred
    StrategyLifecycleChanged {
        event_id: Uuid,
        strategy_id: String,
        old_state: String,
        new_state: String,
        changed_at: DateTime<Utc>,
    },
    /// Session P&L crossed the daily loss limit
    DailyLossLimitBreached {
        event_id: Uuid,
        session_pnl: f64,
        limit: f64,
        breached_at: DateTime<Utc>,
    },
}

impl TradingEvent {
    /// Return the stable UUID for this event (for idempotent persistence).
    pub fn event_id(&self) -> Uuid {
        match self {
            Self::TickReceived { event_id, .. } => *event_id,
            Self::BarReceived { event_id, .. } => *event_id,
            Self::OrderSubmitted { event_id, .. } => *event_id,
            Self::OrderFilled { event_id, .. } => *event_id,
            Self::OrderRejected { event_id, .. } => *event_id,
            Self::OrderCancelled { event_id, .. } => *event_id,
            Self::OrderPartiallyFilled { event_id, .. } => *event_id,
            Self::RiskDecision { event_id, .. } => *event_id,
            Self::PositionUpdated { event_id, .. } => *event_id,
            Self::SignalEmitted { event_id, .. } => *event_id,
            Self::StrategyLifecycleChanged { event_id, .. } => *event_id,
            Self::DailyLossLimitBreached { event_id, .. } => *event_id,
        }
    }

    /// Human-readable event type tag (mirrors the serde `event_type` field).
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::TickReceived { .. } => "tick_received",
            Self::BarReceived { .. } => "bar_received",
            Self::OrderSubmitted { .. } => "order_submitted",
            Self::OrderFilled { .. } => "order_filled",
            Self::OrderRejected { .. } => "order_rejected",
            Self::OrderCancelled { .. } => "order_cancelled",
            Self::OrderPartiallyFilled { .. } => "order_partially_filled",
            Self::RiskDecision { .. } => "risk_decision",
            Self::PositionUpdated { .. } => "position_updated",
            Self::SignalEmitted { .. } => "signal_emitted",
            Self::StrategyLifecycleChanged { .. } => "strategy_lifecycle_changed",
            Self::DailyLossLimitBreached { .. } => "daily_loss_limit_breached",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id_round_trip() {
        let id = Uuid::new_v4();
        let event = TradingEvent::BarReceived {
            event_id: id,
            bar: OHLCV {
                time: Utc::now(),
                symbol: Symbol("AAPL".into()),
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1,
            },
            source: "test".into(),
        };
        assert_eq!(event.event_id(), id);
        assert_eq!(event.event_type(), "bar_received");
    }

    #[test]
    fn test_event_serialization_round_trip() {
        let event = TradingEvent::OrderRejected {
            event_id: Uuid::new_v4(),
            order_id: None,
            symbol: Symbol("RELIANCE".into()),
            side: OrderSide::Buy,
            quantity: 10.0,
            reason: "Risk limit exceeded".into(),
            rejected_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("order_rejected"));
        let de: TradingEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(de.event_type(), "order_rejected");
    }

    #[test]
    fn test_all_event_types_have_ids() {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let sym = Symbol("X".into());
        let events: Vec<TradingEvent> = vec![
            TradingEvent::TickReceived {
                event_id: id,
                tick: Tick {
                    time: now,
                    symbol: sym.clone(),
                    bid: 1.0,
                    ask: 1.0,
                    last: 1.0,
                    volume: 1,
                    source: "t".into(),
                },
                adapter_id: "a".into(),
                received_at: now,
            },
            TradingEvent::RiskDecision {
                event_id: id,
                symbol: sym.clone(),
                side: OrderSide::Sell,
                quantity: 1.0,
                passed: false,
                reason: Some("halt".into()),
                decided_at: now,
            },
            TradingEvent::DailyLossLimitBreached {
                event_id: id,
                session_pnl: -2000.0,
                limit: 1000.0,
                breached_at: now,
            },
        ];
        for ev in &events {
            assert_eq!(ev.event_id(), id);
        }
    }
}
