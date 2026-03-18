use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Technical indicator library
///
/// Provides common technical indicators for strategy development and backtesting.
/// All indicators are pure functions operating on price/volume slices, with no
/// external dependencies (TA-Lib wrapper stubs are provided for future integration).

/// Simple Moving Average (SMA)
///
/// Calculates the arithmetic mean of the last `period` values.
pub fn sma(data: &[f64], period: usize) -> Result<Vec<f64>> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() < period {
        return Ok(vec![]);
    }

    let mut result = Vec::with_capacity(data.len() - period + 1);
    let mut sum: f64 = data[..period].iter().sum();
    result.push(sum / period as f64);

    for i in period..data.len() {
        sum += data[i] - data[i - period];
        result.push(sum / period as f64);
    }

    Ok(result)
}

/// Exponential Moving Average (EMA)
///
/// Uses a smoothing factor of 2 / (period + 1).
pub fn ema(data: &[f64], period: usize) -> Result<Vec<f64>> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() < period {
        return Ok(vec![]);
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut result = Vec::with_capacity(data.len() - period + 1);

    // Seed with SMA of first `period` values
    let seed: f64 = data[..period].iter().sum::<f64>() / period as f64;
    result.push(seed);

    for i in period..data.len() {
        let prev = *result.last().unwrap();
        let value = (data[i] - prev) * multiplier + prev;
        result.push(value);
    }

    Ok(result)
}

/// Relative Strength Index (RSI)
///
/// Measures the magnitude of recent price changes to evaluate overbought/oversold conditions.
/// Standard period is 14.
pub fn rsi(data: &[f64], period: usize) -> Result<Vec<f64>> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() < period + 1 {
        return Ok(vec![]);
    }

    let mut gains = Vec::with_capacity(data.len() - 1);
    let mut losses = Vec::with_capacity(data.len() - 1);

    for i in 1..data.len() {
        let change = data[i] - data[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(-change);
        }
    }

    // First average gain/loss
    let mut avg_gain: f64 = gains[..period].iter().sum::<f64>() / period as f64;
    let mut avg_loss: f64 = losses[..period].iter().sum::<f64>() / period as f64;

    let mut result = Vec::with_capacity(data.len() - period);

    let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
    result.push(100.0 - (100.0 / (1.0 + rs)));

    // Smoothed moving average for subsequent values
    for i in period..gains.len() {
        avg_gain = (avg_gain * (period as f64 - 1.0) + gains[i]) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + losses[i]) / period as f64;
        let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
        result.push(100.0 - (100.0 / (1.0 + rs)));
    }

    Ok(result)
}

/// MACD (Moving Average Convergence Divergence)
///
/// Returns (macd_line, signal_line, histogram) tuples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MACDResult {
    pub macd_line: Vec<f64>,
    pub signal_line: Vec<f64>,
    pub histogram: Vec<f64>,
}

pub fn macd(data: &[f64], fast_period: usize, slow_period: usize, signal_period: usize) -> Result<MACDResult> {
    if fast_period >= slow_period {
        return Err(anyhow!("Fast period must be less than slow period"));
    }

    let fast_ema = ema(data, fast_period)?;
    let slow_ema = ema(data, slow_period)?;

    if slow_ema.is_empty() {
        return Ok(MACDResult {
            macd_line: vec![],
            signal_line: vec![],
            histogram: vec![],
        });
    }

    // Align fast and slow EMAs (slow starts later)
    let offset = fast_ema.len() - slow_ema.len();
    let macd_line: Vec<f64> = fast_ema[offset..]
        .iter()
        .zip(slow_ema.iter())
        .map(|(f, s)| f - s)
        .collect();

    let signal_line = ema(&macd_line, signal_period)?;

    if signal_line.is_empty() {
        return Ok(MACDResult {
            macd_line,
            signal_line: vec![],
            histogram: vec![],
        });
    }

    let hist_offset = macd_line.len() - signal_line.len();
    let histogram: Vec<f64> = macd_line[hist_offset..]
        .iter()
        .zip(signal_line.iter())
        .map(|(m, s)| m - s)
        .collect();

    Ok(MACDResult {
        macd_line,
        signal_line,
        histogram,
    })
}

/// Bollinger Bands
///
/// Returns (upper, middle, lower) bands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerBandsResult {
    pub upper: Vec<f64>,
    pub middle: Vec<f64>,
    pub lower: Vec<f64>,
}

