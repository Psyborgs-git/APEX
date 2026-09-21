use crate::state::AppState;
use crate::validation;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct OrderBookLevelDto {
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderBookDto {
    pub symbol: String,
    /// "binance" = real L2 depth; "synthetic" = estimated from the cached quote.
    pub source: String,
    pub bids: Vec<OrderBookLevelDto>,
    pub asks: Vec<OrderBookLevelDto>,
}

/// Map an APEX symbol to a Binance spot pair when it looks like a crypto pair.
fn binance_pair(symbol: &str) -> Option<String> {
    let cleaned = symbol.replace('/', "").replace('-', "").to_uppercase();
    let quotes = ["USDT", "USDC", "BTC", "ETH", "BNB"];
    let looks_crypto = quotes.iter().any(|q| cleaned.ends_with(q)) && cleaned.len() >= 5;
    if looks_crypto { Some(cleaned) } else { None }
}

/// Get order book depth for a symbol. Real L2 data is available only for
/// crypto pairs (via Binance's public depth endpoint); other symbols fall
/// back to a synthetic book estimated around the cached quote.
#[tauri::command]
pub async fn get_order_book(
    symbol: String,
    state: State<'_, AppState>,
) -> Result<OrderBookDto, String> {
    validation::validate_symbol(&symbol)?;

    if let Some(pair) = binance_pair(&symbol) {
        let url = format!(
            "https://api.binance.com/api/v3/depth?symbol={}&limit=20",
            pair
        );
        #[derive(serde::Deserialize)]
        struct Depth {
            bids: Vec<(String, String)>,
            asks: Vec<(String, String)>,
        }
        if let Ok(resp) = state.http.get(&url).send().await {
            if let Ok(depth) = resp.json::<Depth>().await {
                if !depth.bids.is_empty() {
                    let bids = depth
                        .bids
                        .iter()
                        .filter_map(|(p, q)| {
                            Some(OrderBookLevelDto {
                                price: p.parse().ok()?,
                                quantity: q.parse().ok()?,
                            })
                        })
                        .collect();
                    let asks = depth
                        .asks
                        .iter()
                        .filter_map(|(p, q)| {
                            Some(OrderBookLevelDto {
                                price: p.parse().ok()?,
                                quantity: q.parse().ok()?,
                            })
                        })
                        .collect();
                    return Ok(OrderBookDto {
                        symbol,
                        source: "binance".to_string(),
                        bids,
                        asks,
                    });
                }
            }
        }
    }

    // Synthetic book estimated around the cached quote — deterministic per
    // symbol so the visualisation is stable between polls.
    let quote = state
        .aggregator
        .get_cached_quote(&symbol)
        .ok_or_else(|| format!("No quote or depth available for {}", symbol))?;

    let mid = (quote.bid + quote.ask) / 2.0;
    if mid <= 0.0 {
        return Err(format!("Invalid quote for {}", symbol));
    }
    let spread = (quote.ask - quote.bid).max(mid * 0.0005);
    let seed: u32 = symbol.bytes().fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
    let mut bids = Vec::with_capacity(20);
    let mut asks = Vec::with_capacity(20);
    let base_qty = (quote.volume as f64 / 400.0).max(1.0);

    for i in 0..20usize {
        // xorshift-ish deterministic pseudo-random for size jitter
        let rb = seed.wrapping_mul(i as u32 + 1).rotate_left(7) % 1000;
        let ra = seed.wrapping_mul(i as u32 + 101).rotate_left(11) % 1000;
        let step = spread * (i as f64 + 0.5);
        bids.push(OrderBookLevelDto {
            price: quote.bid - step,
            quantity: (base_qty * (0.4 + rb as f64 / 500.0)).max(0.01),
        });
        asks.push(OrderBookLevelDto {
            price: quote.ask + step,
            quantity: (base_qty * (0.4 + ra as f64 / 500.0)).max(0.01),
        });
    }

    Ok(OrderBookDto {
        symbol,
        source: "synthetic".to_string(),
        bids,
        asks,
    })
}
