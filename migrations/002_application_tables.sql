-- Migration 002: Application Tables
-- Alert rules, strategy runs, and ML model registry.

-- ── Alert rules ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS alert_rules (
    rule_id       TEXT PRIMARY KEY,
    rule_json     TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    is_active     INTEGER NOT NULL DEFAULT 1
);

-- ── Strategy runs ───────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS strategy_runs (
    id            TEXT PRIMARY KEY,
    strategy_name TEXT    NOT NULL,
    script_path   TEXT    NOT NULL,
    status        TEXT    NOT NULL DEFAULT 'pending',
    started_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    stopped_at    TEXT,
    params_json   TEXT
);

CREATE INDEX IF NOT EXISTS idx_strategy_runs_status ON strategy_runs (status);

-- ── ML model registry ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ml_models (
    id            TEXT PRIMARY KEY,
    algorithm     TEXT    NOT NULL,
    model_path    TEXT    NOT NULL,
    metadata_path TEXT    NOT NULL,
    metrics_json  TEXT,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_ml_models_algorithm ON ml_models (algorithm);
