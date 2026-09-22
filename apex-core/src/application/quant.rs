//! Quantitative analytics — port of the OpenBB `quantitative` extension's
//! stats / performance / econometrics surface onto pure-Rust math.
//!
//! All functions take plain slices so they can run over stored OHLCV closes
//! or any return series.

/// Simple periodic returns of a price series (len = input len - 1).
pub fn returns(prices: &[f64]) -> Vec<f64> {
    prices
        .windows(2)
        .filter(|w| w[0] > 0.0)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect()
}

pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Sample variance (n-1 denominator), matching OpenBB/statsmodels `var`.
pub fn variance(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0)
}

pub fn std_dev(xs: &[f64]) -> f64 {
    variance(xs).sqrt()
}

/// Sample skewness (Fisher-Pearson, bias-corrected), OpenBB `stats.skew`.
pub fn skewness(xs: &[f64]) -> f64 {
    let n = xs.len();
    if n < 3 {
        return 0.0;
    }
    let m = mean(xs);
    let s = std_dev(xs);
    if s == 0.0 {
        return 0.0;
    }
    let m3 = xs.iter().map(|x| (x - m).powi(3)).sum::<f64>() / n as f64;
    let g1 = m3 / s.powi(3);
    // bias correction for sample estimate
    (n as f64 * (n as f64 - 1.0)).sqrt() / (n as f64 - 2.0) * g1
}

/// Sample excess kurtosis (bias-corrected), OpenBB `stats.kurtosis`.
pub fn kurtosis(xs: &[f64]) -> f64 {
    let n = xs.len();
    if n < 4 {
        return 0.0;
    }
    let m = mean(xs);
    let s = std_dev(xs);
    if s == 0.0 {
        return 0.0;
    }
    let m4 = xs.iter().map(|x| (x - m).powi(4)).sum::<f64>() / n as f64;
    let g2 = m4 / s.powi(4) - 3.0;
    let nf = n as f64;
    ((nf - 1.0) / ((nf - 2.0) * (nf - 3.0))) * ((nf + 1.0) * g2 + 6.0)
}

/// Quantile with linear interpolation, OpenBB `stats.quantile` (R-7 / numpy default).
pub fn quantile(xs: &[f64], q: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = (sorted.len() as f64 - 1.0) * q.clamp(0.0, 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
    }
}

/// Jarque-Bera normality statistic, OpenBB `quantitative.normality` (without p-value lookup).
/// Returns (JB, normal) where `normal` uses the 5% critical value 5.99.
pub fn jarque_bera(xs: &[f64]) -> (f64, bool) {
    let n = xs.len();
    if n < 7 {
        return (0.0, false);
    }
    let s = skewness(xs);
    let k = kurtosis(xs);
    let jb = n as f64 / 6.0 * (s * s + k * k / 4.0);
    (jb, jb <= 5.99)
}

/// Sharpe ratio over a return series, annualized via `periods_per_year`.
/// OpenBB `performance.sharpe_ratio` = mean(xs - rf) / stdev(xs) * sqrt(periods).
pub fn sharpe_ratio(xs: &[f64], risk_free_per_period: f64, periods_per_year: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let excess: Vec<f64> = xs.iter().map(|x| x - risk_free_per_period).collect();
    let sd = std_dev(&excess);
    if sd == 0.0 {
        return 0.0;
    }
    mean(&excess) / sd * periods_per_year.sqrt()
}

/// Sortino ratio — downside deviation, OpenBB `performance.sortino_ratio`.
pub fn sortino_ratio(xs: &[f64], risk_free_per_period: f64, periods_per_year: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let excess: Vec<f64> = xs.iter().map(|x| x - risk_free_per_period).collect();
    let downside: Vec<f64> = xs
        .iter()
        .map(|x| (x - risk_free_per_period).min(0.0))
        .collect();
    let dd = std_dev(&downside);
    if dd == 0.0 {
        return 0.0;
    }
    mean(&excess) / dd * periods_per_year.sqrt()
}

