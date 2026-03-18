-- Migration 003: TimescaleDB Extensions (Production Only)
-- These commands are PostgreSQL/TimescaleDB-specific.
-- Run this migration only when deploying to TimescaleDB.
-- For local SQLite development, skip this file.

-- Enable TimescaleDB extension
-- CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;

-- Convert ohlcv to hypertable for time-series optimisation
-- SELECT create_hypertable('ohlcv', 'time', if_not_exists => TRUE);

-- Add compression policy (compress chunks older than 7 days)
-- ALTER TABLE ohlcv SET (
--     timescaledb.compress,
--     timescaledb.compress_segmentby = 'symbol,timeframe'
-- );
-- SELECT add_compression_policy('ohlcv', INTERVAL '7 days', if_not_exists => true);

-- Add retention policy (auto-delete M1 bars older than 90 days)
-- SELECT add_retention_policy('ohlcv', INTERVAL '90 days', if_not_exists => true);

-- Create continuous aggregate for daily summaries
-- CREATE MATERIALIZED VIEW IF NOT EXISTS ohlcv_daily
-- WITH (timescaledb.continuous) AS
-- SELECT
--     symbol,
--     time_bucket('1 day', time::timestamptz) AS bucket,
--     first(open, time::timestamptz) AS open,
--     max(high) AS high,
--     min(low) AS low,
--     last(close, time::timestamptz) AS close,
--     sum(volume) AS volume
-- FROM ohlcv
-- WHERE timeframe = 'M1'
-- GROUP BY symbol, bucket;
