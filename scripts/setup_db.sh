#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# APEX Terminal — Database Setup Script
# Creates the SQLite database with the required schema.
# Usage:  ./scripts/setup_db.sh [--data-dir DIR]
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

DATA_DIR="data"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--data-dir DIR]"
            echo "  --data-dir DIR  Directory for the SQLite database (default: data)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

DB_PATH="${DATA_DIR}/apex.db"

echo "╔══════════════════════════════════════╗"
echo "║    APEX — Database Setup             ║"
echo "╚══════════════════════════════════════╝"
echo ""
echo "Data directory : ${DATA_DIR}"
echo "Database path  : ${DB_PATH}"
echo ""

# Create data directory
mkdir -p "${DATA_DIR}"

# Check for sqlite3
if ! command -v sqlite3 &>/dev/null; then
    echo "Error: sqlite3 is not installed." >&2
    echo "Install it with:  sudo apt install sqlite3  (Debian/Ubuntu)" >&2
    echo "                   brew install sqlite3      (macOS)" >&2
    exit 1
fi

# Create schema
echo "Creating database schema..."

sqlite3 "${DB_PATH}" <<'SQL'
-- Enable WAL mode for concurrent reads
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ── Orders ──────────────────────────────────────────────────────────
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

-- ── Trades (fills) ──────────────────────────────────────────────────
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

-- ── OHLCV bars ──────────────────────────────────────────────────────
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

-- ── Alert rules ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS alert_rules (
    rule_id       TEXT PRIMARY KEY,
    rule_json     TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    is_active     INTEGER NOT NULL DEFAULT 1
);

-- ── Strategy runs ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS strategy_runs (
    id            TEXT PRIMARY KEY,
    strategy_name TEXT    NOT NULL,
    script_path   TEXT    NOT NULL,
    status        TEXT    NOT NULL DEFAULT 'pending',
    started_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    stopped_at    TEXT,
    params_json   TEXT
);

-- ── ML model registry ───────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ml_models (
    id            TEXT PRIMARY KEY,
    algorithm     TEXT    NOT NULL,
    model_path    TEXT    NOT NULL,
    metadata_path TEXT    NOT NULL,
    metrics_json  TEXT,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

SQL

echo "✓ Database schema created successfully."
echo ""

# Create auxiliary directories
mkdir -p "${DATA_DIR}/../logs"
mkdir -p "${DATA_DIR}/../models"
mkdir -p "${DATA_DIR}/../strategies"

echo "✓ Auxiliary directories created (logs/, models/, strategies/)."
echo ""
echo "Done! Database ready at: ${DB_PATH}"
