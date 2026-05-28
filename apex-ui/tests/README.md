# Playwright E2E Tests for APEX Trading Terminal

This directory contains comprehensive end-to-end tests for the APEX trading terminal using Playwright.

## Test Suites

### 1. Order Placement and Execution (`order-placement.spec.ts`)
Tests the complete order placement flow including:
- Displaying order entry panel
- Entering order details
- Validating order inputs
- Placing limit and market orders
- Viewing executed orders in positions panel
- Cancelling pending orders

### 2. Account Switching (`account-switching.spec.ts`)
Tests trader account management including:
- Displaying account selector
- Listing available trading accounts
- Switching between accounts
- Displaying account balances
- Persisting account selection
- Updating positions when switching accounts
- Showing connection status

### 3. Custom ML Pipeline Execution (`ml-pipeline.spec.ts`)
Tests the Strategy IDE and ML pipeline including:
- Displaying StrategyIDE component
- Creating new strategy files
- Editing strategy code
- Executing strategies and displaying results
- Displaying execution errors
- Routing output to correct panels
- Saving strategy files
- Loading existing strategies
- Displaying ML pipeline status

### 4. Trade Automation Features (`trade-automation.spec.ts`)
Tests comprehensive trading automation including:
- Watchlist CRUD operations (add/remove symbols)
- Real-time price updates
- Workspace layout save/load
- Alert notifications
- Chart visualizations (Candle, OrderBook, Vector)
- Real-time chart updates
- Panel resizing
- Data persistence across reloads
- Settings panel configuration

## Running Tests

```bash
# Run all tests headless
bun run test

# Run tests with UI
bun run test:ui

# Run tests in headed mode (see browser)
bun run test:headed

# Run specific test file
bunx playwright test order-placement.spec.ts

# Run tests in debug mode
bunx playwright test --debug
```

## Configuration

The test configuration is in `playwright.config.ts` and includes:
- Test directory: `./tests`
- Base URL: `http://localhost:1420`
- Browser: Chromium
- Automatic browser-mode Vite dev server startup (`bun run dev --host 127.0.0.1 --port 1420`)
- Trace on first retry
- HTML reporter

## Requirements

- Playwright browsers installed (`bunx playwright install`)
- All dependencies installed (`cd apex-ui && bun install`)

No native Tauri window is required for this suite. The Playwright config boots a
browser-only Vite app on port `1420`, and the frontend uses mocked Tauri IPC
plus polling in that mode.

For a separate desktop smoke test, use one of these verified launchers:

```bash
# From the repo root
bun run dev

# From apex-ui (delegates to the same root launcher)
bun run tauri:dev
```

## Test Data IDs

Tests rely on `data-testid` attributes in components. Ensure components include appropriate test IDs for:
- Form inputs and buttons
- Panels and containers
- Data display elements
- Navigation elements

## Notes

- Tests are designed to work with browser-mode mock data and deterministic IPC responses
- Some tests may require specific setup or fixtures
- The desktop shell is validated separately from Playwright
- As of the latest verification pass, the full Playwright suite completes with **67 passing tests**
