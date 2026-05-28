use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::Mutex;
use tracing::{info, info_span, warn};

use crate::application::data_quality::DataQualityChecker;
use crate::bus::message_bus::{BusMessage, MessageBus, Topic};
use crate::domain::models::*;
use crate::ports::market_data::AdapterHealth;
use crate::ports::market_data::MarketDataPort;
use crate::ports::storage::StoragePort;

/// Market Data Aggregator — the system's sensory cortex
pub struct MarketDataAggregator {
    adapters: Vec<Arc<dyn MarketDataPort>>,
    bus: Arc<MessageBus>,
    storage: Option<Arc<dyn StoragePort>>,
    quote_cache: Arc<DashMap<String, Quote>>,
    tick_buffer: Arc<Mutex<Vec<Tick>>>,
    symbol_map: HashMap<String, String>,
    started_adapters: Arc<DashMap<String, bool>>,
    flush_task_started: Arc<AtomicBool>,
    data_quality_checker: Arc<DataQualityChecker>,
}

impl MarketDataAggregator {
    /// Create a new Market Data Aggregator
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            adapters: Vec::new(),
            bus,
            storage: None,
            quote_cache: Arc::new(DashMap::new()),
            tick_buffer: Arc::new(Mutex::new(Vec::new())),
            symbol_map: HashMap::new(),
            started_adapters: Arc::new(DashMap::new()),
            flush_task_started: Arc::new(AtomicBool::new(false)),
            data_quality_checker: Arc::new(DataQualityChecker::new()),
        }
    }

    /// Add a market data adapter
    pub fn add_adapter(&mut self, adapter: Arc<dyn MarketDataPort>) {
        info!("Registering market data adapter: {}", adapter.adapter_id());
        self.adapters.push(adapter);
    }

    /// Register a symbol alias mapping
    pub fn add_symbol_mapping(&mut self, alias: String, canonical: String) {
        self.symbol_map.insert(alias, canonical);
    }

    /// Attach a storage backend for durable tick persistence.
    pub fn set_storage(&mut self, storage: Arc<dyn StoragePort>) {
        self.storage = Some(storage);
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
    #[tracing::instrument(skip(self, symbols))]
    pub async fn start(&self, symbols: &[Symbol]) -> Result<()> {
        let mut subscribed_any = false;
        let mut last_error = None;

        for adapter in &self.adapters {
            let adapter_id = adapter.adapter_id().to_string();

            if self.started_adapters.contains_key(&adapter_id) {
                continue;
            }

            let mut tick_stream = match adapter.subscribe(symbols).await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!(adapter = %adapter_id, error = %err, "Market data adapter subscription failed");
                    last_error = Some(err);
                    continue;
                }
            };

            self.started_adapters.insert(adapter_id.clone(), true);
            subscribed_any = true;
            let bus = self.bus.clone();
            let quote_cache = self.quote_cache.clone();
            let tick_buffer = self.tick_buffer.clone();
            let data_quality_checker = self.data_quality_checker.clone();

            // Spawn a task per adapter to read from its tick stream
            tokio::spawn(async move {
                while let Some(tick) = tick_stream.recv().await {
                    let symbol_key = tick.symbol.0.clone();
                    let span = info_span!("tick_pipeline", symbol = %symbol_key, source = %adapter_id);

                    // Validate tick with data quality checker
                    if data_quality_checker.validate_tick(&tick).is_err() {
                        warn!("Tick validation failed for {}, skipping", symbol_key);
                        continue;
                    }

                    let tick_clone = span.in_scope(|| {
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

                        tick
                    });

                    // Add to tick buffer for batch write
                    tick_buffer.lock().await.push(tick_clone);
                }
                warn!("Tick stream ended for adapter: {}", adapter_id);
            });
        }

        if !subscribed_any && !self.adapters.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
        }

        // Start tick buffer flush task (every 100ms)
        if !self.flush_task_started.swap(true, Ordering::SeqCst) {
            let tick_buffer = self.tick_buffer.clone();
            let storage = self.storage.clone();
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

                    if let Some(storage) = storage.as_ref() {
                        if let Err(error) = storage.write_ticks(&ticks).await {
                            warn!(error = %error, tick_count = ticks.len(), "Failed to persist tick batch");
                        }
                    }
                }
            });
        }

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

    /// Snapshot market data adapter health by adapter id.
    pub fn adapter_health(&self) -> Vec<(String, AdapterHealth)> {
        self.adapters
            .iter()
            .map(|adapter| (adapter.adapter_id().to_string(), adapter.health()))
            .collect()
    }

    /// Number of actively cached subscriptions/quotes.
    pub fn active_subscription_count(&self) -> usize {
        self.quote_cache.len()
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
