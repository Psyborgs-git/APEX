use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use apex_adapters::execution::angel_one_execution::AngelOneExecutionAdapter;
use apex_adapters::execution::binance::BinanceExecutionAdapter;
use apex_adapters::execution::coinbase::CoinbaseExecutionAdapter;
use apex_adapters::execution::groww_execution::GrowwExecutionAdapter;
use apex_adapters::execution::paper_trading::PaperTradingAdapter;
use apex_adapters::execution::robinhood_execution::RobinhoodExecutionAdapter;
use apex_adapters::execution::zerodha_execution::ZerodhaExecutionAdapter;
use apex_adapters::market_data::angel_one_market_data::AngelOneMarketDataAdapter;
use apex_adapters::market_data::binance::BinanceAdapter;
use apex_adapters::market_data::coinbase::CoinbaseAdapter;
use apex_adapters::market_data::groww_market_data::GrowwMarketDataAdapter;
use apex_adapters::market_data::polymarket::PolymarketAdapter;
use apex_adapters::market_data::robinhood_market_data::RobinhoodMarketDataAdapter;
use apex_adapters::market_data::yahoo_finance::YahooFinanceAdapter;
use apex_adapters::market_data::zerodha_kite::ZerodhaKiteAdapter;
use apex_adapters::storage::sqlite_storage::SqliteStorage;
use apex_adapters::storage::timescale::TimescaleAdapter;
use crate::commands::python_runtime::RuntimePaths;
use crate::config::{AppConfig, StorageBackendKind};
use apex_core::application::alert_engine::AlertEngine;
use apex_core::application::circuit_breaker::reconcile_on_startup;
use apex_core::application::market_data_aggregator::MarketDataAggregator;
use apex_core::application::metrics::Metrics;
use apex_core::application::order_trade_manager::OrderTradeManager;
use apex_core::application::risk_engine::{RiskConfig, RiskEngine};
use apex_core::bus::message_bus::{BusMessage, MessageBus, Topic};
use apex_core::ports::execution::ExecutionPort;
use apex_core::ports::storage::StoragePort;

fn env_var(name: &str) -> Option<String> {
    let raw = std::env::var(name).ok()?;
    let value = raw.trim();

    if value.is_empty() || (value.starts_with("your_") && value.ends_with("_here")) {
        return None;
    }

    Some(value.to_string())
}

enum ExecutionHandle {
    Zerodha(Arc<ZerodhaExecutionAdapter>),
    AngelOne(Arc<AngelOneExecutionAdapter>),
    Groww(Arc<GrowwExecutionAdapter>),
    Robinhood(Arc<RobinhoodExecutionAdapter>),
    Binance(Arc<BinanceExecutionAdapter>),
    Coinbase(Arc<CoinbaseExecutionAdapter>),
}

impl ExecutionHandle {
    fn is_authenticated(&self) -> bool {
        match self {
            Self::Zerodha(adapter) => adapter.is_authenticated(),
            Self::AngelOne(adapter) => adapter.is_authenticated(),
            Self::Groww(adapter) => adapter.is_authenticated(),
            Self::Robinhood(adapter) => adapter.is_authenticated(),
            Self::Binance(adapter) => adapter.is_authenticated(),
            Self::Coinbase(adapter) => adapter.is_authenticated(),
        }
    }

    fn set_session_token(&self, token: &str) {
        match self {
            Self::Zerodha(adapter) => adapter.set_access_token(token.to_string()),
            Self::AngelOne(adapter) => adapter.set_jwt_token(token.to_string()),
            Self::Groww(adapter) => adapter.set_access_token(token.to_string()),
            Self::Robinhood(adapter) => adapter.set_access_token(token.to_string()),
            Self::Binance(_) => {} // Binance uses API key/secret, not session token
            Self::Coinbase(_) => {} // Coinbase uses API key/secret/passphrase, not session token
        }
    }

    fn clear_session_token(&self) {
        match self {
            Self::Zerodha(adapter) => adapter.clear_access_token(),
            Self::AngelOne(adapter) => adapter.clear_jwt_token(),
            Self::Groww(adapter) => adapter.clear_access_token(),
            Self::Robinhood(adapter) => adapter.clear_access_token(),
            Self::Binance(_) => {}
            Self::Coinbase(_) => {}
        }
    }
}

