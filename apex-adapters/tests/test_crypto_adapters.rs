use std::time::Duration;
use tokio::time::timeout;
use apex_adapters::market_data::binance::BinanceAdapter;
use apex_adapters::market_data::coinbase::CoinbaseAdapter;
use apex_adapters::market_data::polymarket::PolymarketAdapter;
use apex_core::ports::market_data::MarketDataPort;
use apex_core::domain::models::Symbol;

#[tokio::test]
async fn test_binance_adapter_connection() {
    let adapter = BinanceAdapter::new();
    
    // Test subscription
    let symbols = vec![Symbol("BTCUSDT".to_string())];
    let mut tick_stream = adapter.subscribe(&symbols).await.expect("Failed to subscribe");
    
    // Wait for at least one tick
    let tick = timeout(Duration::from_secs(10), tick_stream.recv())
        .await
        .expect("Timeout waiting for tick")
        .expect("Tick stream closed");
    
    println!("Received Binance tick: {:?}", tick);
    assert_eq!(tick.symbol.0, "BTCUSDT");
    assert!(tick.last > 0.0);
}

#[tokio::test]
async fn test_coinbase_adapter_connection() {
    let adapter = CoinbaseAdapter::new();
    
    // Test subscription
    let symbols = vec![Symbol("BTC-USD".to_string())];
    let mut tick_stream = adapter.subscribe(&symbols).await.expect("Failed to subscribe");
    
    // Wait for at least one tick
    let tick = timeout(Duration::from_secs(10), tick_stream.recv())
        .await
        .expect("Timeout waiting for tick")
        .expect("Tick stream closed");
    
    println!("Received Coinbase tick: {:?}", tick);
    assert_eq!(tick.symbol.0, "BTC-USD");
    assert!(tick.last > 0.0);
}

#[tokio::test]
async fn test_polymarket_adapter_markets() {
    let adapter = PolymarketAdapter::new();
    
    // Test fetching markets
    let markets = adapter.fetch_markets().await.expect("Failed to fetch markets");
    
    println!("Fetched {} Polymarket markets", markets.len());
    assert!(!markets.is_empty());
}

#[tokio::test]
async fn test_binance_historical_ohlcv() {
    let adapter = BinanceAdapter::new();
    let symbol = Symbol("BTCUSDT".to_string());
    
    let ohlcv = adapter.get_historical_ohlcv(
        &symbol,
        apex_core::domain::models::Timeframe::M1,
        chrono::Utc::now() - chrono::Duration::hours(1),
        chrono::Utc::now(),
    ).await.expect("Failed to fetch OHLCV");
    
    println!("Fetched {} Binance OHLCV bars", ohlcv.len());
    assert!(!ohlcv.is_empty());
}

#[tokio::test]
async fn test_coinbase_historical_ohlcv() {
    let adapter = CoinbaseAdapter::new();
    let symbol = Symbol("BTC-USD".to_string());
    
    let ohlcv = adapter.get_historical_ohlcv(
        &symbol,
        apex_core::domain::models::Timeframe::M1,
        chrono::Utc::now() - chrono::Duration::hours(1),
        chrono::Utc::now(),
    ).await.expect("Failed to fetch OHLCV");
    
    println!("Fetched {} Coinbase OHLCV bars", ohlcv.len());
    assert!(!ohlcv.is_empty());
}
