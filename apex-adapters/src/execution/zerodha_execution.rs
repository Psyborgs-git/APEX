use apex_core::{
    domain::models::*,
    ports::{
        execution::ExecutionPort,
        market_data::AdapterHealth,
    },
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Zerodha Kite execution adapter
///
/// Provides order placement, modification, cancellation, and position management
/// via Zerodha Kite Connect API
pub struct ZerodhaExecutionAdapter {
    api_key: String,
    access_token: Arc<RwLock<Option<String>>>,
    client: Client,
    health: Arc<RwLock<AdapterHealth>>,
}

/// Zerodha order placement request
#[derive(Debug, Serialize)]
struct ZerodhaOrderRequest {
    tradingsymbol: String,
    exchange: String,
    transaction_type: String,
    order_type: String,
    quantity: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<f64>,
    product: String,
    validity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
}

/// Zerodha order placement response
#[derive(Debug, Deserialize)]
struct ZerodhaOrderResponse {
    data: ZerodhaOrderData,
}

#[derive(Debug, Deserialize)]
struct ZerodhaOrderData {
    order_id: String,
}

/// Zerodha order status response
#[derive(Debug, Deserialize)]
struct ZerodhaOrder {
    order_id: String,
    tradingsymbol: String,
    transaction_type: String,
    order_type: String,
    quantity: u64,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    trigger_price: f64,
    status: String,
    #[serde(default)]
    filled_quantity: u64,
    #[serde(default)]
    average_price: f64,
    #[serde(default)]
    order_timestamp: String,
}

/// Zerodha position response
#[derive(Debug, Deserialize)]
struct ZerodhaPositionResponse {
    data: ZerodhaPositionData,
}

#[derive(Debug, Deserialize)]
struct ZerodhaPositionData {
    net: Vec<ZerodhaPosition>,
}

#[derive(Debug, Deserialize)]
struct ZerodhaPosition {
    tradingsymbol: String,
    quantity: i64,
    average_price: f64,
    #[serde(default)]
    pnl: f64,
}

impl ZerodhaExecutionAdapter {
    /// Create a new Zerodha execution adapter
    ///
    /// # Arguments
    /// * `api_key` - Zerodha API key
    /// * `access_token` - Access token after login
    pub fn new(api_key: String, access_token: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            api_key,
            access_token: Arc::new(RwLock::new(access_token)),
            client,
            health: Arc::new(RwLock::new(AdapterHealth::Healthy)),
        })
    }

    /// Set access token
    pub fn set_access_token(&self, token: String) {
        let mut access_token = self.access_token.write().unwrap();
        *access_token = Some(token);
        info!("Zerodha execution access token updated");
    }

    /// Get authorization header
    fn get_auth_header(&self) -> Result<String> {
        let token = self.access_token.read().unwrap();
        match token.as_ref() {
            Some(t) => Ok(format!("token {}:{}", self.api_key, t)),
            None => Err(anyhow::anyhow!("No access token available")),
        }
    }

    /// Check if authenticated
    fn is_authenticated(&self) -> bool {
        self.access_token.read().unwrap().is_some()
    }

    /// Map APEX symbol to Zerodha trading symbol and exchange
    fn symbol_to_zerodha(&self, symbol: &Symbol) -> (String, String) {
        // Simplified mapping - in production, use instrument master
        // Assume NSE for equities
        (symbol.0.clone(), "NSE".to_string())
    }

    /// Map APEX OrderSide to Zerodha transaction type
    fn side_to_transaction_type(&self, side: &OrderSide) -> &'static str {
        match side {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }

    /// Map APEX OrderType to Zerodha order type
    fn order_type_to_zerodha(&self, order_type: &OrderType) -> &'static str {
        match order_type {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::Stop => "SL",
            OrderType::StopLimit => "SL-M",
            OrderType::TrailingStop => "SL", // Zerodha doesn't have native trailing stop
        }
    }

    /// Map Zerodha status to APEX OrderStatus
    fn zerodha_status_to_order_status(&self, status: &str) -> OrderStatus {
        match status {
            "PENDING" | "OPEN" | "TRIGGER PENDING" => OrderStatus::Open,
            "COMPLETE" => OrderStatus::Filled,
            "CANCELLED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            _ => OrderStatus::Pending,
        }
    }

    /// Update health status
    fn set_health(&self, health: AdapterHealth) {
        let mut h = self.health.write().unwrap();
        *h = health;
    }
}

