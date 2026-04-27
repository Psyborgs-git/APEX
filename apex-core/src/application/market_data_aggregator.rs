use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc, Weekday};
use dashmap::DashMap;
use tokio::sync::Mutex;
use tracing::{debug, info, info_span, warn};

use crate::bus::message_bus::{BusMessage, MessageBus, Topic};
use crate::domain::models::*;
use crate::ports::market_data::MarketDataPort;
use crate::ports::storage::StoragePort;

const DEFAULT_GAP_THRESHOLD_MS: i64 = 1_500;
const DEFAULT_REPLAY_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickRejectReason {
    Duplicate,
    OutOfOrder,
    StaleByArbitration,
    OutOfSession,
}

#[derive(Debug, Clone)]
pub struct TickGapEvent {
    pub symbol: Symbol,
    pub source: String,
    pub previous_time: DateTime<Utc>,
    pub current_time: DateTime<Utc>,
    pub gap_ms: i64,
}

#[derive(Debug, Clone)]
pub struct InstrumentMetadata {
    pub symbol: Symbol,
    pub exchange: String,
    pub tick_size: f64,
    pub lot_size: u32,
    pub currency: String,
    pub isin: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CorporateActionType {
    Split,
    ReverseSplit,
    Bonus,
    Dividend,
}

#[derive(Debug, Clone)]
pub struct CorporateAction {
    pub symbol: Symbol,
    pub effective_date: NaiveDate,
    pub action: CorporateActionType,
    /// Multiplicative factor to map raw feed prices onto adjusted prices.
    pub factor: f64,
}

#[derive(Debug, Clone)]
pub struct ExchangeCalendar {
    pub exchange: String,
    pub timezone: String,
    pub open_time: NaiveTime,
    pub close_time: NaiveTime,
    pub trading_days: Vec<Weekday>,
}

impl ExchangeCalendar {
    fn is_open(&self, now: DateTime<Utc>) -> bool {
        let today = now.weekday();
        if !self.trading_days.contains(&today) {
            return false;
        }

        let local_time = now.time();
        local_time >= self.open_time && local_time <= self.close_time
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketSessionState {
    PreOpen,
    Open,
    Closed,
    Halted,
}

#[derive(Debug, Clone)]
struct TickKey {
    time: DateTime<Utc>,
    bid: u64,
    ask: u64,
    last: u64,
    volume: u64,
    source: String,
}

impl TickKey {
    fn from_tick(tick: &Tick) -> Self {
        Self {
            time: tick.time,
            bid: (tick.bid * 1_000_000.0) as u64,
            ask: (tick.ask * 1_000_000.0) as u64,
            last: (tick.last * 1_000_000.0) as u64,
            volume: tick.volume,
            source: tick.source.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct SymbolFeedState {
    last_tick_time: DateTime<Utc>,
    last_arrival_time: DateTime<Utc>,
    last_tick_key: TickKey,
    preferred_feed: String,
}

/// Market Data Aggregator — the system's sensory cortex
pub struct MarketDataAggregator {
    adapters: Vec<Box<dyn MarketDataPort>>,
    bus: Arc<MessageBus>,
    storage: Option<Arc<dyn StoragePort>>,
    quote_cache: Arc<DashMap<String, Quote>>,
    tick_buffer: Arc<Mutex<Vec<Tick>>>,
    symbol_map: HashMap<String, String>,
    instrument_master: Arc<DashMap<String, InstrumentMetadata>>,
    corporate_actions: Arc<DashMap<String, Vec<CorporateAction>>>,
    exchange_calendars: Arc<DashMap<String, ExchangeCalendar>>,
    session_state: Arc<DashMap<String, MarketSessionState>>,
    feed_priority: Arc<DashMap<String, u8>>,
    feed_state: Arc<DashMap<String, SymbolFeedState>>,
    replay_buffers: Arc<DashMap<String, VecDeque<Tick>>>,
    gap_events: Arc<Mutex<Vec<TickGapEvent>>>,
    gap_threshold_ms: i64,
}

impl MarketDataAggregator {
    /// Create a new Market Data Aggregator
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self::new_with_storage(bus, None)
    }

    /// Create a market data aggregator with optional durable storage.
    pub fn new_with_storage(bus: Arc<MessageBus>, storage: Option<Arc<dyn StoragePort>>) -> Self {
        Self {
            adapters: Vec::new(),
            bus,
            storage,
            quote_cache: Arc::new(DashMap::new()),
            tick_buffer: Arc::new(Mutex::new(Vec::new())),
            symbol_map: HashMap::new(),
            instrument_master: Arc::new(DashMap::new()),
            corporate_actions: Arc::new(DashMap::new()),
            exchange_calendars: Arc::new(DashMap::new()),
            session_state: Arc::new(DashMap::new()),
            feed_priority: Arc::new(DashMap::new()),
            feed_state: Arc::new(DashMap::new()),
            replay_buffers: Arc::new(DashMap::new()),
            gap_events: Arc::new(Mutex::new(Vec::new())),
            gap_threshold_ms: DEFAULT_GAP_THRESHOLD_MS,
        }
    }

    /// Add a market data adapter
    pub fn add_adapter(&mut self, adapter: Box<dyn MarketDataPort>) {
        info!("Registering market data adapter: {}", adapter.adapter_id());
        self.feed_priority
            .insert(adapter.adapter_id().to_string(), 100);
        self.adapters.push(adapter);
    }

    /// Set feed priority (lower value = higher preference).
    pub fn set_feed_priority(&self, adapter_id: &str, priority: u8) {
        self.feed_priority.insert(adapter_id.to_string(), priority);
    }

    /// Register a symbol alias mapping
    pub fn add_symbol_mapping(&mut self, alias: String, canonical: String) {
        self.symbol_map.insert(alias, canonical);
    }

    /// Register instrument metadata in symbol master.
    pub fn register_instrument(&self, metadata: InstrumentMetadata) {
        self.instrument_master
            .insert(metadata.symbol.0.clone(), metadata);
    }

    /// Register an exchange trading calendar.
    pub fn register_exchange_calendar(&self, calendar: ExchangeCalendar) {
        self.exchange_calendars
            .insert(calendar.exchange.clone(), calendar);
    }

    /// Register corporate actions for a symbol.
    pub fn register_corporate_actions(&self, symbol: &str, actions: Vec<CorporateAction>) {
        self.corporate_actions.insert(symbol.to_string(), actions);
    }

    /// Override market session state when exchange-side signals are available.
    pub fn set_market_session_state(&self, exchange: &str, state: MarketSessionState) {
        self.session_state.insert(exchange.to_string(), state);
    }

    /// Get the shared quote cache
    pub fn quote_cache(&self) -> Arc<DashMap<String, Quote>> {
        self.quote_cache.clone()
    }

    /// Resolve a symbol to its canonical form
    pub fn resolve_symbol(&self, symbol: &str) -> String {
        self.symbol_map
            .get(symbol)
            .cloned()
            .unwrap_or_else(|| symbol.to_string())
    }

    /// Replay persisted ticks for a symbol and republish them in event order.
    pub async fn replay_symbol(
        &self,
        symbol: &Symbol,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<usize> {
        let Some(storage) = &self.storage else {
            return Ok(0);
        };

        let ticks = storage
            .query_ticks(symbol, from, to, Some(DEFAULT_REPLAY_LIMIT))
            .await?;
        for tick in &ticks {
            self.bus.publish(
                Topic::Tick(tick.symbol.0.clone()),
                BusMessage::TickData(tick.clone()),
            );
        }
        Ok(ticks.len())
    }

    /// Drain and return detected gap events.
    pub async fn take_gap_events(&self) -> Vec<TickGapEvent> {
        let mut gaps = self.gap_events.lock().await;
        gaps.drain(..).collect()
    }

    /// Subscribe to symbols across all adapters and start processing
    #[tracing::instrument(skip(self, symbols))]
    pub async fn start(&self, symbols: &[Symbol]) -> Result<()> {
        for adapter in &self.adapters {
            let mut tick_stream = adapter.subscribe(symbols).await?;
            let bus = self.bus.clone();
            let quote_cache = self.quote_cache.clone();
            let tick_buffer = self.tick_buffer.clone();
            let symbol_map = self.symbol_map.clone();
            let adapter_id = adapter.adapter_id().to_string();
            let feed_priority = self.feed_priority.clone();
            let feed_state = self.feed_state.clone();
            let replay_buffers = self.replay_buffers.clone();
            let corporate_actions = self.corporate_actions.clone();
            let instrument_master = self.instrument_master.clone();
            let calendars = self.exchange_calendars.clone();
            let session_state = self.session_state.clone();
            let gap_events = self.gap_events.clone();
            let gap_threshold_ms = self.gap_threshold_ms;

            // Spawn a task per adapter to read from its tick stream
            tokio::spawn(async move {
                while let Some(raw_tick) = tick_stream.recv().await {
                    let mut tick = raw_tick;
                    tick.symbol.0 = symbol_map
                        .get(&tick.symbol.0)
                        .cloned()
                        .unwrap_or_else(|| tick.symbol.0.clone());

                    let symbol_key = tick.symbol.0.clone();
                    let span =
                        info_span!("tick_pipeline", symbol = %symbol_key, source = %adapter_id);

                    let accepted = span.in_scope(|| {
                        if !market_is_open_for_tick(
                            &tick,
                            &instrument_master,
                            &calendars,
                            &session_state,
                        ) {
                            return Err(TickRejectReason::OutOfSession);
                        }

                        apply_corporate_actions(&mut tick, &corporate_actions);

                        if let Some(reject) =
                            classify_tick_rejection(&tick, &adapter_id, &feed_priority, &feed_state)
                        {
                            return Err(reject);
                        }

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

                        bus.publish(
                            Topic::Tick(symbol_key.clone()),
                            BusMessage::TickData(tick.clone()),
                        );
                        bus.publish(
                            Topic::Quote(symbol_key.clone()),
                            BusMessage::QuoteData(quote),
                        );

                        let mut rb = replay_buffers
                            .entry(symbol_key)
                            .or_insert_with(|| VecDeque::with_capacity(DEFAULT_REPLAY_LIMIT));
                        if rb.len() >= DEFAULT_REPLAY_LIMIT {
                            rb.pop_front();
                        }
                        rb.push_back(tick.clone());

                        Ok(tick.clone())
                    });

                    match accepted {
                        Ok(tick_clone) => {
                            detect_and_record_gap(
                                &tick_clone,
                                gap_threshold_ms,
                                &feed_state,
                                &gap_events,
                            )
                            .await;
                            tick_buffer.lock().await.push(tick_clone);
                        }
                        Err(reason) => {
                            debug!(symbol = %tick.symbol.0, source = %adapter_id, ?reason, "tick dropped");
                        }
                    }
                }
                warn!("Tick stream ended for adapter: {}", adapter_id);
            });
        }

        // Start tick buffer flush task (every 100ms) with durable persistence.
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

                if let Some(storage) = &storage {
                    if let Err(err) = storage.write_ticks(&ticks).await {
                        warn!(error = %err, count = ticks.len(), "failed to persist tick batch");
                    }
                }
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

fn classify_tick_rejection(
    tick: &Tick,
    adapter_id: &str,
    feed_priority: &Arc<DashMap<String, u8>>,
    feed_state: &Arc<DashMap<String, SymbolFeedState>>,
) -> Option<TickRejectReason> {
    let key = TickKey::from_tick(tick);
    let symbol_key = tick.symbol.0.clone();

    if let Some(prev) = feed_state.get(&symbol_key) {
        if prev.last_tick_key.time > tick.time {
            return Some(TickRejectReason::OutOfOrder);
        }

        if prev.last_tick_key.time == tick.time
            && prev.last_tick_key.bid == key.bid
            && prev.last_tick_key.ask == key.ask
            && prev.last_tick_key.last == key.last
            && prev.last_tick_key.volume == key.volume
            && prev.last_tick_key.source == key.source
        {
            return Some(TickRejectReason::Duplicate);
        }

        let incoming_priority = feed_priority.get(adapter_id).map(|v| *v).unwrap_or(100);
        let current_priority = feed_priority
            .get(&prev.preferred_feed)
            .map(|v| *v)
            .unwrap_or(100);
        let stale_window = prev
            .last_arrival_time
            .signed_duration_since(Utc::now())
            .num_milliseconds()
            .abs();

        if incoming_priority > current_priority && stale_window < 1_000 {
            return Some(TickRejectReason::StaleByArbitration);
        }
    }

    feed_state.insert(
        symbol_key,
        SymbolFeedState {
            last_tick_time: tick.time,
            last_arrival_time: Utc::now(),
            last_tick_key: key,
            preferred_feed: adapter_id.to_string(),
        },
    );

    None
}

fn apply_corporate_actions(
    tick: &mut Tick,
    actions_map: &Arc<DashMap<String, Vec<CorporateAction>>>,
) {
    let Some(actions) = actions_map.get(&tick.symbol.0) else {
        return;
    };

    for action in actions.iter() {
        if tick.time.date_naive() >= action.effective_date {
            let factor = action.factor;
            if factor > 0.0 {
                tick.bid *= factor;
                tick.ask *= factor;
                tick.last *= factor;
            }
        }
    }
}

fn market_is_open_for_tick(
    tick: &Tick,
    instrument_master: &Arc<DashMap<String, InstrumentMetadata>>,
    calendars: &Arc<DashMap<String, ExchangeCalendar>>,
    session_state: &Arc<DashMap<String, MarketSessionState>>,
) -> bool {
    let Some(instrument) = instrument_master.get(&tick.symbol.0) else {
        return true;
    };

    if let Some(state) = session_state.get(&instrument.exchange) {
        if matches!(
            *state,
            MarketSessionState::Closed | MarketSessionState::Halted
        ) {
            return false;
        }
    }

    if let Some(calendar) = calendars.get(&instrument.exchange) {
        return calendar.is_open(tick.time);
    }

    true
}

async fn detect_and_record_gap(
    tick: &Tick,
    gap_threshold_ms: i64,
    feed_state: &Arc<DashMap<String, SymbolFeedState>>,
    gap_events: &Arc<Mutex<Vec<TickGapEvent>>>,
) {
    if let Some(state) = feed_state.get(&tick.symbol.0) {
        let gap_ms = tick
            .time
            .signed_duration_since(state.last_tick_time)
            .num_milliseconds();
        if gap_ms > gap_threshold_ms {
            gap_events.lock().await.push(TickGapEvent {
                symbol: tick.symbol.clone(),
                source: tick.source.clone(),
                previous_time: state.last_tick_time,
                current_time: tick.time,
                gap_ms,
            });
        }
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
        agg.quote_cache.insert(
            "AAPL".into(),
            Quote {
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
            },
        );

        let quote = agg.get_cached_quote("AAPL").unwrap();
        assert_eq!(quote.symbol.0, "AAPL");
        assert!((quote.last - 150.02).abs() < 0.001);
    }

    #[test]
    fn test_reject_duplicate_and_out_of_order_ticks() {
        let feed_priority = Arc::new(DashMap::new());
        let feed_state = Arc::new(DashMap::new());
        feed_priority.insert("feed-a".to_string(), 10);

        let base_time = Utc::now();
        let tick = Tick {
            time: base_time,
            symbol: Symbol("AAPL".to_string()),
            bid: 100.0,
            ask: 100.1,
            last: 100.05,
            volume: 100,
            source: "feed-a".to_string(),
        };

        assert!(classify_tick_rejection(&tick, "feed-a", &feed_priority, &feed_state).is_none());
        assert_eq!(
            classify_tick_rejection(&tick, "feed-a", &feed_priority, &feed_state),
            Some(TickRejectReason::Duplicate)
        );

        let mut old_tick = tick.clone();
        old_tick.time = base_time - chrono::Duration::milliseconds(1);
        assert_eq!(
            classify_tick_rejection(&old_tick, "feed-a", &feed_priority, &feed_state),
            Some(TickRejectReason::OutOfOrder)
        );
    }

    #[test]
    fn test_corporate_action_adjustment_applies_factor() {
        let actions_map = Arc::new(DashMap::new());
        let today = Utc::now().date_naive();
        actions_map.insert(
            "AAPL".to_string(),
            vec![CorporateAction {
                symbol: Symbol("AAPL".to_string()),
                effective_date: today,
                action: CorporateActionType::Split,
                factor: 0.5,
            }],
        );

        let mut tick = Tick {
            time: Utc::now(),
            symbol: Symbol("AAPL".to_string()),
            bid: 200.0,
            ask: 200.2,
            last: 200.1,
            volume: 100,
            source: "feed-a".to_string(),
        };

        apply_corporate_actions(&mut tick, &actions_map);
        assert!((tick.last - 100.05).abs() < 0.0001);
    }
}