pub fn bollinger_bands(data: &[f64], period: usize, num_std: f64) -> Result<BollingerBandsResult> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() < period {
        return Ok(BollingerBandsResult {
            upper: vec![],
            middle: vec![],
            lower: vec![],
        });
    }

    let middle = sma(data, period)?;
    let mut upper = Vec::with_capacity(middle.len());
    let mut lower = Vec::with_capacity(middle.len());

    for (i, &mid) in middle.iter().enumerate() {
        let start = i;
        let end = i + period;
        let slice = &data[start..end];
        let mean = mid;
        let variance: f64 = slice.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();

        upper.push(mid + num_std * std_dev);
        lower.push(mid - num_std * std_dev);
    }

    Ok(BollingerBandsResult { upper, middle, lower })
}

/// Average True Range (ATR)
///
/// Measures market volatility. Requires high, low, close arrays.
pub fn atr(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Result<Vec<f64>> {
    if high.len() != low.len() || high.len() != close.len() {
        return Err(anyhow!("Input arrays must have same length"));
    }
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if high.len() < period + 1 {
        return Ok(vec![]);
    }

    // Calculate True Range
    let mut tr = Vec::with_capacity(high.len() - 1);
    for i in 1..high.len() {
        let hl = high[i] - low[i];
        let hc = (high[i] - close[i - 1]).abs();
        let lc = (low[i] - close[i - 1]).abs();
        tr.push(hl.max(hc).max(lc));
    }

    // First ATR is simple average
    let mut atr_values = Vec::with_capacity(tr.len() - period + 1);
    let first_atr: f64 = tr[..period].iter().sum::<f64>() / period as f64;
    atr_values.push(first_atr);

    // Smoothed for subsequent values
    for i in period..tr.len() {
        let prev = *atr_values.last().unwrap();
        let value = (prev * (period as f64 - 1.0) + tr[i]) / period as f64;
        atr_values.push(value);
    }

    Ok(atr_values)
}

/// Volume Weighted Average Price (VWAP)
///
/// Cumulative VWAP from start of data.
pub fn vwap(high: &[f64], low: &[f64], close: &[f64], volume: &[u64]) -> Result<Vec<f64>> {
    if high.len() != low.len() || high.len() != close.len() || high.len() != volume.len() {
        return Err(anyhow!("Input arrays must have same length"));
    }
    if high.is_empty() {
        return Ok(vec![]);
    }

    let mut cum_tp_vol = 0.0;
    let mut cum_vol = 0.0;
    let mut result = Vec::with_capacity(high.len());

    for i in 0..high.len() {
        let typical_price = (high[i] + low[i] + close[i]) / 3.0;
        cum_tp_vol += typical_price * volume[i] as f64;
        cum_vol += volume[i] as f64;

        if cum_vol > 0.0 {
            result.push(cum_tp_vol / cum_vol);
        } else {
            result.push(0.0);
        }
    }

    Ok(result)
}

/// Stochastic Oscillator (%K and %D)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StochasticResult {
    pub k: Vec<f64>,
    pub d: Vec<f64>,
}

pub fn stochastic(high: &[f64], low: &[f64], close: &[f64], k_period: usize, d_period: usize) -> Result<StochasticResult> {
    if high.len() != low.len() || high.len() != close.len() {
        return Err(anyhow!("Input arrays must have same length"));
    }
    if k_period == 0 || d_period == 0 {
        return Err(anyhow!("Periods must be > 0"));
    }
    if high.len() < k_period {
        return Ok(StochasticResult { k: vec![], d: vec![] });
    }

    let mut k_values = Vec::with_capacity(high.len() - k_period + 1);

    for i in (k_period - 1)..high.len() {
        let start = i + 1 - k_period;
        let highest = high[start..=i].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let lowest = low[start..=i].iter().cloned().fold(f64::INFINITY, f64::min);

        let k = if (highest - lowest).abs() < f64::EPSILON {
            50.0
        } else {
            ((close[i] - lowest) / (highest - lowest)) * 100.0
        };
        k_values.push(k);
    }

    let d_values = sma(&k_values, d_period)?;

    Ok(StochasticResult {
        k: k_values,
        d: d_values,
    })
}

/// Standard Deviation
pub fn std_dev(data: &[f64], period: usize) -> Result<Vec<f64>> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() < period {
        return Ok(vec![]);
    }

    let means = sma(data, period)?;
    let mut result = Vec::with_capacity(means.len());

    for (i, &mean) in means.iter().enumerate() {
        let start = i;
        let end = i + period;
        let variance: f64 = data[start..end].iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
        result.push(variance.sqrt());
    }

    Ok(result)
}

