use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical symbol representation (e.g. "RELIANCE", "AAPL", "BTC/USDT")
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Symbol(pub String);

/// A single price tick from a market data feed
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

/// A full quote snapshot for a symbol
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

/// OHLCV bar (candlestick)
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

/// Order side
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
}

/// Order lifecycle status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Unique order identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OrderId(pub String);

/// A full order record
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
    pub source:      String,
}

/// A trading position
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

/// A news item from any feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id:          Uuid,
    pub headline:    String,
    pub summary:     String,
    pub source:      String,
    pub url:         String,
    pub published:   DateTime<Utc>,
    pub symbols:     Vec<Symbol>,
    pub sentiment:   Option<f32>,
}

/// A trading signal emitted by a strategy
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

/// Signal action type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalAction {
    Buy,
    Sell,
    Close,
}

/// Request to create a new order
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

/// Parameters for modifying an existing order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyParams {
    pub quantity:   Option<f64>,
    pub price:      Option<f64>,
    pub stop_price: Option<f64>,
}

/// Account balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub total_value:    f64,
    pub cash:           f64,
    pub margin_used:    f64,
    pub margin_available: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl:   f64,
    pub currency:       String,
}

/// Parameters for querying OHLCV data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCVQuery {
    pub symbol:    Symbol,
    pub timeframe: Timeframe,
    pub from:      DateTime<Utc>,
    pub to:        DateTime<Utc>,
    pub limit:     Option<usize>,
}

/// Parameters for querying orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderQuery {
    pub symbol:    Option<Symbol>,
    pub status:    Option<OrderStatus>,
    pub broker_id: Option<String>,
    pub from:      Option<DateTime<Utc>>,
    pub to:        Option<DateTime<Utc>>,
    pub limit:     Option<usize>,
}

/// Timeframe for OHLCV data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Timeframe {
    S1, S5, S15,
    M1, M3, M5, M15, M30,
    H1, H4,
    D1, W1,
}

/// News filter for subscribing to news feeds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsFilter {
    pub symbols:   Option<Vec<Symbol>>,
    pub sources:   Option<Vec<String>>,
    pub keywords:  Option<Vec<String>>,
}

/// Fill event from broker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillEvent {
    pub order_id:   OrderId,
    pub symbol:     Symbol,
    pub side:       OrderSide,
    pub quantity:   f64,
    pub price:      f64,
    pub commission: f64,
    pub filled_at:  DateTime<Utc>,
    pub broker_id:  String,
}

// ---------------------------------------------------------------------------
// P1 — Fill simulation configuration for realistic backtest fills
// ---------------------------------------------------------------------------

/// Configuration for fill simulation in backtesting.
///
/// Enables realistic simulation of spreads, partial fills, simulated latency,
/// and randomized order rejections to match live-trading behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillSimulation {
    /// Half-spread added/subtracted to the close price (in basis points).
    /// E.g. 5 bps → buy fills at close * (1 + 0.0005).
    pub spread_bps: f64,
    /// Fraction of orders that receive only a partial fill on the first
    /// attempt (0.0 = never, 1.0 = always).
    pub partial_fill_rate: f64,
    /// When a partial fill occurs, this fraction of the original quantity is
    /// filled (0.0 – 1.0).
    pub partial_fill_fraction: f64,
    /// Simulated fill latency in milliseconds (added to the bar timestamp).
    pub latency_ms: u64,
    /// Fraction of orders that are randomly rejected by the simulated broker
    /// (0.0 = never reject, 0.05 = 5% rejection rate).
    pub broker_reject_rate: f64,
}

impl Default for FillSimulation {
    fn default() -> Self {
        Self {
            spread_bps: 2.0,
            partial_fill_rate: 0.0,
            partial_fill_fraction: 0.5,
            latency_ms: 0,
            broker_reject_rate: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// P2 — Instrument metadata, exchange calendar, and corporate actions
// ---------------------------------------------------------------------------

/// Type of financial instrument
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentType {
    Equity,
    Future,
    Option,
    Forex,
    Crypto,
    ETF,
    Index,
    Bond,
}

/// Static reference data about a financial instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentMetadata {
    /// Canonical symbol (e.g. "RELIANCE", "AAPL", "BTC/USDT")
    pub symbol:          Symbol,
    /// Human-readable name
    pub name:            String,
    /// Exchange mic code (e.g. "NSE", "NYSE", "XBOM")
    pub exchange:        String,
    /// Instrument type
    pub instrument_type: InstrumentType,
    /// Primary sector / asset class
    pub sector:          Option<String>,
    /// Quote currency (e.g. "INR", "USD")
    pub currency:        String,
    /// Minimum tradeable lot size
    pub lot_size:        f64,
    /// Minimum price tick
    pub tick_size:       f64,
    /// ISIN (if available)
    pub isin:            Option<String>,
    /// Date the instrument was listed
    pub listing_date:    Option<DateTime<Utc>>,
    /// True if the instrument is currently actively traded
    pub is_active:       bool,
}

/// Type of corporate action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorporateActionType {
    Split,
    ReverseSplit,
    Dividend,
    SpecialDividend,
    BonusIssue,
    RightsIssue,
    Merger,
    Spinoff,
    Delisting,
}

/// A corporate action that affects historical price series
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorporateAction {
    pub id:          Uuid,
    pub symbol:      Symbol,
    pub action_type: CorporateActionType,
    /// Ex-date (the first day the stock trades without the right to the action)
    pub ex_date:     DateTime<Utc>,
    /// Adjustment ratio for splits/bonuses (e.g. 2.0 for a 2:1 split)
    pub ratio:       Option<f64>,
    /// Cash amount for dividends (per share, in instrument currency)
    pub amount:      Option<f64>,
    pub description: String,
}

/// A named session within a trading day
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionType {
    PreMarket,
    RegularMarket,
    PostMarket,
    AfterHours,
}

/// One trading session on an exchange (times in UTC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSession {
    pub exchange:     String,
    pub session_type: SessionType,
    /// UTC open time (hh:mm)
    pub open_hhmm:    String,
    /// UTC close time (hh:mm)
    pub close_hhmm:   String,
    /// ISO weekdays the session is active (1=Mon … 7=Sun)
    pub weekdays:     Vec<u8>,
}

