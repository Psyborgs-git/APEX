use serde::{Deserialize, Serialize};

use apex_core::domain::models;

/// Serialize a serde-enabled value to a bare string (strips surrounding quotes).
fn enum_to_string<T: Serialize>(val: &T) -> String {
    serde_json::to_string(val)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// Quote DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteDto {
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub volume: u64,
    pub change_pct: f64,
    pub vwap: f64,
    pub updated_at: String,
}

impl From<&models::Quote> for QuoteDto {
    fn from(q: &models::Quote) -> Self {
        Self {
            symbol: q.symbol.0.clone(),
            bid: q.bid,
            ask: q.ask,
            last: q.last,
            open: q.open,
            high: q.high,
            low: q.low,
            volume: q.volume,
            change_pct: q.change_pct,
            vwap: q.vwap,
            updated_at: q.updated_at.to_rfc3339(),
        }
    }
}

/// OHLCV DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCVDto {
    pub time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

impl From<&models::OHLCV> for OHLCVDto {
    fn from(bar: &models::OHLCV) -> Self {
        Self {
            time: bar.time.to_rfc3339(),
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        }
    }
}

/// Order DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDto {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub status: String,
    pub filled_qty: f64,
    pub avg_price: f64,
    pub created_at: String,
    pub updated_at: String,
    pub broker_id: String,
    pub source: String,
}

impl From<&models::Order> for OrderDto {
    fn from(o: &models::Order) -> Self {
        Self {
            id: o.id.0.clone(),
            symbol: o.symbol.0.clone(),
            side: enum_to_string(&o.side),
            order_type: enum_to_string(&o.order_type),
            quantity: o.quantity,
            price: o.price,
            stop_price: o.stop_price,
            status: enum_to_string(&o.status),
            filled_qty: o.filled_qty,
            avg_price: o.avg_price,
            created_at: o.created_at.to_rfc3339(),
            updated_at: o.updated_at.to_rfc3339(),
            broker_id: o.broker_id.clone(),
            source: o.source.clone(),
        }
    }
}

/// Position DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionDto {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub side: String,
    pub pnl: f64,
    pub pnl_pct: f64,
    pub broker_id: String,
}

impl From<&models::Position> for PositionDto {
    fn from(p: &models::Position) -> Self {
        Self {
            symbol: p.symbol.0.clone(),
            quantity: p.quantity,
            avg_price: p.avg_price,
            side: enum_to_string(&p.side),
            pnl: p.pnl,
            pnl_pct: p.pnl_pct,
            broker_id: p.broker_id.clone(),
        }
    }
}

/// News item DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItemDto {
    pub id: String,
    pub headline: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub published: String,
    pub symbols: Vec<String>,
    pub sentiment: Option<f32>,
}

impl From<&models::NewsItem> for NewsItemDto {
    fn from(n: &models::NewsItem) -> Self {
        Self {
            id: n.id.to_string(),
            headline: n.headline.clone(),
            summary: n.summary.clone(),
            source: n.source.clone(),
            url: n.url.clone(),
            published: n.published.to_rfc3339(),
            symbols: n.symbols.iter().map(|s| s.0.clone()).collect(),
            sentiment: n.sentiment,
        }
    }
}

/// Alert DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertDto {
    pub rule_id: String,
    pub message: String,
    pub severity: String,
}

/// New order request DTO from frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrderRequestDto {
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub broker_id: String,
    pub tag: Option<String>,
}

/// Account balance DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalanceDto {
    pub total_value: f64,
    pub cash: f64,
    pub margin_used: f64,
    pub margin_available: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub currency: String,
}

impl From<&models::AccountBalance> for AccountBalanceDto {
    fn from(b: &models::AccountBalance) -> Self {
        Self {
            total_value: b.total_value,
            cash: b.cash,
            margin_used: b.margin_used,
            margin_available: b.margin_available,
            unrealized_pnl: b.unrealized_pnl,
            realized_pnl: b.realized_pnl,
            currency: b.currency.clone(),
        }
    }
}

/// Risk status DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskStatusDto {
    pub session_pnl: f64,
    pub is_halted: bool,
    pub max_daily_loss: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_quote_dto_from_quote() {
        let quote = models::Quote {
            symbol: models::Symbol("AAPL".into()),
            bid: 150.0,
            ask: 150.05,
            last: 150.02,
            open: 149.0,
            high: 151.0,
            low: 148.5,
            volume: 10000,
            change_pct: 0.5,
            vwap: 149.8,
            updated_at: Utc::now(),
        };
        let dto = QuoteDto::from(&quote);
        assert_eq!(dto.symbol, "AAPL");
        assert!((dto.last - 150.02).abs() < 0.01);
    }

    #[test]
    fn test_order_dto_from_order() {
        let order = models::Order {
            id: models::OrderId("test-1".into()),
            symbol: models::Symbol("AAPL".into()),
            side: models::OrderSide::Buy,
            order_type: models::OrderType::Limit,
            quantity: 10.0,
            price: Some(150.0),
            stop_price: None,
            status: models::OrderStatus::Open,
            filled_qty: 0.0,
            avg_price: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "paper".into(),
            source: "manual".into(),
        };
        let dto = OrderDto::from(&order);
        assert_eq!(dto.id, "test-1");
        assert_eq!(dto.side, "Buy");
    }

    #[test]
    fn test_position_dto_from_position() {
        let pos = models::Position {
            symbol: models::Symbol("AAPL".into()),
            quantity: 10.0,
            avg_price: 150.0,
            side: models::OrderSide::Buy,
            pnl: 50.0,
            pnl_pct: 3.33,
            broker_id: "paper".into(),
        };
        let dto = PositionDto::from(&pos);
        assert_eq!(dto.symbol, "AAPL");
        assert!((dto.pnl - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_new_order_request_dto_deserialization() {
        let json = r#"{
            "symbol": "AAPL",
            "side": "Buy",
            "order_type": "Limit",
            "quantity": 10.0,
            "price": 150.0,
            "stop_price": null,
            "broker_id": "paper",
            "tag": null
        }"#;
        let dto: NewOrderRequestDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.symbol, "AAPL");
        assert_eq!(dto.side, "Buy");
    }

    #[test]
    fn test_risk_status_dto_serialization() {
        let dto = RiskStatusDto {
            session_pnl: -500.0,
            is_halted: false,
            max_daily_loss: 50000.0,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("session_pnl"));
    }
}
