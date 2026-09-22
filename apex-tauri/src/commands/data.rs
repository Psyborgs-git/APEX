use crate::dto::OHLCVDto;
use crate::state::AppState;
use apex_core::domain::models::{OHLCVQuery, Symbol, Timeframe, OHLCV};
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

    // Refresh when the cache is empty, undersized, or its newest bar is older
    // than the requested timeframe's bucket — a partial/old cache must not
    // starve intraday charts or analytics of recent data.
    let bucket_secs: i64 = match tf {
        Timeframe::S1 => 1,
        Timeframe::S5 => 5,
        Timeframe::S15 => 15,
        Timeframe::M1 => 60,
        Timeframe::M3 => 180,
        Timeframe::M5 => 300,
        Timeframe::M15 => 900,
        Timeframe::M30 => 1800,
        Timeframe::H1 => 3600,
        Timeframe::H4 => 14400,
        Timeframe::D1 => 86_400,
        Timeframe::W1 => 604_800,
    };
    let newest_is_stale = bars
        .last()
        .map(|b| (now - b.time).num_seconds() > bucket_secs * 2)
        .unwrap_or(true);
    if bars.is_empty() || bars.len() < limit || newest_is_stale {
        let lookback_days = match tf {
            Timeframe::S1 | Timeframe::S5 | Timeframe::S15 => 1,
            Timeframe::M1 | Timeframe::M3 | Timeframe::M5 | Timeframe::M15 | Timeframe::M30 => 7,
            Timeframe::H1 | Timeframe::H4 => 60,
            Timeframe::D1 | Timeframe::W1 => 3650,
        };
        let from = now - chrono::Duration::days(lookback_days);

        // Storage-first: a provider outage must not blank charts that already
        // have cached history — only an empty cache makes the error fatal.
        let fetched = match state
            .history_source
            .get_historical_ohlcv(&Symbol(symbol.to_string()), tf, from, now)
            .await
        {
            Ok(b) => b,
            Err(e) if bars.is_empty() => {
                return Err(format!("Failed to fetch historical data for {symbol}: {e}"));
            }
            Err(e) => {
                tracing::warn!(
                    symbol = %symbol,
                    error = %e,
                    "OHLCV refresh failed; serving cached bars"
                );
                Vec::new()
            }
        };

        if !fetched.is_empty() {
            // Merge cached + fetched on bar time (fetched wins) so a larger
            // request backfills the cache instead of being trimmed by it.
            let mut merged: std::collections::BTreeMap<chrono::DateTime<Utc>, OHLCV> =
                bars.iter().map(|b| (b.time, b.clone())).collect();
            for b in &fetched {
                merged.insert(b.time, b.clone());
            }
            let merged: Vec<OHLCV> = merged.into_values().collect();
            if let Err(e) = state.storage.write_ohlcv(&merged).await {
                tracing::warn!(symbol = %symbol, error = %e, "Failed to persist fetched OHLCV bars");
            }
            bars = merged;
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
pub async fn get_watchlist_symbols(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    // Return symbols from the aggregator's quote cache
    let symbols: Vec<String> = state
        .aggregator
        .quote_cache()
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    Ok(symbols)
}
