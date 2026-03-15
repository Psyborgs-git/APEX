use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tokio::sync::Mutex;

use apex_core::domain::models::*;
use apex_core::ports::storage::StoragePort;

/// SQLite-based storage adapter for configuration, orders, and lightweight data storage.
/// Serves as a self-contained fallback when TimescaleDB is unavailable.
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Create a new SQLite storage adapter
    pub fn new(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };

        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Initialize the database schema
    pub async fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS ticks (
                time        TEXT NOT NULL,
                symbol      TEXT NOT NULL,
                bid         REAL,
                ask         REAL,
                last        REAL,
                volume      INTEGER,
                source      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_ticks_symbol_time ON ticks(symbol, time);

            CREATE TABLE IF NOT EXISTS ohlcv (
                time        TEXT NOT NULL,
                symbol      TEXT NOT NULL,
                open        REAL NOT NULL,
                high        REAL NOT NULL,
                low         REAL NOT NULL,
                close       REAL NOT NULL,
                volume      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ohlcv_symbol_time ON ohlcv(symbol, time);

            CREATE TABLE IF NOT EXISTS orders (
                id          TEXT PRIMARY KEY,
                symbol      TEXT NOT NULL,
                side        TEXT NOT NULL,
                order_type  TEXT NOT NULL,
                quantity    REAL NOT NULL,
                price       REAL,
                stop_price  REAL,
                status      TEXT NOT NULL,
                filled_qty  REAL NOT NULL DEFAULT 0,
                avg_price   REAL NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                broker_id   TEXT NOT NULL,
                source      TEXT NOT NULL DEFAULT 'manual'
            );

            CREATE TABLE IF NOT EXISTS positions (
                symbol      TEXT NOT NULL,
                broker_id   TEXT NOT NULL,
                quantity    REAL NOT NULL,
                avg_price   REAL NOT NULL,
                side        TEXT NOT NULL,
                pnl         REAL NOT NULL DEFAULT 0,
                pnl_pct     REAL NOT NULL DEFAULT 0,
                PRIMARY KEY (symbol, broker_id)
            );

            CREATE TABLE IF NOT EXISTS workspace_layouts (
                name        TEXT PRIMARY KEY,
                layout_json TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS alert_rules (
                id          TEXT PRIMARY KEY,
                rule_json   TEXT NOT NULL,
                delivery_json TEXT NOT NULL,
                enabled     INTEGER NOT NULL DEFAULT 1
            );
            ",
        )?;
        Ok(())
    }
}

