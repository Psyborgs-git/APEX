use super::data::load_bars;
use crate::dto::{
    IndicatorResultDto, NamedSeriesDto, QuantStatsDto, RegressionDto, SeriesPointDto,
};
use crate::state::AppState;
use apex_core::application::{indicators, quant};
use apex_core::domain::models::OHLCV;
use serde_json::Value as Json;
use std::collections::HashMap;
use tauri::State;

const MAX_BARS: usize = 1500;

fn periods_per_year(bars: &[OHLCV]) -> f64 {
    // Infer bar cadence from median spacing
    if bars.len() < 3 {
        return 252.0;
    }
    let mut gaps: Vec<i64> = bars
        .windows(2)
        .map(|w| (w[1].time - w[0].time).num_seconds())
        .filter(|g| *g > 0)
        .collect();
    gaps.sort();
    let median = gaps[gaps.len() / 2];
    match median {
        g if g <= 90 => 252.0 * 390.0 / 60.0, // ~per-minute
        g if g <= 300 => 252.0 * 78.0,
        g if g <= 900 => 252.0 * 26.0,
        g if g <= 1800 => 252.0 * 13.0,
        g if g <= 3600 => 252.0 * 6.5,
        g if g <= 14400 => 252.0 * 1.625,
        g if g <= 86400 => 252.0,
        _ => 52.0,
    }
}

fn times(bars: &[OHLCV]) -> Vec<String> {
    bars.iter().map(|b| b.time.to_rfc3339()).collect()
}

/// Zip a trailing indicator output onto the tail of the bar times
/// (every indicator in `indicators.rs` emits values aligned to the last
/// bar of its rolling window).
fn align_points(bar_times: &[String], values: Vec<f64>) -> Vec<SeriesPointDto> {
    let offset = bar_times.len().saturating_sub(values.len());
    values
        .into_iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite())
        .map(|(i, v)| SeriesPointDto {
            time: bar_times[offset + i].clone(),
            value: v,
        })
        .collect()
}

fn num_param(params: &Option<Json>, key: &str, default: f64) -> f64 {
    params
        .as_ref()
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_f64())
        .unwrap_or(default)
}

fn usize_param(params: &Option<Json>, key: &str, default: usize) -> usize {
    num_param(params, key, default as f64).max(1.0) as usize
}

/// Compute a technical indicator over stored/fetched OHLCV bars.
/// OpenBB `technical.*` router surface: sma, ema, rsi, macd, bbands, atr,
/// vwap, stoch, stddev, roc. Params come in as a JSON object (OpenBB kwargs style).
#[tauri::command]
pub async fn compute_indicator(
    symbol: String,
    indicator: String,
    timeframe: Option<String>,
    params: Option<Json>,
    state: State<'_, AppState>,
) -> Result<IndicatorResultDto, String> {
    let bars = load_bars(&state, &symbol, timeframe.as_deref(), MAX_BARS).await?;
    if bars.len() < 3 {
        return Err(format!("Not enough bars for {} (got {})", symbol, bars.len()));
    }
    let ts = times(&bars);
    let close: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let high: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let low: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let vol: Vec<u64> = bars.iter().map(|b| b.volume).collect();

    let ind = indicator.to_lowercase();
    let map_err = |e: anyhow::Error| e.to_string();

    let (overlay, series): (bool, Vec<(String, Vec<f64>)>) = match ind.as_str() {
        "sma" => (
            true,
            vec![("sma".into(), indicators::sma(&close, usize_param(&params, "period", 20)).map_err(map_err)?)],
        ),
        "ema" => (
            true,
            vec![("ema".into(), indicators::ema(&close, usize_param(&params, "period", 20)).map_err(map_err)?)],
        ),
        "bbands" | "bb" => {
            let r = indicators::bollinger_bands(
                &close,
                usize_param(&params, "period", 20),
                num_param(&params, "std", 2.0),
            )
            .map_err(map_err)?;
            (true, vec![("upper".into(), r.upper), ("middle".into(), r.middle), ("lower".into(), r.lower)])
        }
        "vwap" => (true, vec![("vwap".into(), indicators::vwap(&high, &low, &close, &vol).map_err(map_err)?)]),
        "rsi" => (false, vec![("rsi".into(), indicators::rsi(&close, usize_param(&params, "period", 14)).map_err(map_err)?)]),
        "macd" => {
            let r = indicators::macd(
                &close,
                usize_param(&params, "fast", 12),
                usize_param(&params, "slow", 26),
                usize_param(&params, "signal", 9),
            )
            .map_err(map_err)?;
            (false, vec![("macd".into(), r.macd_line), ("signal".into(), r.signal_line), ("histogram".into(), r.histogram)])
        }
        "stoch" | "stochastic" => {
            let r = indicators::stochastic(
                &high,
                &low,
                &close,
                usize_param(&params, "k", 14),
                usize_param(&params, "d", 3),
            )
            .map_err(map_err)?;
            (false, vec![("k".into(), r.k), ("d".into(), r.d)])
        }
        "atr" => (false, vec![("atr".into(), indicators::atr(&high, &low, &close, usize_param(&params, "period", 14)).map_err(map_err)?)]),
        "stddev" | "stdev" => (false, vec![("stddev".into(), indicators::std_dev(&close, usize_param(&params, "period", 20)).map_err(map_err)?)]),
        "roc" => (false, vec![("roc".into(), indicators::roc(&close, usize_param(&params, "period", 10)).map_err(map_err)?)]),
        other => {
            return Err(format!(
                "Unknown indicator '{}'. Supported: sma, ema, bbands, vwap, rsi, macd, stoch, atr, stddev, roc",
                other
            ))
        }
    };

    Ok(IndicatorResultDto {
        symbol,
        indicator: ind,
        overlay,
        series: series
            .into_iter()
            .map(|(name, vals)| NamedSeriesDto {
                name,
                points: align_points(&ts, vals),
            })
            .collect(),
    })
}

