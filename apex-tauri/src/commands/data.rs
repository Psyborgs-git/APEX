use crate::dto::OHLCVDto;
use crate::state::AppState;
use apex_core::domain::models::{OHLCV, OHLCVQuery, Symbol, Timeframe};
use chrono::Utc;
use tauri::State;

/// Load OHLCV bars for a symbol: storage first, falling back to the market
/// data adapter (persisting what it returns). Shared by chart/history,
/// indicator, and quant commands — OpenBB-style "fetch then cache" pipeline.
pub(crate) async fn load_bars(
    state: &AppState,
    symbol: &str,
    timeframe: Option<&str>,
    limit: usize,
) -> Result<Vec<OHLCV>, String> {
    let tf = match timeframe.unwrap_or("d1").to_lowercase().as_str() {
        "1m" | "m1" => Timeframe::M1,
        "5m" | "m5" => Timeframe::M5,
        "15m" | "m15" => Timeframe::M15,
        "1h" | "h1" => Timeframe::H1,
        "4h" | "h4" => Timeframe::H4,
        "1d" | "d1" => Timeframe::D1,
        "1w" | "w1" => Timeframe::W1,
        _ => Timeframe::D1,
    };

    let now = Utc::now();

    let params = OHLCVQuery {
        symbol: Symbol(symbol.to_string()),
        timeframe: tf.clone(),
        from: chrono::DateTime::UNIX_EPOCH,
        to: now,
        limit: Some(limit),
    };

    let mut bars = state
        .storage
        .query_ohlcv(params)
        .await
        .map_err(|e| format!("Failed to query historical data for {}: {}", symbol, e))?;

    // Storage miss → pull from the market data adapter and persist for next time.
    if bars.is_empty() {
        let lookback_days = match tf {
            Timeframe::S1 | Timeframe::S5 | Timeframe::S15 => 1,
            Timeframe::M1 | Timeframe::M3 | Timeframe::M5 | Timeframe::M15 | Timeframe::M30 => 7,
            Timeframe::H1 | Timeframe::H4 => 60,
            Timeframe::D1 | Timeframe::W1 => 3650,
        };
        let from = now - chrono::Duration::days(lookback_days);

        let fetched = state
            .history_source
            .get_historical_ohlcv(&Symbol(symbol.to_string()), tf, from, now)
            .await
            .map_err(|e| format!("Failed to fetch historical data for {}: {}", symbol, e))?;

        if !fetched.is_empty() {
            if let Err(e) = state.storage.write_ohlcv(&fetched).await {
                tracing::warn!(symbol = %symbol, error = %e, "Failed to persist fetched OHLCV bars");
            }
            bars = fetched;
        }
    }

    let start = bars.len().saturating_sub(limit);
    Ok(bars.split_off(start))
}

/// Get historical data for a symbol from storage.
#[tauri::command]
pub async fn get_historical_data(
    symbol: String,
    timeframe: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<OHLCVDto>, String> {
    let bars = load_bars(&state, &symbol, timeframe.as_deref(), limit.unwrap_or(500)).await?;
    Ok(bars.iter().map(OHLCVDto::from).collect())
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
