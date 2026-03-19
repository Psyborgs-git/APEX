-- Migration 001: Core Tables
-- Creates the foundational tables required for APEX Terminal.
-- Compatible with both SQLite (local) and TimescaleDB (production).

-- ── Orders ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS orders (
    id            TEXT PRIMARY KEY,
    symbol        TEXT    NOT NULL,
    side          TEXT    NOT NULL CHECK (side IN ('Buy', 'Sell')),
    order_type    TEXT    NOT NULL CHECK (order_type IN ('Market', 'Limit', 'Stop', 'StopLimit')),
    quantity      REAL    NOT NULL CHECK (quantity > 0),
    price         REAL,
    stop_price    REAL,
    status        TEXT    NOT NULL DEFAULT 'Pending',
    filled_qty    REAL    NOT NULL DEFAULT 0,
    avg_price     REAL    NOT NULL DEFAULT 0,
    broker_id     TEXT    NOT NULL,
    source        TEXT    NOT NULL DEFAULT 'manual',
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_orders_symbol   ON orders (symbol);
CREATE INDEX IF NOT EXISTS idx_orders_status   ON orders (status);
CREATE INDEX IF NOT EXISTS idx_orders_created  ON orders (created_at);

-- ── Trades (fills) ──────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS trades (
    id            TEXT PRIMARY KEY,
    order_id      TEXT    NOT NULL REFERENCES orders(id),
    symbol        TEXT    NOT NULL,
    side          TEXT    NOT NULL,
    quantity      REAL    NOT NULL CHECK (quantity > 0),
    price         REAL    NOT NULL,
    commission    REAL    NOT NULL DEFAULT 0,
    executed_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_trades_order    ON trades (order_id);
CREATE INDEX IF NOT EXISTS idx_trades_symbol   ON trades (symbol);

-- ── OHLCV bars ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ohlcv (
    symbol        TEXT    NOT NULL,
    timeframe     TEXT    NOT NULL,
    time          TEXT    NOT NULL,
    open          REAL    NOT NULL,
    high          REAL    NOT NULL,
    low           REAL    NOT NULL,
    close         REAL    NOT NULL,
    volume        REAL    NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, timeframe, time)
);
