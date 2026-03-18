use apex_core::{
    domain::models::*,
    ports::storage::StoragePort,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::{Config, Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::{debug, error, info};

/// TimescaleDB adapter for time-series data storage
///
/// This adapter provides high-performance tick/OHLCV storage with automatic compression
/// and continuous aggregates for common timeframes.
pub struct TimescaleAdapter {
    pool: Pool,
}

impl TimescaleAdapter {
    /// Create a new TimescaleDB adapter
    ///
    /// # Arguments
    /// * `connection_url` - PostgreSQL connection string (e.g. "postgresql://user:pass@localhost:5432/apex_market")
    pub async fn new(connection_url: &str) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.url = Some(connection_url.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create TimescaleDB connection pool")?;

        let adapter = Self { pool };

        // Initialize database schema if needed
        adapter.initialize_schema().await?;

        info!("TimescaleDB adapter initialized successfully");
        Ok(adapter)
    }

    /// Initialize the database schema with hypertables and continuous aggregates
    async fn initialize_schema(&self) -> Result<()> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        // Create ticks table as hypertable
        client.batch_execute(r#"
            CREATE TABLE IF NOT EXISTS ticks (
                time        TIMESTAMPTZ NOT NULL,
                symbol      TEXT NOT NULL,
                bid         DOUBLE PRECISION,
                ask         DOUBLE PRECISION,
                last        DOUBLE PRECISION,
                volume      BIGINT,
                source      TEXT
            );

            SELECT create_hypertable('ticks', 'time', if_not_exists => TRUE);

            -- Add compression policy (compress after 7 days)
            SELECT add_compression_policy('ticks', INTERVAL '7 days', if_not_exists => TRUE);

            -- Create index for symbol lookups
            CREATE INDEX IF NOT EXISTS idx_ticks_symbol_time ON ticks (symbol, time DESC);
        "#).await.context("Failed to create ticks hypertable")?;

        // Create OHLCV table as hypertable
        client.batch_execute(r#"
            CREATE TABLE IF NOT EXISTS ohlcv (
                time       TIMESTAMPTZ NOT NULL,
                symbol     TEXT NOT NULL,
                timeframe  TEXT NOT NULL,
                open       DOUBLE PRECISION NOT NULL,
                high       DOUBLE PRECISION NOT NULL,
                low        DOUBLE PRECISION NOT NULL,
                close      DOUBLE PRECISION NOT NULL,
                volume     BIGINT NOT NULL
            );

            SELECT create_hypertable('ohlcv', 'time', if_not_exists => TRUE);

            -- Add compression policy
            SELECT add_compression_policy('ohlcv', INTERVAL '30 days', if_not_exists => TRUE);

            -- Create unique index to prevent duplicates
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ohlcv_symbol_time_tf
                ON ohlcv (symbol, timeframe, time DESC);
        "#).await.context("Failed to create ohlcv hypertable")?;

        // Create continuous aggregates for common timeframes
        client.batch_execute(r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS ohlcv_1m
            WITH (timescaledb.continuous) AS
            SELECT
                time_bucket('1 minute', time) AS time,
                symbol,
                first(last, time)  AS open,
                max(last)          AS high,
                min(last)          AS low,
                last(last, time)   AS close,
                sum(volume)        AS volume
            FROM ticks
            GROUP BY time_bucket('1 minute', time), symbol
            WITH NO DATA;

            CREATE MATERIALIZED VIEW IF NOT EXISTS ohlcv_5m
            WITH (timescaledb.continuous) AS
            SELECT
                time_bucket('5 minutes', time) AS time,
                symbol,
                first(last, time)  AS open,
                max(last)          AS high,
                min(last)          AS low,
                last(last, time)   AS close,
                sum(volume)        AS volume
            FROM ticks
            GROUP BY time_bucket('5 minutes', time), symbol
            WITH NO DATA;

            CREATE MATERIALIZED VIEW IF NOT EXISTS ohlcv_1h
            WITH (timescaledb.continuous) AS
            SELECT
                time_bucket('1 hour', time) AS time,
                symbol,
                first(last, time)  AS open,
                max(last)          AS high,
                min(last)          AS low,
                last(last, time)   AS close,
                sum(volume)        AS volume
            FROM ticks
            GROUP BY time_bucket('1 hour', time), symbol
            WITH NO DATA;
        "#).await.context("Failed to create continuous aggregates")?;

        // Create orders table
        client.batch_execute(r#"
            CREATE TABLE IF NOT EXISTS orders (
                id          TEXT PRIMARY KEY,
                symbol      TEXT NOT NULL,
                side        TEXT NOT NULL,
                order_type  TEXT NOT NULL,
                quantity    DOUBLE PRECISION NOT NULL,
                price       DOUBLE PRECISION,
                stop_price  DOUBLE PRECISION,
                status      TEXT NOT NULL,
                filled_qty  DOUBLE PRECISION NOT NULL,
                avg_price   DOUBLE PRECISION NOT NULL,
                created_at  TIMESTAMPTZ NOT NULL,
                updated_at  TIMESTAMPTZ NOT NULL,
                broker_id   TEXT NOT NULL,
                source      TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_orders_symbol ON orders (symbol);
            CREATE INDEX IF NOT EXISTS idx_orders_status ON orders (status);
            CREATE INDEX IF NOT EXISTS idx_orders_created ON orders (created_at DESC);
        "#).await.context("Failed to create orders table")?;

        // Create positions table
        client.batch_execute(r#"
            CREATE TABLE IF NOT EXISTS positions (
                symbol     TEXT NOT NULL,
                broker_id  TEXT NOT NULL,
                quantity   DOUBLE PRECISION NOT NULL,
                avg_price  DOUBLE PRECISION NOT NULL,
                side       TEXT NOT NULL,
                pnl        DOUBLE PRECISION NOT NULL,
                pnl_pct    DOUBLE PRECISION NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (symbol, broker_id)
            );
        "#).await.context("Failed to create positions table")?;

        debug!("Database schema initialized successfully");
        Ok(())
    }

    /// Map Timeframe enum to PostgreSQL interval string
    fn timeframe_to_interval(tf: &Timeframe) -> &'static str {
        match tf {
            Timeframe::S1 => "1 second",
            Timeframe::S5 => "5 seconds",
            Timeframe::S15 => "15 seconds",
            Timeframe::M1 => "1 minute",
            Timeframe::M3 => "3 minutes",
            Timeframe::M5 => "5 minutes",
            Timeframe::M15 => "15 minutes",
            Timeframe::M30 => "30 minutes",
            Timeframe::H1 => "1 hour",
            Timeframe::H4 => "4 hours",
            Timeframe::D1 => "1 day",
            Timeframe::W1 => "1 week",
        }
    }

    /// Map Timeframe enum to string for storage
    fn timeframe_to_string(tf: &Timeframe) -> &'static str {
        match tf {
            Timeframe::S1 => "S1",
            Timeframe::S5 => "S5",
            Timeframe::S15 => "S15",
            Timeframe::M1 => "M1",
            Timeframe::M3 => "M3",
            Timeframe::M5 => "M5",
            Timeframe::M15 => "M15",
            Timeframe::M30 => "M30",
            Timeframe::H1 => "H1",
            Timeframe::H4 => "H4",
            Timeframe::D1 => "D1",
            Timeframe::W1 => "W1",
        }
    }
}

#[async_trait]
impl StoragePort for TimescaleAdapter {
    async fn write_ticks(&self, ticks: &[Tick]) -> Result<()> {
        if ticks.is_empty() {
            return Ok(());
        }

        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        // Batch insert for performance
        let stmt = client.prepare(
            "INSERT INTO ticks (time, symbol, bid, ask, last, volume, source)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        ).await?;

        for tick in ticks {
            client.execute(
                &stmt,
                &[
                    &tick.time,
                    &tick.symbol.0,
                    &tick.bid,
                    &tick.ask,
                    &tick.last,
                    &(tick.volume as i64),
                    &tick.source,
                ],
            ).await.context("Failed to insert tick")?;
        }

        debug!("Inserted {} ticks", ticks.len());
        Ok(())
    }

    async fn write_ohlcv(&self, bars: &[OHLCV]) -> Result<()> {
        if bars.is_empty() {
            return Ok(());
        }

        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        // Use a default timeframe for manually inserted bars
        let stmt = client.prepare(
            "INSERT INTO ohlcv (time, symbol, timeframe, open, high, low, close, volume)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (symbol, timeframe, time) DO UPDATE SET
                open = EXCLUDED.open,
                high = EXCLUDED.high,
                low = EXCLUDED.low,
                close = EXCLUDED.close,
                volume = EXCLUDED.volume"
        ).await?;

        for bar in bars {
            client.execute(
                &stmt,
                &[
                    &bar.time,
                    &bar.symbol.0,
                    &"M1", // Default to M1, can be extended to support other timeframes
                    &bar.open,
                    &bar.high,
                    &bar.low,
                    &bar.close,
                    &(bar.volume as i64),
                ],
            ).await.context("Failed to insert OHLCV bar")?;
        }

        debug!("Inserted {} OHLCV bars", bars.len());
        Ok(())
    }

    async fn query_ohlcv(&self, params: OHLCVQuery) -> Result<Vec<OHLCV>> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let timeframe_str = Self::timeframe_to_string(&params.timeframe);

        let query = format!(
            "SELECT time, symbol, open, high, low, close, volume
             FROM ohlcv
             WHERE symbol = $1 AND timeframe = $2 AND time >= $3 AND time <= $4
             ORDER BY time ASC
             {}",
            if let Some(limit) = params.limit {
                format!("LIMIT {}", limit)
            } else {
                String::new()
            }
        );

        let rows = client.query(
            &query,
            &[
                &params.symbol.0,
                &timeframe_str,
                &params.from,
                &params.to,
            ],
        ).await.context("Failed to query OHLCV data")?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let volume: i64 = row.get(6);
            results.push(OHLCV {
                time: row.get(0),
                symbol: Symbol(row.get(1)),
                open: row.get(2),
                high: row.get(3),
                low: row.get(4),
                close: row.get(5),
                volume: volume as u64,
            });
        }

        debug!("Queried {} OHLCV bars for {}", results.len(), params.symbol.0);
        Ok(results)
    }

    async fn write_order(&self, order: &Order) -> Result<()> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let side_str = match order.side {
            OrderSide::Buy => "Buy",
            OrderSide::Sell => "Sell",
        };

        let type_str = match order.order_type {
            OrderType::Market => "Market",
            OrderType::Limit => "Limit",
            OrderType::Stop => "Stop",
            OrderType::StopLimit => "StopLimit",
            OrderType::TrailingStop => "TrailingStop",
        };

        let status_str = match order.status {
            OrderStatus::Pending => "Pending",
            OrderStatus::Open => "Open",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
            OrderStatus::Rejected => "Rejected",
        };

        client.execute(
            "INSERT INTO orders
             (id, symbol, side, order_type, quantity, price, stop_price, status,
              filled_qty, avg_price, created_at, updated_at, broker_id, source)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            &[
                &order.id.0,
                &order.symbol.0,
                &side_str,
                &type_str,
                &order.quantity,
                &order.price,
                &order.stop_price,
                &status_str,
                &order.filled_qty,
                &order.avg_price,
                &order.created_at,
                &order.updated_at,
                &order.broker_id,
                &order.source,
            ],
        ).await.context("Failed to write order")?;

        debug!("Wrote order {}", order.id.0);
        Ok(())
    }

    async fn update_order(&self, order: &Order) -> Result<()> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let status_str = match order.status {
            OrderStatus::Pending => "Pending",
            OrderStatus::Open => "Open",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
            OrderStatus::Rejected => "Rejected",
        };

        client.execute(
            "UPDATE orders
             SET status = $1, filled_qty = $2, avg_price = $3, updated_at = $4
             WHERE id = $5",
            &[
                &status_str,
                &order.filled_qty,
                &order.avg_price,
                &order.updated_at,
                &order.id.0,
            ],
        ).await.context("Failed to update order")?;

        debug!("Updated order {}", order.id.0);
        Ok(())
    }

    async fn query_orders(&self, params: OrderQuery) -> Result<Vec<Order>> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let mut query = String::from(
            "SELECT id, symbol, side, order_type, quantity, price, stop_price, status,
                    filled_qty, avg_price, created_at, updated_at, broker_id, source
             FROM orders WHERE 1=1"
        );

        let mut param_count = 0;
        let mut query_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();

        if let Some(ref symbol) = params.symbol {
            param_count += 1;
            query.push_str(&format!(" AND symbol = ${}", param_count));
            query_params.push(Box::new(symbol.0.clone()));
        }

        if let Some(ref status) = params.status {
            param_count += 1;
            let status_str = match status {
                OrderStatus::Pending => "Pending",
                OrderStatus::Open => "Open",
                OrderStatus::PartiallyFilled => "PartiallyFilled",
                OrderStatus::Filled => "Filled",
                OrderStatus::Cancelled => "Cancelled",
                OrderStatus::Rejected => "Rejected",
            };
            query.push_str(&format!(" AND status = ${}", param_count));
            query_params.push(Box::new(status_str.to_string()));
        }

        if let Some(ref broker_id) = params.broker_id {
            param_count += 1;
            query.push_str(&format!(" AND broker_id = ${}", param_count));
            query_params.push(Box::new(broker_id.clone()));
        }

        if let Some(from) = params.from {
            param_count += 1;
            query.push_str(&format!(" AND created_at >= ${}", param_count));
            query_params.push(Box::new(from));
        }

        if let Some(to) = params.to {
            param_count += 1;
            query.push_str(&format!(" AND created_at <= ${}", param_count));
            query_params.push(Box::new(to));
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = params.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        // Convert boxed params to references for query
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            query_params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let rows = client.query(&query, &param_refs[..])
            .await.context("Failed to query orders")?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let side_str: String = row.get(2);
            let side = match side_str.as_str() {
                "Buy" => OrderSide::Buy,
                "Sell" => OrderSide::Sell,
                _ => OrderSide::Buy, // Default
            };

            let type_str: String = row.get(3);
            let order_type = match type_str.as_str() {
                "Market" => OrderType::Market,
                "Limit" => OrderType::Limit,
                "Stop" => OrderType::Stop,
                "StopLimit" => OrderType::StopLimit,
                "TrailingStop" => OrderType::TrailingStop,
                _ => OrderType::Market, // Default
            };

            let status_str: String = row.get(7);
            let status = match status_str.as_str() {
                "Pending" => OrderStatus::Pending,
                "Open" => OrderStatus::Open,
                "PartiallyFilled" => OrderStatus::PartiallyFilled,
                "Filled" => OrderStatus::Filled,
                "Cancelled" => OrderStatus::Cancelled,
                "Rejected" => OrderStatus::Rejected,
                _ => OrderStatus::Pending, // Default
            };

            results.push(Order {
                id: OrderId(row.get(0)),
                symbol: Symbol(row.get(1)),
                side,
                order_type,
                quantity: row.get(4),
                price: row.get(5),
                stop_price: row.get(6),
                status,
                filled_qty: row.get(8),
                avg_price: row.get(9),
                created_at: row.get(10),
                updated_at: row.get(11),
                broker_id: row.get(12),
                source: row.get(13),
            });
        }

        debug!("Queried {} orders", results.len());
        Ok(results)
    }

    async fn write_position(&self, pos: &Position) -> Result<()> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let side_str = match pos.side {
            OrderSide::Buy => "Buy",
            OrderSide::Sell => "Sell",
        };

        client.execute(
            "INSERT INTO positions
             (symbol, broker_id, quantity, avg_price, side, pnl, pnl_pct, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (symbol, broker_id) DO UPDATE SET
                quantity = EXCLUDED.quantity,
                avg_price = EXCLUDED.avg_price,
                side = EXCLUDED.side,
                pnl = EXCLUDED.pnl,
                pnl_pct = EXCLUDED.pnl_pct,
                updated_at = EXCLUDED.updated_at",
            &[
                &pos.symbol.0,
                &pos.broker_id,
                &pos.quantity,
                &pos.avg_price,
                &side_str,
                &pos.pnl,
                &pos.pnl_pct,
                &chrono::Utc::now(),
            ],
        ).await.context("Failed to write position")?;

        debug!("Wrote position for {} @ {}", pos.symbol.0, pos.broker_id);
        Ok(())
    }

    async fn query_positions(&self, broker_id: &str) -> Result<Vec<Position>> {
        let client = self.pool.get().await
            .context("Failed to get database connection")?;

        let rows = client.query(
            "SELECT symbol, broker_id, quantity, avg_price, side, pnl, pnl_pct
             FROM positions
             WHERE broker_id = $1",
            &[&broker_id],
        ).await.context("Failed to query positions")?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let side_str: String = row.get(4);
            let side = match side_str.as_str() {
                "Buy" => OrderSide::Buy,
                "Sell" => OrderSide::Sell,
                _ => OrderSide::Buy, // Default
            };

            results.push(Position {
                symbol: Symbol(row.get(0)),
                quantity: row.get(2),
                avg_price: row.get(3),
                side,
                pnl: row.get(5),
                pnl_pct: row.get(6),
                broker_id: row.get(1),
            });
        }

        debug!("Queried {} positions for broker {}", results.len(), broker_id);
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_mapping() {
        assert_eq!(TimescaleAdapter::timeframe_to_string(&Timeframe::M1), "M1");
        assert_eq!(TimescaleAdapter::timeframe_to_string(&Timeframe::H1), "H1");
        assert_eq!(TimescaleAdapter::timeframe_to_interval(&Timeframe::M5), "5 minutes");
    }
}