#[async_trait]
impl ExecutionPort for ZerodhaExecutionAdapter {
    async fn place_order(&self, order: &NewOrderRequest) -> Result<OrderId> {
        if !self.is_authenticated() {
            self.set_health(AdapterHealth::Unhealthy("Not authenticated".to_string()));
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let (tradingsymbol, exchange) = self.symbol_to_zerodha(&order.symbol);

        let zerodha_order = ZerodhaOrderRequest {
            tradingsymbol,
            exchange,
            transaction_type: self.side_to_transaction_type(&order.side).to_string(),
            order_type: self.order_type_to_zerodha(&order.order_type).to_string(),
            quantity: order.quantity as u64,
            price: order.price,
            trigger_price: order.stop_price,
            product: "CNC".to_string(), // Cash and Carry (can be MIS for intraday)
            validity: "DAY".to_string(),
            tag: order.tag.clone(),
        };

        let auth_header = self.get_auth_header()?;

        let response = self.client
            .post("https://api.kite.trade/orders/regular")
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .json(&zerodha_order)
            .send()
            .await
            .context("Failed to place order with Zerodha")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            self.set_health(AdapterHealth::Degraded(format!("Order placement failed: {}", status)));
            return Err(anyhow::anyhow!("Zerodha order placement error {}: {}", status, error_text));
        }

        let order_response: ZerodhaOrderResponse = response.json().await
            .context("Failed to parse order response")?;

        self.set_health(AdapterHealth::Healthy);

        info!("Placed order {} with Zerodha", order_response.data.order_id);
        Ok(OrderId(order_response.data.order_id))
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<()> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let url = format!("https://api.kite.trade/orders/regular/{}", order_id.0);
        let auth_header = self.get_auth_header()?;

        let response = self.client
            .delete(&url)
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to cancel order with Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to cancel order: {}", response.status()));
        }

