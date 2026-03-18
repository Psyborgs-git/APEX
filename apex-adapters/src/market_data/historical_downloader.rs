use apex_core::{
    domain::models::*,
    ports::market_data::AdapterHealth,
};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

/// Historical data downloader
///
/// Downloads bulk historical OHLCV data from free sources (Yahoo Finance)
/// for backtesting and analysis. Data is stored as CSV files on disk.
pub struct HistoricalDownloader {
    client: Client,
    output_dir: PathBuf,
    health: Arc<RwLock<AdapterHealth>>,
}

/// Download job configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJob {
    /// Symbols to download
    pub symbols: Vec<Symbol>,
    /// Start date
    pub from: DateTime<Utc>,
    /// End date
    pub to: DateTime<Utc>,
    /// Timeframe (only D1 and W1 supported for free data)
    pub timeframe: Timeframe,
}

/// Download result for a single symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    pub symbol: Symbol,
    pub bars_downloaded: usize,
    pub file_path: String,
    pub status: DownloadStatus,
}

/// Status of a download
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    Success,
    Failed(String),
    Skipped(String),
}

/// Full download output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOutput {
    pub total_symbols: usize,
    pub successful: usize,
    pub failed: usize,
    pub results: Vec<DownloadResult>,
}

impl HistoricalDownloader {
    /// Create a new historical data downloader
    pub fn new(output_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&output_dir)
            .context("Failed to create output directory")?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            output_dir,
            health: Arc::new(RwLock::new(AdapterHealth::Healthy)),
        })
    }

    /// Download historical data for all symbols in a job
    pub async fn download(&self, job: &DownloadJob) -> Result<DownloadOutput> {
        info!(
            "Starting download for {} symbols from {} to {}",
            job.symbols.len(),
            job.from.format("%Y-%m-%d"),
            job.to.format("%Y-%m-%d")
        );

        let mut results = Vec::new();
        let mut successful = 0;
        let mut failed = 0;

        for symbol in &job.symbols {
            match self.download_symbol(symbol, &job.from, &job.to, &job.timeframe).await {
                Ok(result) => {
                    if result.status == DownloadStatus::Success {
                        successful += 1;
                    }
                    results.push(result);
                }
                Err(e) => {
                    failed += 1;
                    results.push(DownloadResult {
                        symbol: symbol.clone(),
                        bars_downloaded: 0,
                        file_path: String::new(),
                        status: DownloadStatus::Failed(e.to_string()),
                    });
                    error!("Failed to download {}: {}", symbol.0, e);
                }
            }

            // Rate limiting: 500ms between requests
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let output = DownloadOutput {
            total_symbols: job.symbols.len(),
            successful,
            failed,
            results,
        };

        info!(
            "Download complete: {}/{} successful, {} failed",
            output.successful, output.total_symbols, output.failed
        );

        Ok(output)
    }

    /// Download data for a single symbol from Yahoo Finance
    async fn download_symbol(
        &self,
        symbol: &Symbol,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
        timeframe: &Timeframe,
    ) -> Result<DownloadResult> {
        let interval = match timeframe {
            Timeframe::D1 => "1d",
            Timeframe::W1 => "1wk",
            Timeframe::H1 => "1h",
            _ => "1d",
        };

        let period1 = from.timestamp();
        let period2 = to.timestamp();

        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1={}&period2={}&interval={}&includePrePost=false",
            symbol.0, period1, period2, interval
        );

        debug!("Downloading {} from Yahoo Finance", symbol.0);

        let response = self.client
            .get(&url)
            .header("User-Agent", "APEX-Trading-Terminal/0.1")
            .send()
            .await
            .context(format!("Failed to fetch data for {}", symbol.0))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Yahoo Finance HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let data: serde_json::Value = response.json().await
            .context("Failed to parse Yahoo Finance response")?;

        // Parse Yahoo Finance chart response
        let chart = data
            .get("chart")
            .and_then(|c| c.get("result"))
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("Invalid Yahoo Finance response"))?;

        let timestamps = chart
            .get("timestamp")
            .and_then(|t| t.as_array())
            .ok_or_else(|| anyhow::anyhow!("No timestamp data"))?;

        let indicators = chart
            .get("indicators")
            .and_then(|i| i.get("quote"))
            .and_then(|q| q.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("No quote data"))?;

        let opens = indicators.get("open").and_then(|v| v.as_array());
        let highs = indicators.get("high").and_then(|v| v.as_array());
        let lows = indicators.get("low").and_then(|v| v.as_array());
        let closes = indicators.get("close").and_then(|v| v.as_array());
        let volumes = indicators.get("volume").and_then(|v| v.as_array());

        let (opens, highs, lows, closes, volumes) = match (opens, highs, lows, closes, volumes) {
            (Some(o), Some(h), Some(l), Some(c), Some(v)) => (o, h, l, c, v),
            _ => return Err(anyhow::anyhow!("Incomplete OHLCV data for {}", symbol.0)),
        };

        // Build CSV content
        let mut csv = String::from("timestamp,open,high,low,close,volume\n");
        let mut bar_count = 0;

        for i in 0..timestamps.len() {
            let ts = timestamps[i].as_i64().unwrap_or(0);
            let open = opens.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let high = highs.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let low = lows.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let close = closes.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let volume = volumes.get(i).and_then(|v| v.as_u64()).unwrap_or(0);

            // Skip null bars
            if open == 0.0 && close == 0.0 {
                continue;
            }

            let dt = DateTime::from_timestamp(ts, 0)
                .unwrap_or_else(|| Utc::now());

            csv.push_str(&format!(
                "{},{:.2},{:.2},{:.2},{:.2},{}\n",
                dt.format("%Y-%m-%d %H:%M:%S"),
                open, high, low, close, volume
            ));
            bar_count += 1;
        }

        // Write to file
        let file_name = format!("{}_{}.csv", symbol.0, interval);
        let file_path = self.output_dir.join(&file_name);
        std::fs::write(&file_path, &csv)
            .context(format!("Failed to write CSV for {}", symbol.0))?;

        info!("Downloaded {} bars for {} → {}", bar_count, symbol.0, file_path.display());

        Ok(DownloadResult {
            symbol: symbol.clone(),
            bars_downloaded: bar_count,
            file_path: file_path.to_string_lossy().to_string(),
            status: DownloadStatus::Success,
        })
    }

    /// Load previously downloaded CSV data into OHLCV bars
    pub fn load_csv(&self, symbol: &Symbol, timeframe: &Timeframe) -> Result<Vec<OHLCV>> {
        let interval = match timeframe {
            Timeframe::D1 => "1d",
            Timeframe::W1 => "1wk",
            Timeframe::H1 => "1h",
            _ => "1d",
        };

        let file_name = format!("{}_{}.csv", symbol.0, interval);
        let file_path = self.output_dir.join(&file_name);

        let content = std::fs::read_to_string(&file_path)
            .context(format!("Failed to read CSV for {}", symbol.0))?;

        let mut bars = Vec::new();
        for (i, line) in content.lines().enumerate() {
            if i == 0 {
                continue; // Skip header
            }

            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 6 {
                continue;
            }

            let time = chrono::NaiveDateTime::parse_from_str(parts[0], "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|dt| dt.and_utc())
                .unwrap_or_else(Utc::now);

            bars.push(OHLCV {
                time,
                symbol: symbol.clone(),
                open: parts[1].parse().unwrap_or(0.0),
                high: parts[2].parse().unwrap_or(0.0),
                low: parts[3].parse().unwrap_or(0.0),
                close: parts[4].parse().unwrap_or(0.0),
                volume: parts[5].parse().unwrap_or(0),
            });
        }

        debug!("Loaded {} bars for {} from CSV", bars.len(), symbol.0);
        Ok(bars)
    }

    /// Health status
    pub fn health(&self) -> AdapterHealth {
        self.health.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_download_job_creation() {
        let job = DownloadJob {
            symbols: vec![Symbol("AAPL".to_string()), Symbol("GOOG".to_string())],
            from: Utc::now() - chrono::Duration::days(365),
            to: Utc::now(),
            timeframe: Timeframe::D1,
        };
        assert_eq!(job.symbols.len(), 2);
    }

    #[test]
    fn test_download_status_equality() {
        assert_eq!(DownloadStatus::Success, DownloadStatus::Success);
        assert_ne!(DownloadStatus::Success, DownloadStatus::Failed("err".to_string()));
    }

    #[test]
    fn test_download_output_serialization() {
        let output = DownloadOutput {
            total_symbols: 5,
            successful: 3,
            failed: 2,
            results: vec![
                DownloadResult {
                    symbol: Symbol("AAPL".to_string()),
                    bars_downloaded: 252,
                    file_path: "/tmp/AAPL_1d.csv".to_string(),
                    status: DownloadStatus::Success,
                },
            ],
        };
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: DownloadOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_symbols, 5);
    }

    #[test]
    fn test_load_csv() {
        let tmp_dir = std::env::temp_dir().join("apex_test_hist");
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let csv_content = "timestamp,open,high,low,close,volume\n\
                           2024-01-02 00:00:00,100.00,105.00,99.00,103.00,1000000\n\
                           2024-01-03 00:00:00,103.00,107.00,102.00,106.00,1200000\n\
                           2024-01-04 00:00:00,106.00,108.00,104.00,105.00,900000\n";

        let file_path = tmp_dir.join("TEST_1d.csv");
        std::fs::write(&file_path, csv_content).unwrap();

        let downloader = HistoricalDownloader::new(tmp_dir.clone()).unwrap();
        let bars = downloader.load_csv(&Symbol("TEST".to_string()), &Timeframe::D1).unwrap();

        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].open, 100.0);
        assert_eq!(bars[0].close, 103.0);
        assert_eq!(bars[1].volume, 1200000);
        assert_eq!(bars[2].symbol.0, "TEST");

        // Cleanup
        std::fs::remove_dir_all(&tmp_dir).ok();
    }

    #[test]
    fn test_downloader_creation() {
        let tmp_dir = std::env::temp_dir().join("apex_test_dl");
        let downloader = HistoricalDownloader::new(tmp_dir.clone());
        assert!(downloader.is_ok());
        assert_eq!(downloader.unwrap().health(), AdapterHealth::Healthy);
        std::fs::remove_dir_all(&tmp_dir).ok();
    }
}
