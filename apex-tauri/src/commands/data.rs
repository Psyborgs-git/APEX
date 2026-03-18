use crate::dto::OHLCVDto;
use crate::state::AppState;
use apex_core::domain::models::{OHLCVQuery, Symbol, Timeframe};
use apex_core::ports::storage::StoragePort;
use chrono::Utc;
use tauri::State;

/// Get historical data for a symbol from storage.
#[tauri::command]
pub async fn get_historical_data(
    symbol: String,
    timeframe: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<OHLCVDto>, String> {
    let tf = match timeframe.as_deref().unwrap_or("d1").to_lowercase().as_str() {
        "1m" | "m1" => Timeframe::M1,
        "5m" | "m5" => Timeframe::M5,
        "15m" | "m15" => Timeframe::M15,
        "1h" | "h1" => Timeframe::H1,
        "4h" | "h4" => Timeframe::H4,
        "1d" | "d1" => Timeframe::D1,
        "1w" | "w1" => Timeframe::W1,
        _ => Timeframe::D1,
    };

    let params = OHLCVQuery {
        symbol: Symbol(symbol.clone()),
        timeframe: tf,
        from: chrono::DateTime::UNIX_EPOCH,
        to: Utc::now(),
        limit: limit.or(Some(500)),
    };

    state
        .storage
        .query_ohlcv(params)
        .await
        .map(|bars| bars.iter().map(OHLCVDto::from).collect())
        .map_err(|e| format!("Failed to query historical data for {}: {}", symbol, e))
}

/// Get watchlist symbols from storage.
#[tauri::command]
pub async fn get_watchlist_symbols(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // Return symbols from the aggregator's quote cache
    let symbols: Vec<String> = state
        .aggregator
        .quote_cache()
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    Ok(symbols)
}
