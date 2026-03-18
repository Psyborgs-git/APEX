# Changelog

All notable changes to this project will be documented in this file.

The format is based on "Keep a Changelog" and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **Tauri IPC Commands**: `get_ohlcv`, `get_account_balance`, `modify_order`, `get_alert_rules`, `get_historical_data`, `get_watchlist_symbols` (6 new commands, 16 total)
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
