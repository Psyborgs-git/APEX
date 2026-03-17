use apex_core::{
    domain::models::*,
    ports::market_data::{AdapterHealth, MarketDataPort, TickStream},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Zerodha Kite market data adapter
///
/// Provides real-time and historical market data from Zerodha Kite Connect API
/// Documentation: https://kite.trade/docs/connect/v3/
pub struct ZerodhaKiteAdapter {
    api_key: String,
    access_token: Arc<RwLock<Option<String>>>,
    client: Client,
    health: Arc<RwLock<AdapterHealth>>,
}

/// Zerodha API quote response
#[derive(Debug, Deserialize)]
struct ZerodhaQuote {
    last_price: f64,
    #[serde(default)]
    ohlc: ZerodhaOHLC,
    #[serde(default)]
    depth: ZerodhaDepth,
    #[serde(default)]
    volume: u64,
    #[serde(default)]
    change_percent: f64,
}

#[derive(Debug, Deserialize, Default)]
struct ZerodhaOHLC {
    #[serde(default)]
    open: f64,
    #[serde(default)]
    high: f64,
    #[serde(default)]
    low: f64,
    #[serde(default)]
    close: f64,
}

#[derive(Debug, Deserialize, Default)]
struct ZerodhaDepth {
    #[serde(default)]
    buy: Vec<ZerodhaDepthItem>,
    #[serde(default)]
    sell: Vec<ZerodhaDepthItem>,
}

#[derive(Debug, Deserialize)]
struct ZerodhaDepthItem {
    price: f64,
    quantity: u64,
    orders: u64,
}

/// Zerodha API OHLC response
#[derive(Debug, Deserialize)]
struct ZerodhaHistoricalData {
    data: ZerodhaHistoricalDataWrapper,
}

#[derive(Debug, Deserialize)]
struct ZerodhaHistoricalDataWrapper {
    candles: Vec<Vec<serde_json::Value>>,
}

impl ZerodhaKiteAdapter {
    /// Create a new Zerodha Kite adapter
    ///
    /// # Arguments
    /// * `api_key` - Zerodha API key from app.kite.trade
    /// * `access_token` - Access token obtained after login flow
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

    /// Set access token (after login flow)
    pub fn set_access_token(&self, token: String) {
        let mut access_token = self.access_token.write().unwrap();
        *access_token = Some(token);
        info!("Zerodha access token updated");
    }

    /// Check if authenticated
    fn is_authenticated(&self) -> bool {
        self.access_token.read().unwrap().is_some()
    }

    /// Get authorization header value
    fn get_auth_header(&self) -> Result<String> {
        let token = self.access_token.read().unwrap();
        match token.as_ref() {
            Some(t) => Ok(format!("token {}:{}", self.api_key, t)),
            None => Err(anyhow::anyhow!("No access token available")),
        }
    }

    /// Map APEX symbol to Zerodha instrument token
    /// Note: In production, maintain a symbol-to-instrument mapping table
    fn symbol_to_instrument_token(&self, symbol: &Symbol) -> String {
        // Simplified mapping - in production, use Zerodha instruments API
        // Format: NSE:SYMBOL or BSE:SYMBOL
        format!("NSE:{}", symbol.0)
    }

    /// Map Zerodha instrument to APEX symbol
    fn instrument_to_symbol(&self, instrument: &str) -> Symbol {
        // Extract symbol from "NSE:SYMBOL" or "BSE:SYMBOL"
        let parts: Vec<&str> = instrument.split(':').collect();
        Symbol(parts.get(1).unwrap_or(&"UNKNOWN").to_string())
    }

    /// Map APEX timeframe to Zerodha interval string
    fn timeframe_to_interval(&self, tf: &Timeframe) -> &'static str {
        match tf {
            Timeframe::M1 => "minute",
            Timeframe::M3 => "3minute",
            Timeframe::M5 => "5minute",
            Timeframe::M15 => "15minute",
            Timeframe::M30 => "30minute",
            Timeframe::H1 => "60minute",
            Timeframe::D1 => "day",
            _ => "minute", // Default fallback
        }
    }

    /// Update health status
    fn set_health(&self, health: AdapterHealth) {
        let mut h = self.health.write().unwrap();
        *h = health;
    }
}

#[async_trait]
impl MarketDataPort for ZerodhaKiteAdapter {
    async fn subscribe(&self, symbols: &[Symbol]) -> Result<TickStream> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        // Zerodha WebSocket subscription would go here
        // For now, implement polling-based subscription
        let (tx, rx) = mpsc::channel(1000);

        let symbols_clone: Vec<Symbol> = symbols.to_vec();
        let adapter = ZerodhaKiteAdapter::new(
            self.api_key.clone(),
            self.access_token.read().unwrap().clone(),
        )?;