/// Query parameters for instrument metadata lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentQuery {
    pub symbol:          Option<Symbol>,
    pub exchange:        Option<String>,
    pub instrument_type: Option<InstrumentType>,
    pub is_active:       Option<bool>,
    pub limit:           Option<usize>,
}

// ---------------------------------------------------------------------------
// P4 — ML experiment tracking metadata
// ---------------------------------------------------------------------------

/// Status of a single ML experiment run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Importance of a single feature in a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureImportance {
    pub feature_name: String,
    pub importance:   f64,
    pub rank:         usize,
}

/// Data drift measurement for a single feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftMeasurement {
    pub feature_name:       String,
    /// Population Stability Index — PSI > 0.2 is a strong signal of drift
    pub psi:                f64,
    /// Kolmogorov-Smirnov statistic
    pub ks_stat:            f64,
    /// True if this feature is considered drifted
    pub is_drifted:         bool,
    pub measured_at:        DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_creation() {
        let s = Symbol("AAPL".into());
        assert_eq!(s.0, "AAPL");
    }

    #[test]
    fn test_order_status_equality() {
        assert_eq!(OrderStatus::Pending, OrderStatus::Pending);
        assert_ne!(OrderStatus::Pending, OrderStatus::Filled);
    }

    #[test]
    fn test_tick_serialization_roundtrip() {
        let tick = Tick {
            time: Utc::now(),
            symbol: Symbol("RELIANCE".into()),
            bid: 2500.50,
            ask: 2501.00,
            last: 2500.75,
            volume: 1000,
            source: "test".into(),
        };
        let json = serde_json::to_string(&tick).unwrap();
        let deserialized: Tick = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.symbol, tick.symbol);
        assert_eq!(deserialized.bid, tick.bid);
    }

    #[test]
    fn test_order_serialization_roundtrip() {
        let order = Order {
            id: OrderId("ord-001".into()),
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: 10.0,
            price: Some(150.0),
            stop_price: None,
            status: OrderStatus::Pending,
            filled_qty: 0.0,
            avg_price: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "paper".into(),
            source: "manual".into(),
        };
        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, order.id);
        assert_eq!(deserialized.side, OrderSide::Buy);
    }

    #[test]
    fn test_new_order_request_serialization() {
        let req = NewOrderRequest {
            symbol: Symbol("BTC/USDT".into()),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            quantity: 0.5,
            price: None,
            stop_price: None,
            tag: Some("strategy_1".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: NewOrderRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.symbol, req.symbol);
    }

    #[test]
    fn test_timeframe_variants() {
        let tf = Timeframe::M5;
        let json = serde_json::to_string(&tf).unwrap();
        assert_eq!(json, "\"M5\"");
    }

    #[test]
    fn test_fill_simulation_default() {
        let sim = FillSimulation::default();
        assert_eq!(sim.spread_bps, 2.0);
        assert_eq!(sim.partial_fill_rate, 0.0);
        assert_eq!(sim.broker_reject_rate, 0.0);
    }

    #[test]
    fn test_instrument_metadata_creation() {
        let meta = InstrumentMetadata {
            symbol: Symbol("RELIANCE".into()),
            name: "Reliance Industries".into(),
            exchange: "NSE".into(),
            instrument_type: InstrumentType::Equity,
            sector: Some("Energy".into()),
            currency: "INR".into(),
            lot_size: 1.0,
            tick_size: 0.05,
            isin: Some("INE002A01018".into()),
            listing_date: None,
            is_active: true,
        };
        assert_eq!(meta.symbol.0, "RELIANCE");
        assert_eq!(meta.exchange, "NSE");
    }

    #[test]
    fn test_corporate_action_serialization() {
        let action = CorporateAction {
            id: Uuid::new_v4(),
            symbol: Symbol("AAPL".into()),
            action_type: CorporateActionType::Split,
            ex_date: Utc::now(),
            ratio: Some(4.0),
            amount: None,
            description: "4:1 stock split".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("Split"));
    }

    #[test]
    fn test_drift_measurement_fields() {
        let drift = DriftMeasurement {
            feature_name: "rsi_14".into(),
            psi: 0.25,
            ks_stat: 0.18,
            is_drifted: true,
            measured_at: Utc::now(),
        };
        assert!(drift.is_drifted);
        assert!(drift.psi > 0.2);
    }
}
