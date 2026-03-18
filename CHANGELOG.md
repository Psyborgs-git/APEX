# Changelog

All notable changes to this project will be documented in this file.

The format is based on "Keep a Changelog" and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **CommandBar Execution Wiring**: Full command execution — order placement (BUY/SELL), symbol navigation, panel switching (:ML, :HEALTH, :STRATEGY, :CHART) — with success/error feedback display
- **Tauri Event Bridge Hooks**: `useQuoteStream`, `useOrderStream`, `usePositionStream`, `useHealthStream` real-time event hooks with automatic Tauri/browser fallback
- **Hot Path Profiling**: `#[tracing::instrument]` spans on `MarketDataAggregator::start`, `OrderTradeManager::submit_order/cancel_order/handle_fill/reconcile_positions`, `RiskEngine::check` with selective field recording
- **Database Migrations**: 3 SQL migration files — `001_core_tables.sql` (orders, trades, ohlcv), `002_application_tables.sql` (alert_rules, strategy_runs, ml_models), `003_timescaledb_extensions.sql` (hypertable, compression, retention)
- **Production Bundle Config**: Tauri bundle updated with Python sidecar resources, migration files, and Python3 system dependency for Linux
- **Playwright E2E Tests**: 9 new command bar tests (65 total, all passing)
- **ML Workbench UI**: Full training dashboard with algorithm selection (Random Forest, Gradient Boosting, Logistic Regression, XGBoost), feature selection chips, CV split configuration, lag period settings, and model registry with metrics display
- **ML Zustand Store**: Centralized state management for ML models, training status, and error handling
- **Health Monitor UI**: System health dashboard with adapter status indicators, uptime/memory/subscription metrics, and real-time health polling (5s intervals)
- **Health Zustand Store**: Centralized state management for system health data
- **Walk-Forward Backtest Engine**: Rolling train/test window validation with configurable n_windows and train_pct, overfitting ratio calculation, and aggregate test metrics
- **WalkForwardConfig/WalkForwardResult**: Full configuration and result types for walk-forward analysis
- **BacktestMetrics Default**: Default implementation for zero-initialized backtest metrics
- **Tauri IPC Commands**: `list_ml_models`, `train_ml_model`, `delete_ml_model`, `get_system_health` (4 new commands, 20 total)
- **ML DTOs**: MLModelDto, MLTrainingRequestDto, MLTrainingResultDto for frontend–backend data transfer
- **Health DTOs**: AdapterHealthDto, SystemHealthDto for health monitoring data transfer
- **ModelRegistry State**: In-memory ML model registry managed by Tauri for IPC access
- **Workspace Tabs**: Added ML Workbench and Health Monitor as new center-column tabs alongside Chart and Strategy IDE
- **Playwright Tests**: 29 new E2E tests — 13 ML Workbench + 7 Health Monitor + 9 CommandBar (65 total)
- **Rust Tests**: 6 new walk-forward backtest unit tests (164 total, all passing)
- **Tauri IPC Commands**: `get_ohlcv`, `get_account_balance`, `modify_order`, `get_alert_rules`, `get_historical_data`, `get_watchlist_symbols` (6 prior new commands)
- **Real-time Event Push**: Message bus → Tauri emitter for 7 event types (quotes, orders, positions, news, alerts, strategy signals, adapter health)
- **Paper Trading Registration**: Paper trading adapter auto-registered as default execution adapter on startup
- **Data Command Module**: New `apex-tauri/src/commands/data.rs` for historical data and watchlist queries
- **Strategy IDE Tab**: Added Chart/Strategy IDE tab switching to workspace layout
- **UI Test IDs**: Added `data-testid` attributes to OrderEntry, PositionsPanel, and StrategyIDE components for Playwright testing
- **StrategyIDE Enhancements**: File creation dialog with name input, save button with confirmation, pipeline status indicator
- **Angel One Adapters**: Full `ExecutionPort` + `MarketDataPort` implementations against SmartAPI (`apiconnect.angelone.in`) with JWT auth, symbol/order mappings, 3s polling subscription
- **Groww Adapters**: Full `ExecutionPort` + `MarketDataPort` implementations against Groww REST API with order lifecycle, position/balance queries, polling subscription
- **Robinhood Adapters**: Full `ExecutionPort` + `MarketDataPort` implementations against Robinhood API with OAuth bearer auth, order lifecycle (cancel-replace pattern for modifications), polling subscription
- **Backtest Engine**: Event-driven bar replay across multiple symbols with merged timeline, configurable slippage/commission (bps), full metrics suite (Sharpe ratio, max drawdown, profit factor, win rate, equity curve, consecutive win/loss tracking)
- **Technical Indicator Library**: Pure Rust implementations of SMA, EMA, RSI, MACD, Bollinger Bands, ATR, VWAP, Stochastic Oscillator, Standard Deviation, Rate of Change — all with comprehensive unit tests
- **Market Scanner**: Real-time symbol screening engine with price/volume/indicator-based criteria (RSI, SMA crossover), AND logic across filters
- **Historical Data Downloader**: Bulk OHLCV data download from Yahoo Finance with CSV storage and reload capability, rate limiting, progress tracking
- **Crash Recovery**: `reconcile_on_startup()` called during Tauri app bootstrap
- **Position Reconciliation Loop**: 30s periodic position reconciliation across all registered brokers

### Changed
- **Workspace Layout**: Center column now supports tab switching between Chart+OrderEntry and StrategyIDE views
- **Playwright Tests**: Rewrote order-placement, ml-pipeline, and account-switching test suites to match actual UI (36 tests total, all passing)
- **Playwright Config**: Set `reuseExistingServer: true` for faster local test runs

### Fixed
- **apex-core Cargo.toml**: Added missing `reqwest` workspace dependency
- **apex-adapters/redis_state.rs**: Fixed `flush_all()` never-type fallback error with explicit `query_async::<()>()` type annotation
- **apex-adapters/timescale.rs**: Fixed `query_orders()` future Send safety by adding `Send` bound to `ToSql` trait objects
- **apex-tauri/commands/data.rs**: Added missing `Symbol` import
- **apex-tauri/commands/orders.rs**: Fixed unused variable warnings with underscore prefixes

## [0.1.0] - 2026-03-17

### Added
- Initial release.
