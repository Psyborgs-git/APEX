use anyhow::{Context, Result};
use duckdb::{Connection, params};
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

use apex_core::domain::models::*;

/// DuckDB adapter for fast analytical queries
///
/// Optimized for:
/// - Backtesting data slices
/// - Correlation analysis
/// - Rolling statistics
/// - Parquet export/import
/// - OLAP-style aggregations
pub struct DuckDBAdapter {
    conn: Arc<Mutex<Connection>>,
}

impl DuckDBAdapter {
    /// Create a new DuckDB adapter
    ///
    /// # Arguments
    /// * `db_path` - Path to DuckDB file (use `:memory:` for in-memory database)
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .context("Failed to open DuckDB connection")?;

        let adapter = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        adapter.initialize_schema()?;

        info!("DuckDB adapter initialized at {}", db_path);
        Ok(adapter)
    }

    /// Initialize analytical tables
    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create OHLCV table optimized for analytical queries
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS ohlcv_analytics (
                time        TIMESTAMP NOT NULL,
                symbol      VARCHAR NOT NULL,
                timeframe   VARCHAR NOT NULL,
                open        DOUBLE NOT NULL,
                high        DOUBLE NOT NULL,
                low         DOUBLE NOT NULL,
                close       DOUBLE NOT NULL,
                volume      BIGINT NOT NULL,
                returns     DOUBLE,
                log_returns DOUBLE
            );

            CREATE INDEX IF NOT EXISTS idx_ohlcv_symbol_time
                ON ohlcv_analytics (symbol, timeframe, time);
        "#)?;

        // Create trades table for performance analysis
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS trades_analytics (
                id          VARCHAR PRIMARY KEY,
                symbol      VARCHAR NOT NULL,
                side        VARCHAR NOT NULL,
                quantity    DOUBLE NOT NULL,
                entry_price DOUBLE NOT NULL,
                exit_price  DOUBLE,
                entry_time  TIMESTAMP NOT NULL,
                exit_time   TIMESTAMP,
                pnl         DOUBLE,
                pnl_pct     DOUBLE,
                strategy_id VARCHAR,
                tags        VARCHAR
            );
        "#)?;

        debug!("DuckDB schema initialized");
        Ok(())
    }

    /// Load OHLCV data into DuckDB for analysis
    pub fn load_ohlcv(&self, bars: &[OHLCV], timeframe: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "INSERT INTO ohlcv_analytics
             (time, symbol, timeframe, open, high, low, close, volume, returns, log_returns)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )?;

        for (i, bar) in bars.iter().enumerate() {
            let returns = if i > 0 {
                (bar.close - bars[i - 1].close) / bars[i - 1].close
            } else {
                0.0
            };

            let log_returns = if i > 0 && bars[i - 1].close > 0.0 {
                (bar.close / bars[i - 1].close).ln()
            } else {
                0.0
            };

            stmt.execute(params![
                bar.time.to_rfc3339(),
                &bar.symbol.0,
                timeframe,
                bar.open,
                bar.high,
                bar.low,
                bar.close,
                bar.volume as i64,
                returns,
                log_returns,
            ])?;
        }

        debug!("Loaded {} bars for analytical queries", bars.len());
        Ok(())
    }

    /// Calculate rolling correlation between two instruments
    ///
    /// Returns correlation coefficient over specified window
    pub fn rolling_correlation(
        &self,
        symbol1: &Symbol,
        symbol2: &Symbol,
        window_days: i32,
    ) -> Result<Vec<(chrono::DateTime<chrono::Utc>, f64)>> {
        let conn = self.conn.lock().unwrap();

        let query = format!(r#"
            WITH returns AS (
                SELECT
                    time,
                    symbol,
                    log_returns
                FROM ohlcv_analytics
                WHERE symbol IN (?, ?)
                  AND timeframe = 'D1'
                ORDER BY time
            ),
            pivoted AS (
                SELECT
                    time,
                    MAX(CASE WHEN symbol = ? THEN log_returns END) AS ret1,
                    MAX(CASE WHEN symbol = ? THEN log_returns END) AS ret2
                FROM returns
                GROUP BY time
            )
            SELECT
                time,
                CORR(ret1, ret2) OVER (
                    ORDER BY time
                    ROWS BETWEEN {} PRECEDING AND CURRENT ROW
                ) AS correlation
            FROM pivoted
            WHERE ret1 IS NOT NULL AND ret2 IS NOT NULL
            ORDER BY time
        "#, window_days - 1);

        let mut stmt = conn.prepare(&query)?;

        let rows = stmt.query_map(
            params![&symbol1.0, &symbol2.0, &symbol1.0, &symbol2.0],
            |row| {
                let time_str: String = row.get(0)?;
                let corr: f64 = row.get(1)?;

                let time = chrono::DateTime::parse_from_rfc3339(&time_str)
                    .ok()
                    .and_then(|dt| Some(dt.with_timezone(&chrono::Utc)))
                    .unwrap_or_else(chrono::Utc::now);

                Ok((time, corr))
            },
        )?;

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
        }

        debug!("Calculated rolling correlation for {} vs {} over {} days", symbol1.0, symbol2.0, window_days);
        Ok(results)
    }

    /// Calculate volatility (standard deviation of returns) over a rolling window
    pub fn rolling_volatility(
        &self,
        symbol: &Symbol,
        window_days: i32,
    ) -> Result<Vec<(chrono::DateTime<chrono::Utc>, f64)>> {
        let conn = self.conn.lock().unwrap();

        let query = format!(r#"
            SELECT
                time,
                STDDEV(log_returns) OVER (
                    ORDER BY time
                    ROWS BETWEEN {} PRECEDING AND CURRENT ROW
                ) AS volatility
            FROM ohlcv_analytics
            WHERE symbol = ? AND timeframe = 'D1'
            ORDER BY time
        "#, window_days - 1);

        let mut stmt = conn.prepare(&query)?;

        let rows = stmt.query_map(params![&symbol.0], |row| {
            let time_str: String = row.get(0)?;
            let vol: f64 = row.get(1)?;

            let time = chrono::DateTime::parse_from_rfc3339(&time_str)
                .ok()
                .and_then(|dt| Some(dt.with_timezone(&chrono::Utc)))
                .unwrap_or_else(chrono::Utc::now);

            Ok((time, vol))
        })?;

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
        }

        debug!("Calculated rolling volatility for {} over {} days", symbol.0, window_days);
        Ok(results)
    }

    /// Export OHLCV data to Parquet file
    pub fn export_to_parquet(&self, symbol: &Symbol, output_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let query = format!(
            "COPY (SELECT * FROM ohlcv_analytics WHERE symbol = '{}') TO '{}' (FORMAT PARQUET)",
            symbol.0, output_path
        );

        conn.execute(&query, [])?;

        info!("Exported {} data to {}", symbol.0, output_path);
        Ok(())
    }

    /// Import OHLCV data from Parquet file
    pub fn import_from_parquet(&self, parquet_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let query = format!(
            "INSERT INTO ohlcv_analytics SELECT * FROM read_parquet('{}')",
            parquet_path
        );

        conn.execute(&query, [])?;

        info!("Imported data from {}", parquet_path);
        Ok(())
    }

    /// Calculate Sharpe ratio for a series of returns
    pub fn calculate_sharpe_ratio(&self, symbol: &Symbol, risk_free_rate: f64) -> Result<f64> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(r#"
            SELECT
                AVG(log_returns) AS mean_return,
                STDDEV(log_returns) AS std_return
            FROM ohlcv_analytics
            WHERE symbol = ? AND timeframe = 'D1'
              AND log_returns IS NOT NULL
        "#)?;

        let row = stmt.query_row(params![&symbol.0], |row| {
            let mean: f64 = row.get(0)?;
            let std: f64 = row.get(1)?;
            Ok((mean, std))
        })?;

        let (mean_return, std_return) = row;

        // Annualize (assuming daily returns)
        let annualized_return = mean_return * 252.0;
        let annualized_vol = std_return * (252.0_f64).sqrt();

        let sharpe = if annualized_vol > 0.0 {
            (annualized_return - risk_free_rate) / annualized_vol
        } else {
            0.0
        };

        debug!("Calculated Sharpe ratio for {}: {:.3}", symbol.0, sharpe);
        Ok(sharpe)
    }

    /// Calculate maximum drawdown
    pub fn calculate_max_drawdown(&self, symbol: &Symbol) -> Result<f64> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(r#"
            WITH cumulative AS (
                SELECT
                    time,
                    close,
                    MAX(close) OVER (ORDER BY time) AS running_max
                FROM ohlcv_analytics
                WHERE symbol = ? AND timeframe = 'D1'
                ORDER BY time
            )
            SELECT
                MIN((close - running_max) / running_max) AS max_drawdown
            FROM cumulative
        "#)?;

        let max_dd: f64 = stmt.query_row(params![&symbol.0], |row| row.get(0))?;

        debug!("Calculated max drawdown for {}: {:.2}%", symbol.0, max_dd * 100.0);
        Ok(max_dd)
    }

    /// Get descriptive statistics for a symbol
    pub fn get_statistics(&self, symbol: &Symbol) -> Result<SymbolStats> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(r#"
            SELECT
                COUNT(*) AS count,
                AVG(close) AS mean_price,
                STDDEV(close) AS std_price,
                MIN(close) AS min_price,
                MAX(close) AS max_price,
                AVG(volume) AS avg_volume,
                AVG(returns) AS mean_return,
                STDDEV(returns) AS std_return
            FROM ohlcv_analytics
            WHERE symbol = ? AND timeframe = 'D1'
        "#)?;

        let stats = stmt.query_row(params![&symbol.0], |row| {
            Ok(SymbolStats {
                count: row.get(0)?,
                mean_price: row.get(1)?,
                std_price: row.get(2)?,
                min_price: row.get(3)?,
                max_price: row.get(4)?,
                avg_volume: row.get(5)?,
                mean_return: row.get(6)?,
                std_return: row.get(7)?,
            })
        })?;

        debug!("Retrieved statistics for {}", symbol.0);
        Ok(stats)
    }

    /// Run custom SQL query (for advanced analytics)
    pub fn query_raw(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(sql)?;
        let column_count = stmt.column_count();

        let rows = stmt.query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..column_count {
                let val: Result<String, _> = row.get(i);
                values.push(val.unwrap_or_else(|_| "NULL".to_string()));
            }
            Ok(values)
        })?;

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
        }

        Ok(results)
    }
}

/// Statistical summary for a symbol
#[derive(Debug, Clone)]
pub struct SymbolStats {
    pub count: i64,
    pub mean_price: f64,
    pub std_price: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub avg_volume: f64,
    pub mean_return: f64,
    pub std_return: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duckdb_creation() -> Result<()> {
        let adapter = DuckDBAdapter::new(":memory:")?;
        // Should successfully create in-memory database
        Ok(())
    }

    #[test]
    fn test_parquet_path_format() {
        let symbol = Symbol("AAPL".to_string());
        let path = format!("data/parquet/{}_daily.parquet", symbol.0);
        assert_eq!(path, "data/parquet/AAPL_daily.parquet");
    }
}