enum MarketDataHandle {
    Zerodha(Arc<ZerodhaKiteAdapter>),
    AngelOne(Arc<AngelOneMarketDataAdapter>),
    Groww(Arc<GrowwMarketDataAdapter>),
    Robinhood(Arc<RobinhoodMarketDataAdapter>),
    Binance(Arc<BinanceAdapter>),
    Coinbase(Arc<CoinbaseAdapter>),
    Polymarket(Arc<PolymarketAdapter>),
}

impl MarketDataHandle {
    fn is_authenticated(&self) -> bool {
        match self {
            Self::Zerodha(adapter) => adapter.is_authenticated(),
            Self::AngelOne(adapter) => adapter.is_authenticated(),
            Self::Groww(adapter) => adapter.is_authenticated(),
            Self::Robinhood(adapter) => adapter.is_authenticated(),
            Self::Binance(_) => true, // Binance market data doesn't require auth
            Self::Coinbase(_) => true, // Coinbase market data doesn't require auth
            Self::Polymarket(_) => true, // Polymarket market data doesn't require auth
        }
    }

    fn set_session_token(&self, token: &str) {
        match self {
            Self::Zerodha(adapter) => adapter.set_access_token(token.to_string()),
            Self::AngelOne(adapter) => adapter.set_jwt_token(token.to_string()),
            Self::Groww(adapter) => adapter.set_access_token(token.to_string()),
            Self::Robinhood(adapter) => adapter.set_access_token(token.to_string()),
            Self::Binance(_) => {}
            Self::Coinbase(_) => {}
            Self::Polymarket(_) => {}
        }
    }

    fn clear_session_token(&self) {
        match self {
            Self::Zerodha(adapter) => adapter.clear_access_token(),
            Self::AngelOne(adapter) => adapter.clear_jwt_token(),
            Self::Groww(adapter) => adapter.clear_access_token(),
            Self::Robinhood(adapter) => adapter.clear_access_token(),
            Self::Binance(_) => {}
            Self::Coinbase(_) => {}
            Self::Polymarket(_) => {}
        }
    }
}

struct BrokerRuntimeEntry {
    display_name: &'static str,
    mode: &'static str,
    token_label: Option<&'static str>,
    config_hint: &'static str,
    execution_available: bool,
    market_data_available: bool,
    execution: Option<ExecutionHandle>,
    market_data: Option<MarketDataHandle>,
}

impl BrokerRuntimeEntry {
    fn configured(&self) -> bool {
        self.mode == "paper" || self.execution_available || self.market_data_available
    }

    fn authenticated(&self) -> bool {
        if self.mode == "paper" {
            return true;
        }

        let execution_auth = self
            .execution
            .as_ref()
            .map(ExecutionHandle::is_authenticated)
            .unwrap_or(false);
        let market_auth = self
            .market_data
            .as_ref()
            .map(MarketDataHandle::is_authenticated)
            .unwrap_or(false);

        execution_auth || market_auth
    }

    fn capability_summary(&self) -> &'static str {
        match (self.execution_available, self.market_data_available) {
            (true, true) => "live orders, account access, and market data",
            (true, false) => "live orders and account access",
            (false, true) => "live market data",
            (false, false) => "live connectivity",
        }
    }

    fn status_and_message(&self) -> (String, String) {
        if self.mode == "paper" {
            return (
                "ready".to_string(),
                "Paper trading is active for safe simulated execution.".to_string(),
            );
        }

        if !self.configured() {
            return (
                "not_configured".to_string(),
                format!(
                    "Add {} to `.env` and restart the desktop app to enable {}.",
                    self.config_hint, self.display_name
                ),
            );
        }

        if !self.authenticated() {
            let token_label = self.token_label.unwrap_or("session token");
            return (
                "auth_required".to_string(),
                format!(
                    "{} is configured for {}. Paste a {} to enable live connectivity.",
                    self.display_name,
                    self.capability_summary(),
                    token_label
                ),
            );
        }

        (
            "connected".to_string(),
            format!(
                "{} is authenticated and ready for safe live data and account checks.",
                self.display_name
            ),
        )
    }

    fn set_session_token(&self, token: &str) -> Result<()> {
        if self.mode == "paper" {
            return Err(anyhow!("Paper trading does not use a live session token"));
        }

        if !self.configured() {
            return Err(anyhow!(
                "{} is not configured. Add {} to `.env` and restart the desktop app.",
                self.display_name,
                self.config_hint
            ));
        }

        let mut applied = false;

        if let Some(execution) = &self.execution {
            execution.set_session_token(token);
            applied = true;
        }

        if let Some(market_data) = &self.market_data {
            market_data.set_session_token(token);
            applied = true;
        }

        if !applied {
            return Err(anyhow!("{} does not expose a live session surface", self.display_name));
        }

        Ok(())
    }

    fn clear_session_token(&self) -> Result<()> {
        if self.mode == "paper" {
            return Ok(());
        }

        let mut cleared = false;

        if let Some(execution) = &self.execution {
            execution.clear_session_token();
            cleared = true;
        }

        if let Some(market_data) = &self.market_data {
            market_data.clear_session_token();
            cleared = true;
        }

        if !cleared {
            return Err(anyhow!(
                "{} is not configured. Add {} to `.env` and restart the desktop app.",
                self.display_name,
                self.config_hint
            ));
        }

        Ok(())
    }

    fn to_dto(&self, broker_id: &str) -> crate::dto::BrokerConnectionDto {
        let (status, message) = self.status_and_message();

        crate::dto::BrokerConnectionDto {
            broker_id: broker_id.to_string(),
            display_name: self.display_name.to_string(),
            mode: self.mode.to_string(),
            status,
            configured: self.configured(),
            authenticated: self.authenticated(),
            execution_available: self.execution_available,
            market_data_available: self.market_data_available,
            token_field_label: self.token_label.unwrap_or("").to_string(),
            message,
        }
    }
}

