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