/// Quant stats over a symbol's return series — OpenBB `quantitative.summary`
/// + `performance.*` (sharpe/sortino/omega/drawdown) + rolling stats.
#[tauri::command]
pub async fn get_quant_stats(
    symbol: String,
    timeframe: Option<String>,
    window: Option<usize>,
    state: State<'_, AppState>,
) -> Result<QuantStatsDto, String> {
    let bars = load_bars(&state, &symbol, timeframe.as_deref(), MAX_BARS).await?;
    if bars.len() < 5 {
        return Err(format!("Not enough bars for {} (got {})", symbol, bars.len()));
    }
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let rets = quant::returns(&closes);
    let ppy = periods_per_year(&bars);
    let s = quant::summarize(&closes, ppy);

    let win = window.unwrap_or(21).clamp(5, 120);
    let ts = times(&bars);
    // returns are aligned to closes[1..] → bar_times[1..]
    let ret_times: Vec<String> = ts[1..].to_vec();
    let rv = quant::rolling(&rets, win, |x| quant::std_dev(x) * ppy.sqrt());
    let rs = quant::rolling(&rets, win, |x| quant::sharpe_ratio(x, 0.0, ppy));

    Ok(QuantStatsDto {
        symbol,
        n: s.n,
        mean: s.mean,
        std_dev: s.std_dev,
        variance: s.variance,
        skewness: s.skewness,
        kurtosis: s.kurtosis,
        min: s.min,
        q05: s.q05,
        q25: s.q25,
        median: s.median,
        q75: s.q75,
        q95: s.q95,
        max: s.max,
        jarque_bera: s.jarque_bera,
        normal: s.normal,
        sharpe: s.sharpe,
        sortino: s.sortino,
        omega: if s.omega.is_finite() { s.omega } else { 999.99 },
        max_drawdown: s.max_drawdown,
        ann_volatility: s.ann_volatility,
        autocorr: s.autocorr,
        rolling_vol: align_points(&ret_times, rv),
        rolling_sharpe: align_points(&ret_times, rs),
    })
}

/// OLS regression of y's returns on x's returns over aligned timestamps —
/// OpenBB `econometrics.ols_regression`. Beta ≈ CAPM beta when x is an index.
#[tauri::command]
pub async fn get_regression(
    x_symbol: String,
    y_symbol: String,
    timeframe: Option<String>,
    state: State<'_, AppState>,
) -> Result<RegressionDto, String> {
    let xbars = load_bars(&state, &x_symbol, timeframe.as_deref(), MAX_BARS).await?;
    let ybars = load_bars(&state, &y_symbol, timeframe.as_deref(), MAX_BARS).await?;
    if xbars.len() < 10 || ybars.len() < 10 {
        return Err("Not enough overlapping bars for regression".to_string());
    }

    // Align on shared bar timestamps. For daily/weekly bars align on the
    // calendar date — exchanges in different timezones stamp the same trading
    // day at different instants (US 13:30Z vs NSE 03:45Z).
    let tf_str = timeframe.as_deref().unwrap_or("d1").to_lowercase();
    let daily = matches!(tf_str.as_str(), "d1" | "1d" | "w1" | "1w");
    let key_of = |b: &OHLCV| {
        if daily {
            b.time.format("%Y-%m-%d").to_string()
        } else {
            b.time.to_rfc3339()
        }
    };
    let xclose: HashMap<String, f64> = xbars.iter().map(|b| (key_of(b), b.close)).collect();
    let mut aligned: Vec<(String, f64, f64)> = ybars
        .iter()
        .filter_map(|b| xclose.get(&key_of(b)).map(|&x| (b.time.to_rfc3339(), x, b.close)))
        .collect();
    aligned.sort_by(|a, b| a.0.cmp(&b.0));

    let xs_price: Vec<f64> = aligned.iter().map(|a| a.1).collect();
    let ys_price: Vec<f64> = aligned.iter().map(|a| a.2).collect();
    let xr = quant::returns(&xs_price);
    let yr = quant::returns(&ys_price);

    let fit = quant::ols(&yr, &xr).ok_or("Regression failed: insufficient variance")?;
    let resid_times: Vec<String> = aligned.iter().skip(1).map(|a| a.0.clone()).collect();

    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let scatter: Vec<[f64; 2]> = xr
        .iter()
        .zip(yr.iter())
        .map(|(&x, &y)| {
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            [x, y]
        })
        .collect();

    Ok(RegressionDto {
        x_symbol,
        y_symbol,
        n: fit.n,
        alpha: fit.alpha,
        beta: fit.beta,
        r_squared: fit.r_squared,
        residuals: fit
            .residuals
            .into_iter()
            .enumerate()
            .map(|(i, v)| SeriesPointDto {
                time: resid_times.get(i).cloned().unwrap_or_default(),
                value: v,
            })
            .collect(),
        scatter,
        fit_line: vec![
            [x_min, fit.alpha + fit.beta * x_min],
            [x_max, fit.alpha + fit.beta * x_max],
        ],
    })
}
