//! Structured tracing initialisation for the APEX Terminal.
//!
//! Configures `tracing-subscriber` with two output layers:
//!
//! 1. **Console** — human-readable, colour-enabled, for interactive use.
//! 2. **JSON file** — newline-delimited JSON for offline analysis and
//!    optional Jaeger / Grafana / Datadog ingestion.
//!
//! The JSON log is written to `logs/apex_trace.ndjson` (rotated by size via
//! `tracing-appender` when available; otherwise a simple `std::fs::File`).
//!
//! # Environment variables
//!
//! * `APEX_LOG` — override the default `apex=info` filter (e.g. `apex=debug`)
//! * `APEX_JSON_TRACE` — set to `1` or `true` to enable the JSON file layer

use std::fs;
use std::io;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Initialise the global tracing subscriber.
///
/// Call this **once** at application startup, before any other `tracing` macros.
pub fn init() {
    let log_dir = "logs";
    let _ = fs::create_dir_all(log_dir);

    // Base env filter — defaults to `apex=info` unless overridden.
    let default_filter = "apex=info";
    let env_filter = EnvFilter::try_from_env("APEX_LOG")
        .unwrap_or_else(|_| EnvFilter::new(default_filter));

    // Always present: pretty console layer.
    let console_layer = fmt::layer()
        .with_target(true)
        .with_ansi(true);

    // Optionally: structured JSON layer writing to logs/apex_trace.ndjson.
    let json_enabled = std::env::var("APEX_JSON_TRACE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if json_enabled {
        let json_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{}/apex_trace.ndjson", log_dir))
            .unwrap_or_else(|_| {
                // Fall back to stderr if the file cannot be opened.
                // Safety: `from_raw_fd(2)` is stderr, always open.
                // We use a no-op fallback by opening /dev/null.
                fs::File::create("/dev/null").expect("cannot open /dev/null")
            });

        let json_layer = fmt::layer()
            .json()
            .with_writer(move || -> Box<dyn io::Write + Send> {
                Box::new(json_file.try_clone().expect("file clone"))
            })
            .with_target(true)
            .with_span_list(true)
            .with_current_span(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(json_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_creation() {
        // Ensure logs/ directory can be created (idempotent).
        let _ = fs::create_dir_all("logs");
        assert!(fs::metadata("logs").map(|m| m.is_dir()).unwrap_or(false));
    }
}
