use crate::state::AppState;
use crate::validation;
use apex_core::application::scanner::{ScanConfig, ScanCriterion, ScanOutput};
use apex_core::domain::models::{Symbol, Timeframe};
use serde::Deserialize;
use tauri::State;

/// A single scan criterion as plain key/value pairs from the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct ScanCriterionDto {
    /// One of: price_above, price_below, price_between, volume_above,
    /// change_pct_above, change_pct_below, rsi_above, rsi_below,
    /// above_sma, below_sma
    pub kind: String,
    pub value: Option<f64>,
    pub value2: Option<f64>,
    pub period: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScanRequestDto {
    pub name: Option<String>,
    /// Symbols to scan; empty = watchlist symbols.
    pub symbols: Vec<String>,
    pub criteria: Vec<ScanCriterionDto>,
    pub timeframe: Option<String>,
    pub lookback_bars: Option<usize>,
}

pub(crate) fn to_criterion(dto: &ScanCriterionDto) -> Result<ScanCriterion, String> {
    let v = dto.value;
    match dto.kind.to_ascii_lowercase().as_str() {
        "price_above" => Ok(ScanCriterion::PriceAbove(
            v.ok_or("price_above requires a value")?,
        )),
        "price_below" => Ok(ScanCriterion::PriceBelow(
            v.ok_or("price_below requires a value")?,
        )),
        "price_between" => Ok(ScanCriterion::PriceBetween(
            v.ok_or("price_between requires value")?,
            dto.value2.ok_or("price_between requires value2")?,
        )),
        "volume_above" => Ok(ScanCriterion::VolumeAbove(
            v.ok_or("volume_above requires a value")? as u64,
        )),
        "change_pct_above" => Ok(ScanCriterion::ChangePctAbove(
            v.ok_or("change_pct_above requires a value")?,
        )),
        "change_pct_below" => Ok(ScanCriterion::ChangePctBelow(
            v.ok_or("change_pct_below requires a value")?,
        )),
        "rsi_above" => Ok(ScanCriterion::RsiAbove(
            dto.period.unwrap_or(14),
            v.ok_or("rsi_above requires a value")?,
        )),
        "rsi_below" => Ok(ScanCriterion::RsiBelow(
            dto.period.unwrap_or(14),
            v.ok_or("rsi_below requires a value")?,
        )),
        "above_sma" => Ok(ScanCriterion::AboveSma(dto.period.unwrap_or(20))),
        "below_sma" => Ok(ScanCriterion::BelowSma(dto.period.unwrap_or(20))),
        other => Err(format!("Unknown scan criterion: {}", other)),
    }
}

/// Run a market scan over a symbol universe.
#[tauri::command]
pub async fn run_scan(
    request: ScanRequestDto,
    state: State<'_, AppState>,
) -> Result<ScanOutput, String> {
    if request.criteria.is_empty() {
        return Err("At least one scan criterion is required".to_string());
    }
    if request.criteria.len() > 16 {
        return Err("Too many scan criteria (max 16)".to_string());
    }

    let universe: Vec<Symbol> = if request.symbols.is_empty() {
        state
            .aggregator
            .quote_cache()
            .iter()
            .map(|e| Symbol(e.key().clone()))
            .collect()
    } else {
        if request.symbols.len() > 200 {
            return Err("Scan universe too large (max 200 symbols)".to_string());
        }
        request
            .symbols
            .iter()
            .map(|s| {
                validation::validate_symbol(s)?;
                Ok(Symbol(s.clone()))
            })
            .collect::<Result<Vec<_>, String>>()?
    };

    if universe.is_empty() {
        return Err("No symbols to scan — add symbols to the watchlist first".to_string());
    }

    let criteria = request
        .criteria
        .iter()
        .map(to_criterion)
        .collect::<Result<Vec<_>, String>>()?;

    let timeframe = match request
        .timeframe
        .as_deref()
        .unwrap_or("d1")
        .to_lowercase()
        .as_str()
    {
        "1m" | "m1" => Timeframe::M1,
        "5m" | "m5" => Timeframe::M5,
        "15m" | "m15" => Timeframe::M15,
        "1h" | "h1" => Timeframe::H1,
        "4h" | "h4" => Timeframe::H4,
        "1d" | "d1" => Timeframe::D1,
        "1w" | "w1" => Timeframe::W1,
        _ => Timeframe::D1,
    };

    let config = ScanConfig {
        name: request.name.unwrap_or_else(|| "Ad-hoc scan".to_string()),
        universe,
        criteria,
        timeframe,
        lookback_bars: request.lookback_bars.unwrap_or(50).clamp(5, 1000),
    };

    state
        .scanner
        .run_scan(&config)
        .await
        .map_err(|e| format!("Scan failed: {}", e))
}
