use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

use crate::domain::models::*;
use crate::ports::market_data::MarketDataPort;
use super::indicators;

/// Market scanner — scans symbols against user-defined criteria
///
/// Supports real-time screening across a universe of symbols using
/// price, volume, and indicator-based filters.
pub struct MarketScanner {
    market_data: Arc<dyn MarketDataPort>,
}

/// A single scan criterion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanCriterion {
    /// Price above a threshold
    PriceAbove(f64),
    /// Price below a threshold
    PriceBelow(f64),
    /// Price between a range
    PriceBetween(f64, f64),
    /// Volume above threshold
    VolumeAbove(u64),
    /// Change percentage above threshold
    ChangePctAbove(f64),
    /// Change percentage below threshold
    ChangePctBelow(f64),
    /// RSI above threshold (period, threshold)
    RsiAbove(usize, f64),
    /// RSI below threshold (period, threshold)
    RsiBelow(usize, f64),
    /// SMA crossover: price above SMA(period)
    AboveSma(usize),
    /// SMA crossunder: price below SMA(period)
    BelowSma(usize),
}

/// Configuration for a scan run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Name of this scan
    pub name: String,
    /// Symbols to scan
    pub universe: Vec<Symbol>,
    /// All criteria must match (AND logic)
    pub criteria: Vec<ScanCriterion>,
    /// Timeframe for indicator calculations
    pub timeframe: Timeframe,
    /// How many bars of history to fetch for indicator calculations
    pub lookback_bars: usize,
}

/// A single scan result (a matching symbol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub symbol: Symbol,
    pub last_price: f64,
    pub change_pct: f64,
    pub volume: u64,
    pub matched_at: DateTime<Utc>,
}

/// Full scan output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutput {
    pub config_name: String,
    pub scanned_count: usize,
    pub matched_count: usize,
    pub results: Vec<ScanResult>,
    pub completed_at: DateTime<Utc>,
}

impl MarketScanner {
    /// Create a new market scanner
    pub fn new(market_data: Arc<dyn MarketDataPort>) -> Self {
        Self { market_data }
    }

    /// Run a scan against all symbols in the universe
    pub async fn run_scan(&self, config: &ScanConfig) -> Result<ScanOutput> {
        info!("Starting scan '{}' across {} symbols", config.name, config.universe.len());

        let mut results = Vec::new();

        for symbol in &config.universe {
            match self.evaluate_symbol(symbol, config).await {
                Ok(Some(result)) => {
                    results.push(result);
                }
                Ok(None) => {
                    // Symbol did not match criteria
                }
                Err(e) => {
                    debug!("Scan error for {}: {}", symbol.0, e);
                }
            }
        }

        let output = ScanOutput {
            config_name: config.name.clone(),
            scanned_count: config.universe.len(),
            matched_count: results.len(),
            results,
            completed_at: Utc::now(),
        };

        info!(
            "Scan '{}' complete: {}/{} symbols matched",
            config.name, output.matched_count, output.scanned_count
        );

        Ok(output)
    }

