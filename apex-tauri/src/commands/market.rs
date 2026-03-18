use crate::dto::{OHLCVDto, QuoteDto};
use crate::state::AppState;
use apex_core::domain::models::Symbol;
use chrono::{DateTime, Utc};
use tauri::State;

/// Get a quote for a symbol from the cache.
#[tauri::command]
pub async fn get_quote(symbol: String, state: State<'_, AppState>) -> Result<QuoteDto, String> {
    state
        .aggregator
        .get_cached_quote(&symbol)
        .map(|q| QuoteDto::from(&q))
        .ok_or_else(|| format!("No quote available for {}", symbol))
}

/// Get historical OHLCV data for a symbol.
#[tauri::command]
pub async fn get_ohlcv(
    symbol: String,
    timeframe: String,
    from: i64,
    to: i64,
    state: State<'_, AppState>,
) -> Result<Vec<OHLCVDto>, String> {
    use apex_core::domain::models::Timeframe;

    let tf = match timeframe.to_lowercase().as_str() {
        "1s" | "s1" => Timeframe::S1,
        "5s" | "s5" => Timeframe::S5,
        "15s" | "s15" => Timeframe::S15,
        "1m" | "m1" => Timeframe::M1,
        "3m" | "m3" => Timeframe::M3,
        "5m" | "m5" => Timeframe::M5,
        "15m" | "m15" => Timeframe::M15,
        "30m" | "m30" => Timeframe::M30,
        "1h" | "h1" => Timeframe::H1,
        "4h" | "h4" => Timeframe::H4,
        "1d" | "d1" => Timeframe::D1,
        "1w" | "w1" => Timeframe::W1,
        _ => return Err(format!("Invalid timeframe: {}", timeframe)),
    };

    let from_dt = DateTime::<Utc>::from_timestamp(from, 0)
        .ok_or_else(|| "Invalid from timestamp".to_string())?;
    let to_dt = DateTime::<Utc>::from_timestamp(to, 0)
        .ok_or_else(|| "Invalid to timestamp".to_string())?;

    // Query from storage
    let params = apex_core::domain::models::OHLCVQuery {
        symbol: Symbol(symbol.clone()),
        timeframe: tf,
        from: from_dt,
        to: to_dt,
        limit: Some(1000),
    };

    use apex_core::ports::storage::StoragePort;
    match state.storage.query_ohlcv(params).await {
        Ok(bars) if !bars.is_empty() => {
            Ok(bars.iter().map(OHLCVDto::from).collect())
        }
        _ => {
            // Fall back to aggregator adapter for historical data
            Err(format!("No historical data available for {} in timeframe {}", symbol, timeframe))
        }
    }
}

/// Subscribe to real-time market data for symbols.
#[tauri::command]
pub async fn subscribe_symbols(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let syms: Vec<Symbol> = symbols.into_iter().map(Symbol).collect();
    state
        .aggregator
        .start(&syms)
        .await
        .map_err(|e| e.to_string())
}