/// Omega ratio — gains over losses relative to a threshold, OpenBB `performance.omega_ratio`.
pub fn omega_ratio(xs: &[f64], threshold: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let gains: f64 = xs.iter().map(|x| (x - threshold).max(0.0)).sum();
    let losses: f64 = xs.iter().map(|x| (threshold - x).max(0.0)).sum();
    if losses == 0.0 {
        return if gains > 0.0 { f64::INFINITY } else { 0.0 };
    }
    gains / losses
}

/// Max drawdown of an equity curve (price series), as a negative fraction.
pub fn max_drawdown(prices: &[f64]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd: f64 = 0.0;
    for &p in prices {
        if p > peak {
            peak = p;
        }
        if peak > 0.0 {
            max_dd = max_dd.min((p - peak) / peak);
        }
    }
    max_dd
}

/// Autocorrelation function — correlations at lags 1..=max_lag.
/// OpenBB `econometrics.autocorrelation`.
pub fn autocorrelation(xs: &[f64], max_lag: usize) -> Vec<f64> {
    let n = xs.len();
    let m = mean(xs);
    let denom: f64 = xs.iter().map(|x| (x - m).powi(2)).sum();
    if denom == 0.0 || n < 4 {
        return vec![0.0; max_lag];
    }
    (1..=max_lag)
        .map(|lag| {
            if n <= lag + 2 {
                return 0.0;
            }
            let num: f64 = (0..n - lag).map(|i| (xs[i] - m) * (xs[i + lag] - m)).sum();
            num / denom
        })
        .collect()
}

/// Rolling window stat — maps `window`-sized slices through `f`.
/// OpenBB `rolling.*` semantics.
pub fn rolling(xs: &[f64], window: usize, f: impl Fn(&[f64]) -> f64) -> Vec<f64> {
    if xs.len() < window || window == 0 {
        return Vec::new();
    }
    (window..=xs.len()).map(|e| f(&xs[e - window..e])).collect()
}

#[derive(Debug, Clone)]
pub struct OlsResult {
    pub alpha: f64,
    pub beta: f64,
    pub r_squared: f64,
    pub residuals: Vec<f64>,
    pub n: usize,
}

/// Ordinary least squares y = alpha + beta*x — OpenBB `econometrics.ols_regression`.
/// Pairs are aligned on equal-length slices of the tail (caller aligns timestamps).
pub fn ols(y: &[f64], x: &[f64]) -> Option<OlsResult> {
    let n = y.len().min(x.len());
    if n < 3 {
        return None;
    }
    let (y, x) = (&y[y.len() - n..], &x[x.len() - n..]);
    let mx = mean(x);
    let my = mean(y);
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let sxy: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(a, b)| (a - mx) * (b - my))
        .sum();
    let beta = sxy / sxx;
    let alpha = my - beta * mx;

    let mut sse = 0.0;
    let mut sst = 0.0;
    let mut residuals = Vec::with_capacity(n);
    for i in 0..n {
        let resid = y[i] - (alpha + beta * x[i]);
        residuals.push(resid);
        sse += resid * resid;
        sst += (y[i] - my).powi(2);
    }
    let r_squared = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };

    Some(OlsResult {
        alpha,
        beta,
        r_squared,
        residuals,
        n,
    })
}

/// Summary DTO matching OpenBB `quantitative.summary` fields.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuantSummary {
    pub n: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub variance: f64,
    pub skewness: f64,
    pub kurtosis: f64,
    pub min: f64,
    pub q05: f64,
    pub q25: f64,
    pub median: f64,
    pub q75: f64,
    pub q95: f64,
    pub max: f64,
    pub jarque_bera: f64,
    pub normal: bool,
    pub sharpe: f64,
    pub sortino: f64,
    pub omega: f64,
    pub max_drawdown: f64,
    pub ann_volatility: f64,
    pub autocorr: Vec<f64>,
}