    /// Evaluate a single symbol against all criteria
    async fn evaluate_symbol(
        &self,
        symbol: &Symbol,
        config: &ScanConfig,
    ) -> Result<Option<ScanResult>> {
        let quote = self.market_data.get_snapshot(symbol).await?;

        // Check simple quote-based criteria first (fast path)
        for criterion in &config.criteria {
            match criterion {
                ScanCriterion::PriceAbove(threshold) => {
                    if quote.last <= *threshold {
                        return Ok(None);
                    }
                }
                ScanCriterion::PriceBelow(threshold) => {
                    if quote.last >= *threshold {
                        return Ok(None);
                    }
                }
                ScanCriterion::PriceBetween(low, high) => {
                    if quote.last < *low || quote.last > *high {
                        return Ok(None);
                    }
                }
                ScanCriterion::VolumeAbove(threshold) => {
                    if quote.volume < *threshold {
                        return Ok(None);
                    }
                }
                ScanCriterion::ChangePctAbove(threshold) => {
                    if quote.change_pct <= *threshold {
                        return Ok(None);
                    }
                }
                ScanCriterion::ChangePctBelow(threshold) => {
                    if quote.change_pct >= *threshold {
                        return Ok(None);
                    }
                }
                // Indicator-based criteria need historical data — handled below
                _ => {}
            }
        }

        // Check if we need historical data for indicator criteria
        let needs_historical = config.criteria.iter().any(|c| matches!(
            c,
            ScanCriterion::RsiAbove(_, _)
            | ScanCriterion::RsiBelow(_, _)
            | ScanCriterion::AboveSma(_)
            | ScanCriterion::BelowSma(_)
        ));

        if needs_historical {
            let to = Utc::now();
            // Calculate lookback duration based on timeframe
            let lookback_days = match &config.timeframe {
                Timeframe::M1 | Timeframe::M3 | Timeframe::M5 => 1 + (config.lookback_bars / 78), // ~78 bars per day for M5
                Timeframe::M15 | Timeframe::M30 => 1 + (config.lookback_bars / 26), // ~26 bars per day for M15
                Timeframe::H1 | Timeframe::H4 => 1 + (config.lookback_bars / 7),  // ~7 bars per day for H1
                Timeframe::D1 => config.lookback_bars,
                Timeframe::W1 => config.lookback_bars * 7,
                _ => config.lookback_bars,
            };
            let from = to - chrono::Duration::days(lookback_days.max(1) as i64);
            let bars = self.market_data.get_historical_ohlcv(symbol, config.timeframe.clone(), from, to).await?;

            if bars.is_empty() {
                return Ok(None);
            }

            let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();

            for criterion in &config.criteria {
                match criterion {
                    ScanCriterion::RsiAbove(period, threshold) => {
                        let rsi_values = indicators::rsi(&closes, *period)?;
                        if let Some(&last_rsi) = rsi_values.last() {
                            if last_rsi <= *threshold {
                                return Ok(None);
                            }
                        } else {
                            return Ok(None);
                        }
                    }
                    ScanCriterion::RsiBelow(period, threshold) => {
                        let rsi_values = indicators::rsi(&closes, *period)?;
                        if let Some(&last_rsi) = rsi_values.last() {
                            if last_rsi >= *threshold {
                                return Ok(None);
                            }
                        } else {
                            return Ok(None);
                        }
                    }
                    ScanCriterion::AboveSma(period) => {
                        let sma_values = indicators::sma(&closes, *period)?;
                        if let Some(&last_sma) = sma_values.last() {
                            if quote.last <= last_sma {
                                return Ok(None);
                            }
                        } else {
                            return Ok(None);
                        }
                    }
                    ScanCriterion::BelowSma(period) => {
                        let sma_values = indicators::sma(&closes, *period)?;
                        if let Some(&last_sma) = sma_values.last() {
                            if quote.last >= last_sma {
                                return Ok(None);
                            }
                        } else {
                            return Ok(None);
                        }
                    }
                    _ => {} // Already handled above
                }
            }
        }

        // All criteria passed
        Ok(Some(ScanResult {
            symbol: symbol.clone(),
            last_price: quote.last,
            change_pct: quote.change_pct,
            volume: quote.volume,
            matched_at: Utc::now(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_criterion_serialization() {
        let criterion = ScanCriterion::PriceAbove(100.0);
        let json = serde_json::to_string(&criterion).unwrap();
        let deserialized: ScanCriterion = serde_json::from_str(&json).unwrap();
        match deserialized {
            ScanCriterion::PriceAbove(v) => assert_eq!(v, 100.0),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_scan_config_creation() {
        let config = ScanConfig {
            name: "test_scan".to_string(),
            universe: vec![Symbol("AAPL".to_string()), Symbol("GOOG".to_string())],
            criteria: vec![
                ScanCriterion::PriceAbove(50.0),
                ScanCriterion::VolumeAbove(1_000_000),
            ],
            timeframe: Timeframe::D1,
            lookback_bars: 50,
        };
        assert_eq!(config.universe.len(), 2);
        assert_eq!(config.criteria.len(), 2);
    }

    #[test]
    fn test_scan_output_serialization() {
        let output = ScanOutput {
            config_name: "test".to_string(),
            scanned_count: 10,
            matched_count: 2,
            results: vec![
                ScanResult {
                    symbol: Symbol("AAPL".to_string()),
                    last_price: 150.0,
                    change_pct: 2.5,
                    volume: 5_000_000,
                    matched_at: Utc::now(),
                },
            ],
            completed_at: Utc::now(),
        };
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: ScanOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.matched_count, 2);
        assert_eq!(deserialized.results.len(), 1);
    }

    #[test]
    fn test_criterion_variants() {
        let criteria = vec![
            ScanCriterion::PriceAbove(100.0),
            ScanCriterion::PriceBelow(200.0),
            ScanCriterion::PriceBetween(100.0, 200.0),
            ScanCriterion::VolumeAbove(1_000_000),
            ScanCriterion::ChangePctAbove(2.0),
            ScanCriterion::ChangePctBelow(-2.0),
            ScanCriterion::RsiAbove(14, 70.0),
            ScanCriterion::RsiBelow(14, 30.0),
            ScanCriterion::AboveSma(20),
            ScanCriterion::BelowSma(50),
        ];
        assert_eq!(criteria.len(), 10);
    }
}