        info!("Cancelled order {} with Zerodha", order_id.0);
        Ok(())
    }

    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let url = format!("https://api.kite.trade/orders/regular/{}", order_id.0);
        let auth_header = self.get_auth_header()?;

        let mut form_data = Vec::new();

        if let Some(qty) = params.quantity {
            form_data.push(("quantity", qty.to_string()));
        }

        if let Some(price) = params.price {
            form_data.push(("price", price.to_string()));
        }

        if let Some(trigger_price) = params.stop_price {
            form_data.push(("trigger_price", trigger_price.to_string()));
        }

        let response = self.client
            .put(&url)
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .form(&form_data)
            .send()
            .await
            .context("Failed to modify order with Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to modify order: {}", response.status()));
        }

        info!("Modified order {} with Zerodha", order_id.0);
        Ok(())
    }

    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let url = format!("https://api.kite.trade/orders");
        let auth_header = self.get_auth_header()?;

        let response = self.client
            .get(&url)
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to fetch orders from Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch order status: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;
        let orders = data
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid orders response"))?;

        // Find the order with matching order_id
        for order_value in orders {
            let zerodha_order: ZerodhaOrder = serde_json::from_value(order_value.clone())?;

            if zerodha_order.order_id == order_id.0 {
                let side = match zerodha_order.transaction_type.as_str() {
                    "BUY" => OrderSide::Buy,
                    "SELL" => OrderSide::Sell,
                    _ => OrderSide::Buy,
                };

                let order_type = match zerodha_order.order_type.as_str() {
                    "MARKET" => OrderType::Market,
                    "LIMIT" => OrderType::Limit,
                    "SL" => OrderType::Stop,
                    "SL-M" => OrderType::StopLimit,
                    _ => OrderType::Market,
                };

                let status = self.zerodha_status_to_order_status(&zerodha_order.status);

                let created_at = chrono::DateTime::parse_from_rfc3339(&zerodha_order.order_timestamp)
                    .ok()
                    .and_then(|dt| Some(dt.with_timezone(&Utc)))
                    .unwrap_or_else(Utc::now);

                return Ok(Order {
                    id: order_id.clone(),
                    symbol: Symbol(zerodha_order.tradingsymbol),
                    side,
                    order_type,
                    quantity: zerodha_order.quantity as f64,
                    price: if zerodha_order.price > 0.0 {
                        Some(zerodha_order.price)
                    } else {
                        None
                    },
                    stop_price: if zerodha_order.trigger_price > 0.0 {
                        Some(zerodha_order.trigger_price)
                    } else {
                        None
                    },
                    status,
                    filled_qty: zerodha_order.filled_quantity as f64,
                    avg_price: zerodha_order.average_price,
                    created_at,
                    updated_at: Utc::now(),
                    broker_id: "zerodha".to_string(),
                    source: "api".to_string(),
                });
            }
        }

        Err(anyhow::anyhow!("Order {} not found", order_id.0))
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let auth_header = self.get_auth_header()?;

        let response = self.client
            .get("https://api.kite.trade/portfolio/positions")
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to fetch positions from Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch positions: {}", response.status()));
        }

        let pos_response: ZerodhaPositionResponse = response.json().await
            .context("Failed to parse positions response")?;

        let mut positions = Vec::new();

        for zerodha_pos in pos_response.data.net {
            if zerodha_pos.quantity == 0 {
                continue; // Skip closed positions
            }

            let side = if zerodha_pos.quantity > 0 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };

            let quantity = zerodha_pos.quantity.abs() as f64;
            let pnl_pct = if zerodha_pos.average_price > 0.0 {
                (zerodha_pos.pnl / (zerodha_pos.average_price * quantity)) * 100.0
            } else {
                0.0
            };

            positions.push(Position {
                symbol: Symbol(zerodha_pos.tradingsymbol),
                quantity,
                avg_price: zerodha_pos.average_price,
                side,
                pnl: zerodha_pos.pnl,
                pnl_pct,
                broker_id: "zerodha".to_string(),
            });
        }

        debug!("Fetched {} positions from Zerodha", positions.len());
        Ok(positions)
    }

    async fn get_account_balance(&self) -> Result<AccountBalance> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let auth_header = self.get_auth_header()?;

        let response = self.client
            .get("https://api.kite.trade/user/margins")
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to fetch account balance from Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch balance: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;

        // Extract equity margin data
        let equity = data
            .get("data")
            .and_then(|d| d.get("equity"))
            .ok_or_else(|| anyhow::anyhow!("No equity margin data"))?;

        let available = equity.get("available").and_then(|v| v.get("cash")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let used = equity.get("utilised").and_then(|v| v.get("debits")).and_then(|v| v.as_f64()).unwrap_or(0.0);

        Ok(AccountBalance {
            total_value: available + used,
            cash: available,
            margin_used: used,
            margin_available: available,
            unrealized_pnl: 0.0, // Would need to calculate from positions
            realized_pnl: 0.0,   // Would need to track from trades
            currency: "INR".to_string(),
        })
    }

    fn broker_id(&self) -> &'static str {
        "zerodha"
    }

    fn supported_order_types(&self) -> &[OrderType] {
        &[
            OrderType::Market,
            OrderType::Limit,
            OrderType::Stop,
            OrderType::StopLimit,
        ]
    }

    fn health(&self) -> AdapterHealth {
        self.health.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_to_zerodha() {
        let adapter = ZerodhaExecutionAdapter::new("test_key".to_string(), None).unwrap();
        let symbol = Symbol("RELIANCE".to_string());
        let (trading_symbol, exchange) = adapter.symbol_to_zerodha(&symbol);
        assert_eq!(trading_symbol, "RELIANCE");
        assert_eq!(exchange, "NSE");
    }

    #[test]
    fn test_side_mapping() {
        let adapter = ZerodhaExecutionAdapter::new("test_key".to_string(), None).unwrap();
        assert_eq!(adapter.side_to_transaction_type(&OrderSide::Buy), "BUY");
        assert_eq!(adapter.side_to_transaction_type(&OrderSide::Sell), "SELL");
    }

    #[test]
    fn test_order_type_mapping() {
        let adapter = ZerodhaExecutionAdapter::new("test_key".to_string(), None).unwrap();
        assert_eq!(adapter.order_type_to_zerodha(&OrderType::Market), "MARKET");
        assert_eq!(adapter.order_type_to_zerodha(&OrderType::Limit), "LIMIT");
        assert_eq!(adapter.order_type_to_zerodha(&OrderType::Stop), "SL");
    }
}