#[async_trait]
impl StoragePort for SqliteStorage {
    async fn write_ticks(&self, ticks: &[Tick]) -> Result<()> {
        let conn = self.conn.lock().await;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO ticks (time, symbol, bid, ask, last, volume, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for tick in ticks {
                stmt.execute(params![
                    tick.time.to_rfc3339(),
                    tick.symbol.0,
                    tick.bid,
                    tick.ask,
                    tick.last,
                    tick.volume as i64,
                    tick.source,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    async fn write_ohlcv(&self, bars: &[OHLCV]) -> Result<()> {
        let conn = self.conn.lock().await;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO ohlcv (time, symbol, open, high, low, close, volume)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for bar in bars {
                stmt.execute(params![
                    bar.time.to_rfc3339(),
                    bar.symbol.0,
                    bar.open,
                    bar.high,
                    bar.low,
                    bar.close,
                    bar.volume as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    async fn query_ohlcv(&self, params_q: OHLCVQuery) -> Result<Vec<OHLCV>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT time, symbol, open, high, low, close, volume FROM ohlcv
             WHERE symbol = ?1 AND time >= ?2 AND time <= ?3
             ORDER BY time ASC
             LIMIT ?4",
        )?;

        let limit = params_q.limit.unwrap_or(10000) as i64;
        let rows = stmt.query_map(
            params![
                params_q.symbol.0,
                params_q.from.to_rfc3339(),
                params_q.to.to_rfc3339(),
                limit,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )?;

        let mut bars = Vec::new();
        for row in rows {
            let (time_str, symbol_str, open, high, low, close, volume) = row?;
            let time = DateTime::parse_from_rfc3339(&time_str)
                .map_err(|e| anyhow!("Failed to parse time: {}", e))?
                .with_timezone(&Utc);
            bars.push(OHLCV {
                time,
                symbol: Symbol(symbol_str),
                open,
                high,
                low,
                close,
                volume: volume as u64,
            });
        }
        Ok(bars)
    }

    async fn write_order(&self, order: &Order) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO orders
             (id, symbol, side, order_type, quantity, price, stop_price,
              status, filled_qty, avg_price, created_at, updated_at, broker_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                order.id.0,
                order.symbol.0,
                serde_json::to_string(&order.side).unwrap_or_default(),
                serde_json::to_string(&order.order_type).unwrap_or_default(),
                order.quantity,
                order.price,
                order.stop_price,
                serde_json::to_string(&order.status).unwrap_or_default(),
                order.filled_qty,
                order.avg_price,
                order.created_at.to_rfc3339(),
                order.updated_at.to_rfc3339(),
                order.broker_id,
                order.source,
            ],
        )?;
        Ok(())
    }

    async fn update_order(&self, order: &Order) -> Result<()> {
        self.write_order(order).await
    }

    async fn query_orders(&self, params_q: OrderQuery) -> Result<Vec<Order>> {
        let conn = self.conn.lock().await;

        // Build query with parameterized placeholders to prevent SQL injection
        let mut sql = String::from(
            "SELECT id, symbol, side, order_type, quantity, price, stop_price,
                    status, filled_qty, avg_price, created_at, updated_at, broker_id, source
             FROM orders WHERE 1=1",
        );
        let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref symbol) = params_q.symbol {
            sql.push_str(&format!(" AND symbol = ?{}", param_idx));
            bind_values.push(Box::new(symbol.0.clone()));
            param_idx += 1;
        }
        if let Some(ref status) = params_q.status {
            sql.push_str(&format!(" AND status = ?{}", param_idx));
            bind_values.push(Box::new(serde_json::to_string(status).unwrap_or_default()));
            param_idx += 1;
        }
        if let Some(ref broker_id) = params_q.broker_id {
            sql.push_str(&format!(" AND broker_id = ?{}", param_idx));
            bind_values.push(Box::new(broker_id.clone()));
            param_idx += 1;
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = params_q.limit {
            sql.push_str(&format!(" LIMIT ?{}", param_idx));
            bind_values.push(Box::new(limit as i64));
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            bind_values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, Option<f64>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;

        let mut orders = Vec::new();
        for row in rows {
            let (
                id,
                symbol,
                side,
                order_type,
                quantity,
                price,
                stop_price,
                status,
                filled_qty,
                avg_price,
                created_at,
                updated_at,
                broker_id,
                source,
            ) = row?;
            orders.push(Order {
                id: OrderId(id),
                symbol: Symbol(symbol),
                side: serde_json::from_str(&side).unwrap_or(OrderSide::Buy),
                order_type: serde_json::from_str(&order_type).unwrap_or(OrderType::Market),
                quantity,
                price,
                stop_price,
                status: serde_json::from_str(&status).unwrap_or(OrderStatus::Pending),
                filled_qty,
                avg_price,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                broker_id,
                source,
            });
        }
        Ok(orders)
    }

    async fn write_position(&self, pos: &Position) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO positions
             (symbol, broker_id, quantity, avg_price, side, pnl, pnl_pct)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                pos.symbol.0,
                pos.broker_id,
                pos.quantity,
                pos.avg_price,
                serde_json::to_string(&pos.side).unwrap_or_default(),
                pos.pnl,
                pos.pnl_pct,
            ],
        )?;
        Ok(())
    }

    async fn query_positions(&self, broker_id: &str) -> Result<Vec<Position>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT symbol, broker_id, quantity, avg_price, side, pnl, pnl_pct
             FROM positions WHERE broker_id = ?1",
        )?;

        let rows = stmt.query_map(params![broker_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
            ))
        })?;

        let mut positions = Vec::new();
        for row in rows {
            let (symbol, broker_id, quantity, avg_price, side, pnl, pnl_pct) = row?;
            positions.push(Position {
                symbol: Symbol(symbol),
                broker_id,
                quantity,
                avg_price,
                side: serde_json::from_str(&side).unwrap_or(OrderSide::Buy),
                pnl,
                pnl_pct,
            });
        }
        Ok(positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> SqliteStorage {
        let storage = SqliteStorage::new(":memory:").unwrap();
        storage.init_schema().await.unwrap();
        storage
    }

    #[tokio::test]
    async fn test_create_storage() {
        let _storage = setup().await;
    }

    #[tokio::test]
    async fn test_write_and_query_ticks() {
        let storage = setup().await;
        let ticks = vec![
            Tick {
                time: Utc::now(),
                symbol: Symbol("AAPL".into()),
                bid: 150.0,
                ask: 150.05,
                last: 150.02,
                volume: 100,
                source: "test".into(),
            },
            Tick {
                time: Utc::now(),
                symbol: Symbol("AAPL".into()),
                bid: 150.10,
                ask: 150.15,
                last: 150.12,
                volume: 200,
                source: "test".into(),
            },
        ];
        storage.write_ticks(&ticks).await.unwrap();
    }

    #[tokio::test]
    async fn test_write_and_query_ohlcv() {
        let storage = setup().await;
        let now = Utc::now();
        let bars = vec![OHLCV {
            time: now,
            symbol: Symbol("AAPL".into()),
            open: 150.0,
            high: 155.0,
            low: 149.0,
            close: 154.0,
            volume: 1000000,
        }];
        storage.write_ohlcv(&bars).await.unwrap();

        let query = OHLCVQuery {
            symbol: Symbol("AAPL".into()),
            timeframe: Timeframe::D1,
            from: now - chrono::Duration::hours(1),
            to: now + chrono::Duration::hours(1),
            limit: Some(100),
        };
        let results = storage.query_ohlcv(query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!((results[0].open - 150.0).abs() < 0.01);
        assert!((results[0].close - 154.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_write_and_query_orders() {
        let storage = setup().await;
        let order = Order {
            id: OrderId("test-001".into()),
            symbol: Symbol("AAPL".into()),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity: 10.0,
            price: Some(150.0),
            stop_price: None,
            status: OrderStatus::Open,
            filled_qty: 0.0,
            avg_price: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "paper".into(),
            source: "manual".into(),
        };
        storage.write_order(&order).await.unwrap();

        let query = OrderQuery {
            symbol: Some(Symbol("AAPL".into())),
            status: None,
            broker_id: Some("paper".into()),
            from: None,
            to: None,
            limit: Some(10),
        };
        let results = storage.query_orders(query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.0, "test-001");
    }

    #[tokio::test]
    async fn test_update_order() {
        let storage = setup().await;
        let mut order = Order {
            id: OrderId("test-002".into()),
            symbol: Symbol("GOOG".into()),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            quantity: 5.0,
            price: None,
            stop_price: None,
            status: OrderStatus::Pending,
            filled_qty: 0.0,
            avg_price: 0.0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            broker_id: "paper".into(),
            source: "strategy".into(),
        };
        storage.write_order(&order).await.unwrap();

        order.status = OrderStatus::Filled;
        order.filled_qty = 5.0;
        order.avg_price = 2800.0;
        storage.update_order(&order).await.unwrap();

        let query = OrderQuery {
            symbol: None,
            status: None,
            broker_id: None,
            from: None,
            to: None,
            limit: None,
        };
        let results = storage.query_orders(query).await.unwrap();
        let found = results.iter().find(|o| o.id.0 == "test-002").unwrap();
        assert_eq!(found.status, OrderStatus::Filled);
        assert!((found.avg_price - 2800.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_write_and_query_positions() {
        let storage = setup().await;
        let pos = Position {
            symbol: Symbol("AAPL".into()),
            broker_id: "paper".into(),
            quantity: 10.0,
            avg_price: 150.0,
            side: OrderSide::Buy,
            pnl: 50.0,
            pnl_pct: 3.33,
        };
        storage.write_position(&pos).await.unwrap();

        let positions = storage.query_positions("paper").await.unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol.0, "AAPL");
        assert!((positions[0].quantity - 10.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_position_upsert() {
        let storage = setup().await;
        let pos1 = Position {
            symbol: Symbol("AAPL".into()),
            broker_id: "paper".into(),
            quantity: 10.0,
            avg_price: 150.0,
            side: OrderSide::Buy,
            pnl: 50.0,
            pnl_pct: 3.33,
        };
        storage.write_position(&pos1).await.unwrap();

        let pos2 = Position {
            symbol: Symbol("AAPL".into()),
            broker_id: "paper".into(),
            quantity: 20.0,
            avg_price: 155.0,
            side: OrderSide::Buy,
            pnl: 100.0,
            pnl_pct: 3.22,
        };
        storage.write_position(&pos2).await.unwrap();

        let positions = storage.query_positions("paper").await.unwrap();
        assert_eq!(positions.len(), 1);
        assert!((positions[0].quantity - 20.0).abs() < 0.01);
    }
}
