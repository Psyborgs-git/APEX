use crate::dto::QuoteDto;
use crate::state::AppState;
use apex_core::domain::models::Symbol;

/// Get a quote for a symbol from the cache.
pub async fn get_quote(symbol: String, state: &AppState) -> Result<QuoteDto, String> {
    state
        .aggregator
        .get_cached_quote(&symbol)
        .map(|q| QuoteDto::from(&q))
        .ok_or_else(|| format!("No quote available for {}", symbol))
}

/// Subscribe to real-time market data for symbols.
pub async fn subscribe_symbols(symbols: Vec<String>, state: &AppState) -> Result<(), String> {
    let syms: Vec<Symbol> = symbols.into_iter().map(Symbol).collect();
    state
        .aggregator
        .start(&syms)
        .await
        .map_err(|e| e.to_string())
}
