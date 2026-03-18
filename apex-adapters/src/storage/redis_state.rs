use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use serde_json;
use tracing::{debug, error, info};

use apex_core::domain::models::*;

/// Redis adapter for hot state caching
///
/// Provides sub-millisecond access to:
/// - Current quotes (QUOTE:{SYMBOL})
/// - Open positions (POSITION:{ACCOUNT}:{SYMBOL})
/// - Order status (ORDER:{ORDER_ID})
/// - Strategy state (STRATEGY:{ID}:state)
pub struct RedisStateAdapter {
    conn: ConnectionManager,
}

impl RedisStateAdapter {
    /// Create a new Redis state adapter
    ///
    /// # Arguments
    /// * `redis_url` - Redis connection string (e.g. "redis://127.0.0.1:6379")
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)
            .context("Failed to create Redis client")?;

        let conn = ConnectionManager::new(client).await
            .context("Failed to create Redis connection manager")?;

        info!("Redis state adapter initialized successfully");
        Ok(Self { conn })
    }

    /// Store a quote in Redis
    ///
    /// Stores quote as a hash with fields: bid, ask, last, volume, updated_at
    /// Key: QUOTE:{SYMBOL}
    /// TTL: 1 hour (auto-expire stale quotes)
    pub async fn set_quote(&mut self, quote: &Quote) -> Result<()> {
        let key = format!("QUOTE:{}", quote.symbol.0);

        let _: () = self.conn.hset_multiple(
            &key,
            &[
                ("bid", quote.bid.to_string()),
                ("ask", quote.ask.to_string()),
                ("last", quote.last.to_string()),
                ("open", quote.open.to_string()),
                ("high", quote.high.to_string()),
                ("low", quote.low.to_string()),
                ("volume", quote.volume.to_string()),
                ("change_pct", quote.change_pct.to_string()),
                ("vwap", quote.vwap.to_string()),
                ("updated_at", quote.updated_at.to_rfc3339()),
            ],
        ).await.context("Failed to set quote in Redis")?;

        // Set expiry to 1 hour
        let _: () = self.conn.expire(&key, 3600).await?;

        debug!("Cached quote for {}", quote.symbol.0);
        Ok(())
    }

    /// Retrieve a quote from Redis
    pub async fn get_quote(&mut self, symbol: &Symbol) -> Result<Option<Quote>> {
        let key = format!("QUOTE:{}", symbol.0);

        let exists: bool = self.conn.exists(&key).await?;
        if !exists {
            return Ok(None);
        }

        let values: Vec<String> = self.conn.hget(
            &key,
            &["bid", "ask", "last", "open", "high", "low", "volume", "change_pct", "vwap", "updated_at"],
        ).await.context("Failed to get quote from Redis")?;

        if values.len() != 10 {
            return Ok(None);
        }

        Ok(Some(Quote {
            symbol: symbol.clone(),
            bid: values[0].parse().unwrap_or(0.0),
            ask: values[1].parse().unwrap_or(0.0),
            last: values[2].parse().unwrap_or(0.0),
            open: values[3].parse().unwrap_or(0.0),
            high: values[4].parse().unwrap_or(0.0),
            low: values[5].parse().unwrap_or(0.0),
            volume: values[6].parse().unwrap_or(0),
            change_pct: values[7].parse().unwrap_or(0.0),
            vwap: values[8].parse().unwrap_or(0.0),
            updated_at: chrono::DateTime::parse_from_rfc3339(&values[9])
                .ok()
                .and_then(|dt| Some(dt.with_timezone(&chrono::Utc)))
                .unwrap_or_else(chrono::Utc::now),
        }))
    }

    /// Store a position in Redis
    ///
    /// Key: POSITION:{BROKER_ID}:{SYMBOL}
    /// Stores: quantity, avg_price, side, pnl, pnl_pct
    pub async fn set_position(&mut self, pos: &Position) -> Result<()> {
        let key = format!("POSITION:{}:{}", pos.broker_id, pos.symbol.0);

        let side_str = match pos.side {
            OrderSide::Buy => "Buy",
            OrderSide::Sell => "Sell",
        };

        let _: () = self.conn.hset_multiple(
            &key,
            &[
                ("quantity", pos.quantity.to_string()),
                ("avg_price", pos.avg_price.to_string()),
                ("side", side_str.to_string()),
                ("pnl", pos.pnl.to_string()),
                ("pnl_pct", pos.pnl_pct.to_string()),
            ],
        ).await.context("Failed to set position in Redis")?;

        debug!("Cached position for {} @ {}", pos.symbol.0, pos.broker_id);
        Ok(())
    }

    /// Retrieve a position from Redis
    pub async fn get_position(&mut self, broker_id: &str, symbol: &Symbol) -> Result<Option<Position>> {
        let key = format!("POSITION:{}:{}", broker_id, symbol.0);

        let exists: bool = self.conn.exists(&key).await?;
        if !exists {
            return Ok(None);
        }

        let values: Vec<String> = self.conn.hget(
            &key,
            &["quantity", "avg_price", "side", "pnl", "pnl_pct"],
        ).await.context("Failed to get position from Redis")?;

        if values.len() != 5 {
            return Ok(None);
        }

        let side = match values[2].as_str() {
            "Buy" => OrderSide::Buy,
            "Sell" => OrderSide::Sell,
            _ => OrderSide::Buy,
        };

        Ok(Some(Position {
            symbol: symbol.clone(),
            quantity: values[0].parse().unwrap_or(0.0),
            avg_price: values[1].parse().unwrap_or(0.0),
            side,
            pnl: values[3].parse().unwrap_or(0.0),
            pnl_pct: values[4].parse().unwrap_or(0.0),
            broker_id: broker_id.to_string(),
        }))
    }

    /// Get all positions for a broker
    pub async fn get_all_positions(&mut self, broker_id: &str) -> Result<Vec<Position>> {
        let pattern = format!("POSITION:{}:*", broker_id);
        let keys: Vec<String> = self.conn.keys(&pattern).await?;

        let mut positions = Vec::new();

        for key in keys {
            // Extract symbol from key: POSITION:{broker_id}:{symbol}
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() != 3 {
                continue;
            }

            let symbol = Symbol(parts[2].to_string());

            if let Some(pos) = self.get_position(broker_id, &symbol).await? {
                positions.push(pos);
            }
        }

        Ok(positions)
    }

    /// Store order state in Redis
    ///
    /// Key: ORDER:{ORDER_ID}
    /// Stores full order JSON with 5-minute TTL
    pub async fn set_order(&mut self, order: &Order) -> Result<()> {
        let key = format!("ORDER:{}", order.id.0);
        let order_json = serde_json::to_string(order)
            .context("Failed to serialize order")?;

        let _: () = self.conn.set_ex(&key, order_json, 300).await
            .context("Failed to set order in Redis")?;

        debug!("Cached order {}", order.id.0);
        Ok(())
    }

    /// Retrieve order state from Redis
    pub async fn get_order(&mut self, order_id: &OrderId) -> Result<Option<Order>> {
        let key = format!("ORDER:{}", order_id.0);

        let order_json: Option<String> = self.conn.get(&key).await?;

        match order_json {
            Some(json) => {
                let order: Order = serde_json::from_str(&json)
                    .context("Failed to deserialize order")?;
                Ok(Some(order))
            }
            None => Ok(None),
        }
    }

    /// Delete order from cache (after terminal state)
    pub async fn delete_order(&mut self, order_id: &OrderId) -> Result<()> {
        let key = format!("ORDER:{}", order_id.0);
        let _: () = self.conn.del(&key).await?;
        debug!("Deleted order {} from cache", order_id.0);
        Ok(())
    }

    /// Store strategy state
    ///
    /// Key: STRATEGY:{ID}:state
    /// Stores: status, last_signal, pnl, metrics
    pub async fn set_strategy_state(&mut self, strategy_id: &str, state: &serde_json::Value) -> Result<()> {
        let key = format!("STRATEGY:{}:state", strategy_id);
        let state_json = serde_json::to_string(state)
            .context("Failed to serialize strategy state")?;

        let _: () = self.conn.set(&key, state_json).await
            .context("Failed to set strategy state")?;

        debug!("Cached strategy state for {}", strategy_id);
        Ok(())
    }

    /// Retrieve strategy state
    pub async fn get_strategy_state(&mut self, strategy_id: &str) -> Result<Option<serde_json::Value>> {
        let key = format!("STRATEGY:{}:state", strategy_id);

        let state_json: Option<String> = self.conn.get(&key).await?;

        match state_json {
            Some(json) => {
                let state: serde_json::Value = serde_json::from_str(&json)
                    .context("Failed to deserialize strategy state")?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    /// Increment a counter (for metrics, trade counts, etc.)
    pub async fn incr_counter(&mut self, key: &str) -> Result<i64> {
        let count: i64 = self.conn.incr(key, 1).await
            .context("Failed to increment counter")?;
        Ok(count)
    }

    /// Get counter value
    pub async fn get_counter(&mut self, key: &str) -> Result<i64> {
        let count: i64 = self.conn.get(key).await.unwrap_or(0);
        Ok(count)
    }

    /// Flush all cached data (use with caution!)
    pub async fn flush_all(&mut self) -> Result<()> {
        redis::cmd("FLUSHDB")
            .query_async::<()>(&mut self.conn)
            .await
            .context("Failed to flush Redis database")?;

        info!("Flushed all Redis data");
        Ok(())
    }

    /// Ping Redis to check connection health
    pub async fn ping(&mut self) -> Result<()> {
        let pong: String = redis::cmd("PING")
            .query_async(&mut self.conn)
            .await
            .context("Failed to ping Redis")?;

        if pong == "PONG" {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Unexpected ping response: {}", pong))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_key_format() {
        let symbol = Symbol("AAPL".to_string());
        let key = format!("QUOTE:{}", symbol.0);
        assert_eq!(key, "QUOTE:AAPL");
    }

    #[test]
    fn test_position_key_format() {
        let key = format!("POSITION:{}:{}", "paper", "RELIANCE");
        assert_eq!(key, "POSITION:paper:RELIANCE");
    }
}