/// Rate of Change (ROC)
pub fn roc(data: &[f64], period: usize) -> Result<Vec<f64>> {
    if period == 0 {
        return Err(anyhow!("Period must be > 0"));
    }
    if data.len() <= period {
        return Ok(vec![]);
    }

    let mut result = Vec::with_capacity(data.len() - period);
    for i in period..data.len() {
        if data[i - period] != 0.0 {
            result.push(((data[i] - data[i - period]) / data[i - period]) * 100.0);
        } else {
            result.push(0.0);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRICES: [f64; 20] = [
        44.0, 44.3, 44.1, 43.6, 44.3, 44.8, 45.1, 43.7, 44.1, 44.6,
        45.0, 45.2, 44.8, 44.3, 44.0, 43.5, 43.9, 44.2, 44.5, 44.7,
    ];

    #[test]
    fn test_sma_basic() {
        let result = sma(&PRICES, 5).unwrap();
        assert_eq!(result.len(), 16); // 20 - 5 + 1
        // First SMA(5) = (44.0 + 44.3 + 44.1 + 43.6 + 44.3) / 5 = 44.06
        assert!((result[0] - 44.06).abs() < 0.01);
    }

    #[test]
    fn test_sma_period_zero() {
        assert!(sma(&PRICES, 0).is_err());
    }

    #[test]
    fn test_sma_insufficient_data() {
        let result = sma(&[1.0, 2.0], 5).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_ema_basic() {
        let result = ema(&PRICES, 5).unwrap();
        assert_eq!(result.len(), 16);
        // First EMA value = SMA(5) = 44.06
        assert!((result[0] - 44.06).abs() < 0.01);
    }

    #[test]
    fn test_rsi_basic() {
        let result = rsi(&PRICES, 14).unwrap();
        assert!(!result.is_empty());
        // RSI should be between 0 and 100
        for &v in &result {
            assert!(v >= 0.0 && v <= 100.0);
        }
    }

    #[test]
    fn test_rsi_period_zero() {
        assert!(rsi(&PRICES, 0).is_err());
    }

    #[test]
    fn test_macd_basic() {
        let data: Vec<f64> = (0..50).map(|i| 100.0 + (i as f64 * 0.5).sin() * 10.0).collect();
        let result = macd(&data, 12, 26, 9).unwrap();
        assert!(!result.macd_line.is_empty());
        assert!(!result.signal_line.is_empty());
        assert!(!result.histogram.is_empty());
    }

    #[test]
    fn test_macd_invalid_periods() {
        assert!(macd(&PRICES, 26, 12, 9).is_err()); // fast >= slow
    }

    #[test]
    fn test_bollinger_bands_basic() {
        let result = bollinger_bands(&PRICES, 5, 2.0).unwrap();
        assert_eq!(result.upper.len(), result.middle.len());
        assert_eq!(result.middle.len(), result.lower.len());
        // Upper > middle > lower
        for i in 0..result.middle.len() {
            assert!(result.upper[i] >= result.middle[i]);
            assert!(result.middle[i] >= result.lower[i]);
        }
    }

    #[test]
    fn test_atr_basic() {
        let high = vec![45.0, 45.5, 45.2, 44.8, 45.3, 45.6, 45.8, 44.9, 45.1, 45.5];
        let low = vec![43.5, 44.0, 43.8, 43.2, 43.9, 44.3, 44.6, 43.2, 43.6, 44.1];
        let close = vec![44.0, 44.3, 44.1, 43.6, 44.3, 44.8, 45.1, 43.7, 44.1, 44.6];
        let result = atr(&high, &low, &close, 5).unwrap();
        assert!(!result.is_empty());
        for &v in &result {
            assert!(v > 0.0);
        }
    }

    #[test]
    fn test_vwap_basic() {
        let high = vec![45.0, 45.5, 45.2];
        let low = vec![43.5, 44.0, 43.8];
        let close = vec![44.0, 44.3, 44.1];
        let volume = vec![1000u64, 1500, 1200];
        let result = vwap(&high, &low, &close, &volume).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0] > 0.0);
    }

    #[test]
    fn test_stochastic_basic() {
        let high = vec![45.0, 45.5, 45.2, 44.8, 45.3, 45.6, 45.8, 44.9, 45.1, 45.5];
        let low = vec![43.5, 44.0, 43.8, 43.2, 43.9, 44.3, 44.6, 43.2, 43.6, 44.1];
        let close = vec![44.0, 44.3, 44.1, 43.6, 44.3, 44.8, 45.1, 43.7, 44.1, 44.6];
        let result = stochastic(&high, &low, &close, 5, 3).unwrap();
        assert!(!result.k.is_empty());
        // %K should be between 0 and 100
        for &v in &result.k {
            assert!(v >= 0.0 && v <= 100.0);
        }
    }

    #[test]
    fn test_std_dev_basic() {
        let result = std_dev(&PRICES, 5).unwrap();
        assert!(!result.is_empty());
        for &v in &result {
            assert!(v >= 0.0);
        }
    }

    #[test]
    fn test_roc_basic() {
        let result = roc(&PRICES, 5).unwrap();
        assert_eq!(result.len(), 15); // 20 - 5
    }

    #[test]
    fn test_roc_period_zero() {
        assert!(roc(&PRICES, 0).is_err());
    }
}