/// Build the full stats summary over a close-price series.
/// `periods_per_year` annualizes sharpe/sortino/vol (252 for daily bars).
pub fn summarize(prices: &[f64], periods_per_year: f64) -> QuantSummary {
    let rets = returns(prices);
    let (jb, normal) = jarque_bera(&rets);
    QuantSummary {
        n: rets.len(),
        mean: mean(&rets),
        std_dev: std_dev(&rets),
        variance: variance(&rets),
        skewness: skewness(&rets),
        kurtosis: kurtosis(&rets),
        min: rets.iter().cloned().fold(f64::INFINITY, f64::min),
        q05: quantile(&rets, 0.05),
        q25: quantile(&rets, 0.25),
        median: quantile(&rets, 0.5),
        q75: quantile(&rets, 0.75),
        q95: quantile(&rets, 0.95),
        max: rets.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        jarque_bera: jb,
        normal,
        sharpe: sharpe_ratio(&rets, 0.0, periods_per_year),
        sortino: sortino_ratio(&rets, 0.0, periods_per_year),
        omega: omega_ratio(&rets, 0.0),
        max_drawdown: max_drawdown(prices),
        ann_volatility: std_dev(&rets) * periods_per_year.sqrt(),
        autocorr: autocorrelation(&rets, 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_and_stats() {
        let prices = vec![100.0, 101.0, 99.0, 102.0, 100.0];
        let r = returns(&prices);
        assert_eq!(r.len(), 4);
        assert!((r[0] - 0.01).abs() < 1e-9);
        assert!((r[1] + 0.01980198019802).abs() < 1e-9);
    }

    #[test]
    fn moments() {
        let xs: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert!((mean(&xs) - 50.5).abs() < 1e-9);
        assert!((variance(&xs) - 841.666666666).abs() < 1e-3);
        // symmetric data → skew ≈ 0
        assert!(skewness(&xs).abs() < 0.01);
        // uniform dist → negative excess kurtosis
        assert!(kurtosis(&xs) < 0.0 && kurtosis(&xs) > -2.0);
    }

    #[test]
    fn quantile_interp() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(quantile(&xs, 0.5), 3.0);
        assert_eq!(quantile(&xs, 0.25), 2.0);
        assert_eq!(quantile(&xs, 0.0), 1.0);
        assert_eq!(quantile(&xs, 1.0), 5.0);
    }

    #[test]
    fn sharpe_sortino_omega() {
        let rets = vec![0.01, -0.005, 0.02, 0.0, -0.01, 0.015];
        assert!(sharpe_ratio(&rets, 0.0, 252.0) > 0.0);
        assert!(sortino_ratio(&rets, 0.0, 252.0) > 0.0);
        assert!(omega_ratio(&rets, 0.0) > 1.0);
        assert_eq!(omega_ratio(&[], 0.0), 0.0);
    }

    #[test]
    fn drawdown() {
        let prices = vec![100.0, 120.0, 90.0, 110.0];
        // peak 120 → trough 90 → -25%
        assert!((max_drawdown(&prices) + 0.25).abs() < 1e-9);
    }

    #[test]
    fn acf() {
        // alternating series → lag-1 strongly negative
        let xs: Vec<f64> = (0..200)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let acf = autocorrelation(&xs, 5);
        assert!(acf[0] < -0.9);
        assert!(acf[1] > 0.9);
    }

    #[test]
    fn ols_recovers_params() {
        // y = 2 + 0.5x + noise
        let x: Vec<f64> = (0..200).map(|i| (i as f64) / 10.0).collect();
        let y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, xv)| 2.0 + 0.5 * xv + (i % 7) as f64 * 0.01)
            .collect();
        let fit = ols(&y, &x).unwrap();
        assert!((fit.beta - 0.5).abs() < 0.01);
        assert!((fit.alpha - 2.0).abs() < 0.1);
        assert!(fit.r_squared > 0.99);
        assert_eq!(fit.residuals.len(), 200);
    }

    #[test]
    fn rolling_stats() {
        let xs: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let means = rolling(&xs, 3, mean);
        assert_eq!(means.len(), 8);
        assert!((means[0] - 2.0).abs() < 1e-9);
    }
}