        tokio::spawn(async move {
            loop {
                for symbol in &symbols_clone {
                    match adapter.get_snapshot(symbol).await {
                        Ok(quote) => {
                            let tick = Tick {
                                time: Utc::now(),
                                symbol: symbol.clone(),
                                bid: quote.bid,
                                ask: quote.ask,
                                last: quote.last,
                                volume: quote.volume,
                                source: "zerodha_kite".to_string(),
                            };

                            if tx.send(tick).await.is_err() {
                                warn!("Tick channel closed, stopping subscription");
                                return;
                            }
                        }
                        Err(e) => {
                            error!("Failed to fetch quote for {}: {}", symbol.0, e);
                        }
                    }
                }

                // Poll every 1 second
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        info!("Subscribed to {} symbols via Zerodha Kite", symbols.len());
        Ok(rx)
    }

    async fn unsubscribe(&self, symbols: &[Symbol]) -> Result<()> {
        // In WebSocket implementation, send unsubscribe message
        debug!("Unsubscribed from {} symbols", symbols.len());
        Ok(())
    }

    async fn get_snapshot(&self, symbol: &Symbol) -> Result<Quote> {
        if !self.is_authenticated() {
            self.set_health(AdapterHealth::Unhealthy("Not authenticated".to_string()));
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let instrument = self.symbol_to_instrument_token(symbol);
        let url = format!(
            "https://api.kite.trade/quote/ohlc?i={}",
            instrument
        );

        let auth_header = self.get_auth_header()?;

        let response = self.client
            .get(&url)
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to fetch quote from Zerodha")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            self.set_health(AdapterHealth::Degraded(format!("HTTP {}", status)));
            return Err(anyhow::anyhow!("Zerodha API error {}: {}", status, error_text));
        }

        let data: serde_json::Value = response.json().await
            .context("Failed to parse Zerodha response")?;

        // Extract quote data from nested response
        let quote_data = data
            .get("data")
            .and_then(|d| d.get(&instrument))
            .ok_or_else(|| anyhow::anyhow!("Quote data not found in response"))?;

        let zerodha_quote: ZerodhaQuote = serde_json::from_value(quote_data.clone())
            .context("Failed to deserialize quote")?;

        // Calculate bid/ask from depth or use last_price
        let (bid, ask) = if !zerodha_quote.depth.buy.is_empty() && !zerodha_quote.depth.sell.is_empty() {
            (
                zerodha_quote.depth.buy[0].price,
                zerodha_quote.depth.sell[0].price,
            )
        } else {
            // Approximate bid/ask if depth not available
            let spread = zerodha_quote.last_price * 0.0005; // 0.05% spread
            (
                zerodha_quote.last_price - spread,
                zerodha_quote.last_price + spread,
            )
        };

        self.set_health(AdapterHealth::Healthy);

        Ok(Quote {
            symbol: symbol.clone(),
            bid,
            ask,
            last: zerodha_quote.last_price,
            open: zerodha_quote.ohlc.open,
            high: zerodha_quote.ohlc.high,
            low: zerodha_quote.ohlc.low,
            volume: zerodha_quote.volume,
            change_pct: zerodha_quote.change_percent,
            vwap: zerodha_quote.last_price, // VWAP not provided by Zerodha quote API
            updated_at: Utc::now(),
        })
    }

    async fn get_historical_ohlcv(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<OHLCV>> {
        if !self.is_authenticated() {
            return Err(anyhow::anyhow!("Not authenticated with Zerodha"));
        }

        let instrument = self.symbol_to_instrument_token(symbol);
        let interval = self.timeframe_to_interval(&timeframe);

        let url = format!(
            "https://api.kite.trade/instruments/historical/{}/{}?from={}&to={}",
            instrument,
            interval,
            from.format("%Y-%m-%d"),
            to.format("%Y-%m-%d")
        );

        let auth_header = self.get_auth_header()?;

        let response = self.client
            .get(&url)
            .header("Authorization", auth_header)
            .header("X-Kite-Version", "3")
            .send()
            .await
            .context("Failed to fetch historical data from Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Zerodha API error: {}", response.status()));
        }

        let hist_data: ZerodhaHistoricalData = response.json().await
            .context("Failed to parse historical data")?;

        let mut bars = Vec::new();

        for candle in hist_data.data.candles {
            if candle.len() < 6 {
                continue;
            }

            // Candle format: [timestamp, open, high, low, close, volume]
            let time_str = candle[0].as_str().unwrap_or("");
            let time = DateTime::parse_from_rfc3339(time_str)
                .ok()
                .and_then(|dt| Some(dt.with_timezone(&Utc)))
                .unwrap_or_else(Utc::now);

            let bar = OHLCV {
                time,
                symbol: symbol.clone(),
                open: candle[1].as_f64().unwrap_or(0.0),
                high: candle[2].as_f64().unwrap_or(0.0),
                low: candle[3].as_f64().unwrap_or(0.0),
                close: candle[4].as_f64().unwrap_or(0.0),
                volume: candle[5].as_u64().unwrap_or(0),
            };

            bars.push(bar);
        }

        debug!("Fetched {} historical bars for {} from Zerodha", bars.len(), symbol.0);
        Ok(bars)
    }

    fn adapter_id(&self) -> &'static str {
        "zerodha_kite"
    }

    fn health(&self) -> AdapterHealth {
        self.health.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_to_instrument() {
        let adapter = ZerodhaKiteAdapter::new("test_key".to_string(), None).unwrap();
        let symbol = Symbol("RELIANCE".to_string());
        let instrument = adapter.symbol_to_instrument_token(&symbol);
        assert_eq!(instrument, "NSE:RELIANCE");
    }

    #[test]
    fn test_timeframe_mapping() {
        let adapter = ZerodhaKiteAdapter::new("test_key".to_string(), None).unwrap();
        assert_eq!(adapter.timeframe_to_interval(&Timeframe::M1), "minute");
        assert_eq!(adapter.timeframe_to_interval(&Timeframe::M5), "5minute");
        assert_eq!(adapter.timeframe_to_interval(&Timeframe::D1), "day");
    }
}
