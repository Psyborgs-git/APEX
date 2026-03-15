use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::bus::message_bus::{BusMessage, MessageBus, Topic};
use crate::domain::models::*;
use crate::ports::market_data::MarketDataPort;

/// Market Data Aggregator — the system's sensory cortex
pub struct MarketDataAggregator {
    adapters: Vec<Box<dyn MarketDataPort>>,
    bus: Arc<MessageBus>,
    quote_cache: Arc<DashMap<String, Quote>>,
    tick_buffer: Arc<Mutex<Vec<Tick>>>,
    symbol_map: HashMap<String, String>,
}

impl MarketDataAggregator {
    /// Create a new Market Data Aggregator
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            adapters: Vec::new(),
            bus,
            quote_cache: Arc::new(DashMap::new()),
            tick_buffer: Arc::new(Mutex::new(Vec::new())),
            symbol_map: HashMap::new(),
        }
    }

    /// Add a market data adapter
    pub fn add_adapter(&mut self, adapter: Box<dyn MarketDataPort>) {
        info!("Registering market data adapter: {}", adapter.adapter_id());
        self.adapters.push(adapter);
    }

    /// Register a symbol alias mapping
    pub fn add_symbol_mapping(&mut self, alias: String, canonical: String) {
        self.symbol_map.insert(alias, canonical);
    }

    /// Get the shared quote cache
    pub fn quote_cache(&self) -> Arc<DashMap<String, Quote>> {
        self.quote_cache.clone()
    }

    /// Resolve a symbol to its canonical form
    pub fn resolve_symbol(&self, symbol: &str) -> String {
        self.symbol_map.get(symbol).cloned().unwrap_or_else(|| symbol.to_string())
    }

    /// Subscribe to symbols across all adapters and start processing
    pub async fn start(&self, symbols: &[Symbol]) -> Result<()> {
        for adapter in &self.adapters {
            let mut tick_stream = adapter.subscribe(symbols).await?;
            let bus = self.bus.clone();
            let quote_cache = self.quote_cache.clone();
            let tick_buffer = self.tick_buffer.clone();
            let adapter_id = adapter.adapter_id().to_string();

            // Spawn a task per adapter to read from its tick stream
            tokio::spawn(async move {
                while let Some(tick) = tick_stream.recv().await {
                    let symbol_key = tick.symbol.0.clone();

                    // Update quote cache
                    let quote = Quote {
                        symbol: tick.symbol.clone(),
                        bid: tick.bid,
                        ask: tick.ask,
                        last: tick.last,
                        open: tick.last,
                        high: tick.last,
                        low: tick.last,
                        volume: tick.volume,
                        change_pct: 0.0,
                        vwap: tick.last,
                        updated_at: tick.time,
                    };
                    quote_cache.insert(symbol_key.clone(), quote.clone());

                    // Publish to message bus
                    bus.publish(
                        Topic::Tick(symbol_key.clone()),
                        BusMessage::TickData(tick.clone()),
                    );
                    bus.publish(
                        Topic::Quote(symbol_key),
                        BusMessage::QuoteData(quote),
                    );

                    // Add to tick buffer for batch write
                    tick_buffer.lock().await.push(tick);
                }
                warn!("Tick stream ended for adapter: {}", adapter_id);
            });
        }

        // Start tick buffer flush task (every 100ms)
        let tick_buffer = self.tick_buffer.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                let ticks: Vec<Tick> = {
                    let mut buffer = tick_buffer.lock().await;
                    if buffer.is_empty() {
                        continue;
                    }
                    buffer.drain(..).collect()
                };
                // In production, these would be written to storage
                // For now, we just drain the buffer
                let _count = ticks.len();
            }
        });

        Ok(())
    }

    /// Get a snapshot quote for a symbol from cache
    pub fn get_cached_quote(&self, symbol: &str) -> Option<Quote> {
        self.quote_cache.get(symbol).map(|q| q.clone())
    }

    /// Get the number of registered adapters
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_create_aggregator() {
        let bus = Arc::new(MessageBus::new());
        let agg = MarketDataAggregator::new(bus);
        assert_eq!(agg.adapter_count(), 0);
    }

    #[test]
    fn test_symbol_mapping() {
        let bus = Arc::new(MessageBus::new());
        let mut agg = MarketDataAggregator::new(bus);
        agg.add_symbol_mapping("REL".into(), "RELIANCE.NS".into());
        assert_eq!(agg.resolve_symbol("REL"), "RELIANCE.NS");
        assert_eq!(agg.resolve_symbol("AAPL"), "AAPL");
    }

    #[tokio::test]
    async fn test_quote_cache() {
        let bus = Arc::new(MessageBus::new());
        let agg = MarketDataAggregator::new(bus);

        // Initially empty
        assert!(agg.get_cached_quote("AAPL").is_none());

        // Manually insert a quote
        agg.quote_cache.insert("AAPL".into(), Quote {
            symbol: Symbol("AAPL".into()),
            bid: 150.0,
            ask: 150.05,
            last: 150.02,
            open: 149.0,
            high: 151.0,
            low: 148.5,
            volume: 10000,
            change_pct: 0.5,
            vwap: 149.8,
            updated_at: Utc::now(),
        });

        let quote = agg.get_cached_quote("AAPL").unwrap();
        assert_eq!(quote.symbol.0, "AAPL");
        assert!((quote.last - 150.02).abs() < 0.001);
    }
}
