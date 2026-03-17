#![allow(dead_code)]
use std::sync::Arc;

use apex_adapters::storage::sqlite_storage::SqliteStorage;
use apex_core::application::alert_engine::AlertEngine;
use apex_core::application::market_data_aggregator::MarketDataAggregator;
use apex_core::application::order_trade_manager::OrderTradeManager;
use apex_core::application::risk_engine::{RiskConfig, RiskEngine};
use apex_core::bus::MessageBus;

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

        let otm = Arc::new(OrderTradeManager::new(risk.clone(), bus.clone()));
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
}