/// Shared application state — initialised once at startup, shared across all IPC handlers.
pub struct AppState {
    pub aggregator: Arc<MarketDataAggregator>,
    pub otm: Arc<OrderTradeManager>,
    pub alerts: Arc<AlertEngine>,
    pub risk: Arc<RiskEngine>,
    pub bus: Arc<MessageBus>,
    pub storage: Arc<dyn StoragePort>,
    pub storage_backend: String,
    pub storage_target: String,
    pub metrics: Arc<Metrics>,
    brokers: HashMap<String, BrokerRuntimeEntry>,
    pub started_at: Instant,
}

struct StorageBootstrap {
    storage: Arc<dyn StoragePort>,
    backend: &'static str,
    target: String,
}

impl AppState {
    /// Initialize all application services.
    pub async fn init(runtime_paths: RuntimePaths) -> Result<Self> {
        let bus = Arc::new(MessageBus::new());
        let config = AppConfig::load(&runtime_paths)?;
        let resolved_data_dir = config.resolved_data_dir(&runtime_paths)?;
        let storage = build_storage(&runtime_paths, &config).await?;

        tracing::info!(
            bundled_runtime = runtime_paths.is_bundled(),
            config_path = %runtime_paths.config_file().display(),
            config_dir = %runtime_paths.config_dir().display(),
            data_dir = %resolved_data_dir.display(),
            storage_backend = storage.backend,
            storage_target = %storage.target,
            python_root = %runtime_paths.python_root().display(),
            work_root = %runtime_paths.work_root().display(),
            resource_dir = ?runtime_paths.resource_dir().map(|path| path.display().to_string()),
            "Resolved application runtime paths"
        );

        let risk_config: RiskConfig = config.risk_config();
        let risk = Arc::new(RiskEngine::new(risk_config));

        let mut aggregator_inner = MarketDataAggregator::new(bus.clone());
        aggregator_inner.set_storage(storage.storage.clone());
        let mut otm_inner = OrderTradeManager::new(risk.clone(), bus.clone());
        let mut brokers = HashMap::new();

        let yahoo_adapter = Arc::new(YahooFinanceAdapter::new());
        aggregator_inner.add_adapter(yahoo_adapter);

        // Register paper trading adapter — always available as default execution.
        let paper = Arc::new(PaperTradingAdapter::new());
        otm_inner.register_execution("paper".to_string(), paper);
        brokers.insert(
            "paper".to_string(),
            BrokerRuntimeEntry {
                display_name: "Paper Trading",
                mode: "paper",
                token_label: None,
                config_hint: "",
                execution_available: true,
                market_data_available: false,
                execution: None,
                market_data: None,
            },
        );

        let zerodha_api_key = env_var("ZERODHA_API_KEY");
        let zerodha_access_token = env_var("ZERODHA_ACCESS_TOKEN");
        let mut zerodha_execution = None;
        let mut zerodha_market_data = None;
        let mut zerodha_execution_available = false;
        let mut zerodha_market_data_available = false;

        if let Some(api_key) = zerodha_api_key.clone() {
            match ZerodhaExecutionAdapter::new(api_key.clone(), zerodha_access_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    otm_inner.register_execution("zerodha".to_string(), adapter.clone());
                    zerodha_execution = Some(ExecutionHandle::Zerodha(adapter));
                    zerodha_execution_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Zerodha execution adapter");
                }
            }

            match ZerodhaKiteAdapter::new(api_key, zerodha_access_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    aggregator_inner.add_adapter(adapter.clone());
                    zerodha_market_data = Some(MarketDataHandle::Zerodha(adapter));
                    zerodha_market_data_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Zerodha market data adapter");
                }
            }
        }

        brokers.insert(
            "zerodha".to_string(),
            BrokerRuntimeEntry {
                display_name: "Zerodha Kite",
                mode: "live",
                token_label: Some("Access Token"),
                config_hint: "ZERODHA_API_KEY",
                execution_available: zerodha_execution_available,
                market_data_available: zerodha_market_data_available,
                execution: zerodha_execution,
                market_data: zerodha_market_data,
            },
        );

        let angel_api_key = env_var("ANGEL_ONE_API_KEY");
        let angel_client_code = env_var("ANGEL_ONE_CLIENT_CODE");
        let angel_jwt_token = env_var("ANGEL_ONE_JWT_TOKEN");
        let mut angel_execution = None;
        let mut angel_market_data = None;
        let mut angel_execution_available = false;
        let mut angel_market_data_available = false;

        if let (Some(api_key), Some(client_code)) = (angel_api_key, angel_client_code) {
            match AngelOneExecutionAdapter::new(api_key.clone(), client_code, angel_jwt_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    otm_inner.register_execution("angel_one".to_string(), adapter.clone());
                    angel_execution = Some(ExecutionHandle::AngelOne(adapter));
                    angel_execution_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Angel One execution adapter");
                }
            }

            match AngelOneMarketDataAdapter::new(api_key, angel_jwt_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    aggregator_inner.add_adapter(adapter.clone());
                    angel_market_data = Some(MarketDataHandle::AngelOne(adapter));
                    angel_market_data_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Angel One market data adapter");
                }
            }
        }

        brokers.insert(
            "angel_one".to_string(),
            BrokerRuntimeEntry {
                display_name: "Angel One",
                mode: "live",
                token_label: Some("JWT Token"),
                config_hint: "ANGEL_ONE_API_KEY and ANGEL_ONE_CLIENT_CODE",
                execution_available: angel_execution_available,
                market_data_available: angel_market_data_available,
                execution: angel_execution,
                market_data: angel_market_data,
            },
        );

        let groww_api_key = env_var("GROWW_API_KEY");
        let groww_access_token = env_var("GROWW_ACCESS_TOKEN");
        let mut groww_execution = None;
        let mut groww_market_data = None;
        let mut groww_execution_available = false;
        let mut groww_market_data_available = false;

        if let Some(api_key) = groww_api_key {
            match GrowwExecutionAdapter::new(api_key.clone(), groww_access_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    otm_inner.register_execution("groww".to_string(), adapter.clone());
                    groww_execution = Some(ExecutionHandle::Groww(adapter));
                    groww_execution_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Groww execution adapter");
                }
            }

            match GrowwMarketDataAdapter::new(api_key, groww_access_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    aggregator_inner.add_adapter(adapter.clone());
                    groww_market_data = Some(MarketDataHandle::Groww(adapter));
                    groww_market_data_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Groww market data adapter");
                }
            }
        }

        brokers.insert(
            "groww".to_string(),
            BrokerRuntimeEntry {
                display_name: "Groww",
                mode: "live",
                token_label: Some("Access Token"),
                config_hint: "GROWW_API_KEY",
                execution_available: groww_execution_available,
                market_data_available: groww_market_data_available,
                execution: groww_execution,
                market_data: groww_market_data,
            },
        );

        let robinhood_client_id = env_var("ROBINHOOD_CLIENT_ID");
        let robinhood_access_token = env_var("ROBINHOOD_ACCESS_TOKEN");
        let mut robinhood_execution = None;
        let mut robinhood_market_data = None;
        let mut robinhood_execution_available = false;
        let mut robinhood_market_data_available = false;

        if let Some(client_id) = robinhood_client_id {
            match RobinhoodExecutionAdapter::new(client_id, robinhood_access_token.clone()) {
                Ok(adapter) => {
                    let adapter = Arc::new(adapter);
                    otm_inner.register_execution("robinhood".to_string(), adapter.clone());
                    robinhood_execution = Some(ExecutionHandle::Robinhood(adapter));
                    robinhood_execution_available = true;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to initialize Robinhood execution adapter");
                }
            }
        }

        match RobinhoodMarketDataAdapter::new(robinhood_access_token.clone()) {
            Ok(adapter) => {
                let adapter = Arc::new(adapter);
                aggregator_inner.add_adapter(adapter.clone());
                robinhood_market_data = Some(MarketDataHandle::Robinhood(adapter));
                robinhood_market_data_available = true;
            }
            Err(err) => {
                tracing::warn!(error = %err, "Failed to initialize Robinhood market data adapter");
            }
        }

        brokers.insert(
            "robinhood".to_string(),
            BrokerRuntimeEntry {
                display_name: "Robinhood",
                mode: "live",
                token_label: Some("Access Token"),
                config_hint: "ROBINHOOD_ACCESS_TOKEN (market data) and ROBINHOOD_CLIENT_ID (execution)",
                execution_available: robinhood_execution_available,
                market_data_available: robinhood_market_data_available,
                execution: robinhood_execution,
                market_data: robinhood_market_data,
            },
        );

        // Binance - Crypto exchange
        let binance_api_key = env_var("BINANCE_API_KEY");
        let binance_api_secret = env_var("BINANCE_API_SECRET");
        let mut binance_execution = None;
        let mut binance_market_data = None;
        let mut binance_execution_available = false;
        let mut binance_market_data_available = false;

        if let (Some(api_key), Some(api_secret)) = (binance_api_key, binance_api_secret) {
            let adapter = Arc::new(BinanceExecutionAdapter::new(api_key, api_secret, false));
            otm_inner.register_execution("binance".to_string(), adapter.clone());
            binance_execution = Some(ExecutionHandle::Binance(adapter));
            binance_execution_available = true;
        }

        // Binance market data doesn't require API keys
        let binance_adapter = Arc::new(BinanceAdapter::new());
        aggregator_inner.add_adapter(binance_adapter.clone());
        binance_market_data = Some(MarketDataHandle::Binance(binance_adapter));
        binance_market_data_available = true;

        brokers.insert(
            "binance".to_string(),
            BrokerRuntimeEntry {
                display_name: "Binance",
                mode: "live",
                token_label: Some("API Key"),
                config_hint: "BINANCE_API_KEY and BINANCE_API_SECRET (execution only)",
                execution_available: binance_execution_available,
                market_data_available: binance_market_data_available,
                execution: binance_execution,
                market_data: binance_market_data,
            },
        );

        // Coinbase Pro - Crypto exchange
        let coinbase_api_key = env_var("COINBASE_API_KEY");
        let coinbase_api_secret = env_var("COINBASE_API_SECRET");
        let coinbase_passphrase = env_var("COINBASE_PASSPHRASE");
        let mut coinbase_execution = None;
        let mut coinbase_market_data = None;
        let mut coinbase_execution_available = false;
        let mut coinbase_market_data_available = false;

        if let (Some(api_key), Some(api_secret), Some(passphrase)) = (coinbase_api_key, coinbase_api_secret, coinbase_passphrase) {
            let adapter = Arc::new(CoinbaseExecutionAdapter::new(api_key, api_secret, passphrase));
            otm_inner.register_execution("coinbase".to_string(), adapter.clone());
            coinbase_execution = Some(ExecutionHandle::Coinbase(adapter));
            coinbase_execution_available = true;
        }

        // Coinbase market data doesn't require API keys
        let coinbase_adapter = Arc::new(CoinbaseAdapter::new());
        aggregator_inner.add_adapter(coinbase_adapter.clone());
        coinbase_market_data = Some(MarketDataHandle::Coinbase(coinbase_adapter));
        coinbase_market_data_available = true;

        brokers.insert(
            "coinbase".to_string(),
            BrokerRuntimeEntry {
                display_name: "Coinbase Pro",
                mode: "live",
                token_label: Some("API Key"),
                config_hint: "COINBASE_API_KEY, COINBASE_API_SECRET, and COINBASE_PASSPHRASE (execution only)",
                execution_available: coinbase_execution_available,
                market_data_available: coinbase_market_data_available,
                execution: coinbase_execution,
                market_data: coinbase_market_data,
            },
        );

        // Polymarket - Prediction markets
        let polymarket_adapter = Arc::new(PolymarketAdapter::new());
        aggregator_inner.add_adapter(polymarket_adapter.clone());
        let polymarket_market_data = Some(MarketDataHandle::Polymarket(polymarket_adapter));

        brokers.insert(
            "polymarket".to_string(),
            BrokerRuntimeEntry {
                display_name: "Polymarket",
                mode: "live",
                token_label: None,
                config_hint: "",
                execution_available: false,
                market_data_available: true,
                execution: None,
                market_data: polymarket_market_data,
            },
        );

        let aggregator = Arc::new(aggregator_inner);
        let otm = Arc::new(otm_inner);

        let alerts = Arc::new(AlertEngine::new(bus.clone()));

        // Crash recovery — reconcile stale orders and positions on startup
        let broker_ids = otm.authenticated_broker_ids();
        if !broker_ids.is_empty() {
            match reconcile_on_startup(&otm, &broker_ids).await {
                Ok(report) => {
                    tracing::info!(
                        pending = report.pending_orders,
                        reconciled = report.brokers_reconciled,
                        "Crash recovery completed"
                    );
                    if !report.errors.is_empty() {
                        tracing::warn!("Reconciliation errors: {:?}", report.errors);
                    }
                }
                Err(e) => {
                    tracing::warn!("Crash recovery failed (non-fatal): {}", e);
                }
            }
        }

        // Start periodic position reconciliation (every 30 seconds)
        OrderTradeManager::start_reconciliation_loop(
            otm.clone(),
            std::time::Duration::from_secs(30),
        );

        let metrics = Arc::new(Metrics::new());

        Ok(Self {
            aggregator,
            otm,
            alerts,
            risk,
            bus,
            storage: storage.storage,
            storage_backend: storage.backend.to_string(),
            storage_target: storage.target,
            metrics,
            brokers,
            started_at: Instant::now(),
        })
    }

    pub fn broker_connections(&self) -> Vec<crate::dto::BrokerConnectionDto> {
        let mut connections: Vec<_> = self
            .brokers
            .iter()
            .map(|(broker_id, broker)| broker.to_dto(broker_id))
            .collect();

        connections.sort_by_key(|broker| {
            (
                if broker.mode == "paper" { 0 } else { 1 },
                broker.display_name.clone(),
            )
        });

        connections
    }

    pub fn broker_connection(&self, broker_id: &str) -> Option<crate::dto::BrokerConnectionDto> {
        self.brokers.get(broker_id).map(|broker| broker.to_dto(broker_id))
    }

    pub fn set_broker_session(&self, broker_id: &str, session_token: &str) -> Result<()> {
        let broker = self
            .brokers
            .get(broker_id)
            .ok_or_else(|| anyhow!("Unknown broker: {}", broker_id))?;

        broker.set_session_token(session_token)
    }

    pub fn clear_broker_session(&self, broker_id: &str) -> Result<()> {
        let broker = self
            .brokers
            .get(broker_id)
            .ok_or_else(|| anyhow!("Unknown broker: {}", broker_id))?;

        broker.clear_session_token()
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

async fn build_storage(runtime_paths: &RuntimePaths, config: &AppConfig) -> Result<StorageBootstrap> {
    match config.storage.backend_kind()? {
        StorageBackendKind::Sqlite => {
            let sqlite_path = config.resolved_sqlite_path(runtime_paths)?;
            let sqlite_path_text = sqlite_path.to_string_lossy().to_string();
            let sqlite = SqliteStorage::new_with_options(&sqlite_path_text, config.storage.wal_mode)?;
            sqlite.init_schema().await?;

            Ok(StorageBootstrap {
                storage: Arc::new(sqlite),
                backend: "sqlite",
                target: sqlite_path.display().to_string(),
            })
        }
        StorageBackendKind::Timescale => {
            let connection_url = config.storage.postgres_url().ok_or_else(|| {
                anyhow!(
                    "Storage backend is set to Timescale/Postgres but no connection URL was provided. Set `storage.postgres_url` in {} or define `APEX_POSTGRES_URL`.",
                    runtime_paths.config_file().display()
                )
            })?;

            let timescale = TimescaleAdapter::new(&connection_url).await?;

            Ok(StorageBootstrap {
                storage: Arc::new(timescale),
                backend: "timescale",
                target: redact_connection_url(&connection_url),
            })
        }
    }
}

fn redact_connection_url(connection_url: &str) -> String {
    let Some((scheme, rest)) = connection_url.split_once("://") else {
        return "configured via postgres_url".to_string();
    };

    let host_part = rest.rsplit('@').next().unwrap_or(rest);
    format!("{scheme}://{}", host_part)
}
