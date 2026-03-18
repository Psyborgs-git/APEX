#![allow(dead_code)]
use std::sync::Arc;

use apex_adapters::execution::paper_trading::PaperTradingAdapter;
use apex_adapters::storage::sqlite_storage::SqliteStorage;
use apex_core::application::alert_engine::AlertEngine;
use apex_core::application::market_data_aggregator::MarketDataAggregator;
use apex_core::application::order_trade_manager::OrderTradeManager;
use apex_core::application::risk_engine::{RiskConfig, RiskEngine};
use apex_core::bus::message_bus::{BusMessage, MessageBus, Topic};

/// Shared application state — initialised once at startup, shared across all IPC handlers.
pub struct AppState {
    pub aggregator: Arc<MarketDataAggregator>,
    pub otm: Arc<OrderTradeManager>,
    pub alerts: Arc<AlertEngine>,
    pub risk: Arc<RiskEngine>,
    pub bus: Arc<MessageBus>,
    pub storage: Arc<SqliteStorage>,
}

impl AppState {
    /// Initialize all application services.
    pub async fn init() -> anyhow::Result<Self> {
        let bus = Arc::new(MessageBus::new());

        let storage = Arc::new(SqliteStorage::new("apex_data.db")?);
        storage.init_schema().await?;

        let risk = Arc::new(RiskEngine::new(RiskConfig::default()));

        let mut aggregator_inner = MarketDataAggregator::new(bus.clone());
        let yahoo_adapter = apex_adapters::market_data::yahoo_finance::YahooFinanceAdapter::new();
        aggregator_inner.add_adapter(Box::new(yahoo_adapter));
        let aggregator = Arc::new(aggregator_inner);

        // Register paper trading adapter — always available as default execution
        let paper = PaperTradingAdapter::new();
        let mut otm_inner = OrderTradeManager::new(risk.clone(), bus.clone());
        otm_inner.register_execution("paper".to_string(), Box::new(paper));
        let otm = Arc::new(otm_inner);

        let alerts = Arc::new(AlertEngine::new(bus.clone()));

        Ok(Self {
            aggregator,
            otm,
            alerts,
            risk,
            bus,
            storage,
        })
    }

    /// Start real-time event push to the frontend.
    /// Subscribes to message bus topics and forwards events via Tauri app handle.
    pub fn start_event_push(&self, app_handle: tauri::AppHandle) {
        use tauri::Emitter;

        // Forward quote updates
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::Quote("*".into()));
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::QuoteData(quote) = msg {
                    let dto = crate::dto::QuoteDto::from(&quote);
                    let _ = handle.emit("quote-update", &dto);
                }
            }
        });

        // Forward order updates
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::OrderUpdate("*".into()));
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::OrderData(order) = msg {
                    let dto = crate::dto::OrderDto::from(&order);
                    let _ = handle.emit("order-update", &dto);
                }
            }
        });

        // Forward position updates
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::PositionUpdate);
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::PositionData(pos) = msg {
                    let dto = crate::dto::PositionDto::from(&pos);
                    let _ = handle.emit("position-update", &dto);
                }
            }
        });

        // Forward news items
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::NewsItem);
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::News(item) = msg {
                    let dto = crate::dto::NewsItemDto::from(&item);
                    let _ = handle.emit("news-item", &dto);
                }
            }
        });

        // Forward alert events
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::Alert);
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::AlertFired(alert) = msg {
                    let dto = crate::dto::AlertDto {
                        rule_id: alert.rule_id,
                        message: alert.message,
                        severity: format!("{:?}", alert.severity),
                    };
                    let _ = handle.emit("alert-fired", &dto);
                }
            }
        });

        // Forward strategy signals
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::StrategySignal("*".into()));
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::Signal(signal) = msg {
                    let _ = handle.emit("strategy-signal", &signal);
                }
            }
        });

        // Forward adapter health
        let bus = self.bus.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe(Topic::SystemHealth);
            while let Ok(msg) = rx.recv().await {
                if let BusMessage::Health(health) = msg {
                    let _ = handle.emit("adapter-health", &health);
                }
            }
        });
    }
}
