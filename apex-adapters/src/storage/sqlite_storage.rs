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

            -- P2: Instrument master
            CREATE TABLE IF NOT EXISTS instruments (
                symbol          TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                exchange        TEXT NOT NULL,
                instrument_type TEXT NOT NULL,
                sector          TEXT,
                currency        TEXT NOT NULL,
                lot_size        REAL NOT NULL DEFAULT 1,
                tick_size       REAL NOT NULL DEFAULT 0.01,
                isin            TEXT,
                listing_date    TEXT,
                is_active       INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_instruments_exchange ON instruments(exchange);

            -- P2: Corporate actions
            CREATE TABLE IF NOT EXISTS corporate_actions (
                id          TEXT PRIMARY KEY,
                symbol      TEXT NOT NULL,
                action_type TEXT NOT NULL,
                ex_date     TEXT NOT NULL,
                ratio       REAL,
                amount      REAL,
                description TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_corp_actions_symbol ON corporate_actions(symbol, ex_date);

            -- P3: Strategy state persistence
            CREATE TABLE IF NOT EXISTS strategy_state (
                strategy_id TEXT PRIMARY KEY,
                state_json  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
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

    // --- P2: Instrument master -----------------------------------------------

    async fn upsert_instrument(&self, meta: &InstrumentMetadata) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO instruments
             (symbol, name, exchange, instrument_type, sector, currency,
              lot_size, tick_size, isin, listing_date, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                meta.symbol.0,
                meta.name,
                meta.exchange,
                serde_json::to_string(&meta.instrument_type).unwrap_or_default(),
                meta.sector,
                meta.currency,
                meta.lot_size,
                meta.tick_size,
                meta.isin,
                meta.listing_date.map(|d| d.to_rfc3339()),
                meta.is_active as i64,
            ],
        )?;
        Ok(())
    }

    async fn get_instrument(&self, symbol: &Symbol) -> Result<Option<InstrumentMetadata>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT symbol, name, exchange, instrument_type, sector, currency,
                    lot_size, tick_size, isin, listing_date, is_active
             FROM instruments WHERE symbol = ?1",
        )?;

        let mut rows = stmt.query_map(params![symbol.0], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;

        if let Some(row) = rows.next() {
            let (sym, name, exchange, itype, sector, currency, lot_size, tick_size, isin, listing_date, is_active) =
                row?;
            let listing = listing_date
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc));
            Ok(Some(InstrumentMetadata {
                symbol: Symbol(sym),
                name,
                exchange,
                instrument_type: serde_json::from_str(&itype).unwrap_or(InstrumentType::Equity),
                sector,
                currency,
                lot_size,
                tick_size,
                isin,
                listing_date: listing,
                is_active: is_active != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn query_instruments(&self, params_q: InstrumentQuery) -> Result<Vec<InstrumentMetadata>> {
        let conn = self.conn.lock().await;
        let mut sql = String::from(
            "SELECT symbol, name, exchange, instrument_type, sector, currency,
                    lot_size, tick_size, isin, listing_date, is_active
             FROM instruments WHERE 1=1",
        );
        let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref sym) = params_q.symbol {
            sql.push_str(&format!(" AND symbol = ?{}", param_idx));
            bind_values.push(Box::new(sym.0.clone()));
            param_idx += 1;
        }
        if let Some(ref exchange) = params_q.exchange {
            sql.push_str(&format!(" AND exchange = ?{}", param_idx));
            bind_values.push(Box::new(exchange.clone()));
            param_idx += 1;
        }
        if let Some(ref itype) = params_q.instrument_type {
            sql.push_str(&format!(" AND instrument_type = ?{}", param_idx));
            bind_values.push(Box::new(serde_json::to_string(itype).unwrap_or_default()));
            param_idx += 1;
        }
        if let Some(active) = params_q.is_active {
            sql.push_str(&format!(" AND is_active = ?{}", param_idx));
            bind_values.push(Box::new(active as i64));
            param_idx += 1;
        }
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
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;

        let mut instruments = Vec::new();
        for row in rows {
            let (sym, name, exchange, itype, sector, currency, lot_size, tick_size, isin, listing_date, is_active) =
                row?;
            let listing = listing_date
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc));
            instruments.push(InstrumentMetadata {
                symbol: Symbol(sym),
                name,
                exchange,
                instrument_type: serde_json::from_str(&itype).unwrap_or(InstrumentType::Equity),
                sector,
                currency,
                lot_size,
                tick_size,
                isin,
                listing_date: listing,
                is_active: is_active != 0,
            });
        }
        Ok(instruments)
    }

    async fn write_corporate_action(&self, action: &CorporateAction) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO corporate_actions
             (id, symbol, action_type, ex_date, ratio, amount, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                action.id.to_string(),
                action.symbol.0,
                serde_json::to_string(&action.action_type).unwrap_or_default(),
                action.ex_date.to_rfc3339(),
                action.ratio,
                action.amount,
                action.description,
            ],
        )?;
        Ok(())
    }

    async fn query_corporate_actions(&self, symbol: &Symbol) -> Result<Vec<CorporateAction>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, symbol, action_type, ex_date, ratio, amount, description
             FROM corporate_actions WHERE symbol = ?1
             ORDER BY ex_date ASC",
        )?;

        let rows = stmt.query_map(params![symbol.0], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut actions = Vec::new();
        for row in rows {
            let (id, sym, atype, ex_date, ratio, amount, description) = row?;
            let ex_dt = DateTime::parse_from_rfc3339(&ex_date)
                .map_err(|e| anyhow!("Failed to parse ex_date: {}", e))?
                .with_timezone(&Utc);
            actions.push(CorporateAction {
                id: id.parse().unwrap_or_else(|_| uuid::Uuid::new_v4()),
                symbol: Symbol(sym),
                action_type: serde_json::from_str(&atype)
                    .unwrap_or(CorporateActionType::Dividend),
                ex_date: ex_dt,
                ratio,
                amount,
                description,
            });
        }
        Ok(actions)
    }

    // --- P3: Strategy state ------------------------------------------------

    async fn save_strategy_state(&self, strategy_id: &str, state_json: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO strategy_state (strategy_id, state_json, updated_at)
             VALUES (?1, ?2, ?3)",
            params![strategy_id, state_json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    async fn load_strategy_state(&self, strategy_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT state_json FROM strategy_state WHERE strategy_id = ?1",
            params![strategy_id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json)                              => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e)                                => Err(anyhow!("Query error: {}", e)),
        }
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

    // --- P2: Instrument master tests ----------------------------------------

    #[tokio::test]
    async fn test_upsert_and_get_instrument() {
        let storage = setup().await;
        let meta = InstrumentMetadata {
            symbol:          Symbol("RELIANCE".into()),
            name:            "Reliance Industries Ltd".into(),
            exchange:        "NSE".into(),
            instrument_type: InstrumentType::Equity,
            sector:          Some("Energy".into()),
            currency:        "INR".into(),
            lot_size:        1.0,
            tick_size:       0.05,
            isin:            Some("INE002A01018".into()),
            listing_date:    None,
            is_active:       true,
        };
        storage.upsert_instrument(&meta).await.unwrap();

        let found = storage.get_instrument(&Symbol("RELIANCE".into())).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Reliance Industries Ltd");
        assert_eq!(found.exchange, "NSE");
        assert!(found.is_active);
    }

    #[tokio::test]
    async fn test_get_instrument_not_found() {
        let storage = setup().await;
        let result = storage.get_instrument(&Symbol("NONEXISTENT".into())).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_query_instruments_by_exchange() {
        let storage = setup().await;
        for sym in &["AAPL", "MSFT", "GOOGL"] {
            storage.upsert_instrument(&InstrumentMetadata {
                symbol:          Symbol(sym.to_string()),
                name:            sym.to_string(),
                exchange:        "NYSE".into(),
                instrument_type: InstrumentType::Equity,
                sector:          None,
                currency:        "USD".into(),
                lot_size:        1.0,
                tick_size:       0.01,
                isin:            None,
                listing_date:    None,
                is_active:       true,
            }).await.unwrap();
        }
        let results = storage.query_instruments(InstrumentQuery {
            symbol: None,
            exchange: Some("NYSE".into()),
            instrument_type: None,
            is_active: None,
            limit: None,
        }).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_write_and_query_corporate_actions() {
        let storage = setup().await;
        let action = CorporateAction {
            id:          uuid::Uuid::new_v4(),
            symbol:      Symbol("AAPL".into()),
            action_type: CorporateActionType::Split,
            ex_date:     Utc::now(),
            ratio:       Some(4.0),
            amount:      None,
            description: "4:1 stock split".into(),
        };
        storage.write_corporate_action(&action).await.unwrap();

        let actions = storage.query_corporate_actions(&Symbol("AAPL".into())).await.unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].description, "4:1 stock split");
        assert!((actions[0].ratio.unwrap() - 4.0).abs() < 1e-9);
    }

    // --- P3: Strategy state tests -------------------------------------------

    #[tokio::test]
    async fn test_save_and_load_strategy_state() {
        let storage = setup().await;
        storage.save_strategy_state("strat-1", r#"{"position":100}"#).await.unwrap();

        let loaded = storage.load_strategy_state("strat-1").await.unwrap();
        assert!(loaded.is_some());
        assert!(loaded.unwrap().contains("position"));
    }

    #[tokio::test]
    async fn test_load_strategy_state_not_found() {
        let storage = setup().await;
        let loaded = storage.load_strategy_state("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_strategy_state_overwrite() {
        let storage = setup().await;
        storage.save_strategy_state("strat-2", r#"{"v":1}"#).await.unwrap();
        storage.save_strategy_state("strat-2", r#"{"v":2}"#).await.unwrap();

        let loaded = storage.load_strategy_state("strat-2").await.unwrap().unwrap();
        assert!(loaded.contains(r#""v":2"#));
    }
}
