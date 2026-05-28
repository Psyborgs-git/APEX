use prometheus::{Gauge, Histogram, IntCounter, IntGauge, Registry};

/// Application metrics
pub struct Metrics {
    pub registry: Registry,

    // Market data metrics
    pub ticks_received: IntCounter,
    pub ticks_processed: IntCounter,
    pub ticks_rejected: IntCounter,
    pub quote_cache_size: IntGauge,
    pub adapter_health: Gauge,

    // Execution metrics
    pub orders_submitted: IntCounter,
    pub orders_filled: IntCounter,
    pub orders_cancelled: IntCounter,
    pub orders_rejected: IntCounter,
    pub position_count: IntGauge,

    // Storage metrics
    pub ticks_written: IntCounter,
    pub ticks_write_errors: IntCounter,
    pub storage_latency: Histogram,

    // System metrics
    pub active_adapters: IntGauge,
    pub active_strategies: IntGauge,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        // Market data metrics
        let ticks_received = IntCounter::new(
            "apex_ticks_received_total",
            "Total number of ticks received from adapters"
        ).unwrap();
        let ticks_processed = IntCounter::new(
            "apex_ticks_processed_total",
            "Total number of ticks processed successfully"
        ).unwrap();
        let ticks_rejected = IntCounter::new(
            "apex_ticks_rejected_total",
            "Total number of ticks rejected by data quality checks"
        ).unwrap();
        let quote_cache_size = IntGauge::new(
            "apex_quote_cache_size",
            "Current size of the quote cache"
        ).unwrap();
        let adapter_health = Gauge::new(
            "apex_adapter_health",
            "Health status of adapters (1=healthy, 0.5=degraded, 0=unhealthy)"
        ).unwrap();

        // Execution metrics
        let orders_submitted = IntCounter::new(
            "apex_orders_submitted_total",
            "Total number of orders submitted"
        ).unwrap();
        let orders_filled = IntCounter::new(
            "apex_orders_filled_total",
            "Total number of orders filled"
        ).unwrap();
        let orders_cancelled = IntCounter::new(
            "apex_orders_cancelled_total",
            "Total number of orders cancelled"
        ).unwrap();
        let orders_rejected = IntCounter::new(
            "apex_orders_rejected_total",
            "Total number of orders rejected"
        ).unwrap();
        let position_count = IntGauge::new(
            "apex_position_count",
            "Current number of open positions"
        ).unwrap();

        // Storage metrics
        let ticks_written = IntCounter::new(
            "apex_ticks_written_total",
            "Total number of ticks written to storage"
        ).unwrap();
        let ticks_write_errors = IntCounter::new(
            "apex_ticks_write_errors_total",
            "Total number of tick write errors"
        ).unwrap();
        let storage_latency = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "apex_storage_latency_seconds",
                "Storage operation latency in seconds"
            ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
        ).unwrap();

        // System metrics
        let active_adapters = IntGauge::new(
            "apex_active_adapters",
            "Number of active adapters"
        ).unwrap();
        let active_strategies = IntGauge::new(
            "apex_active_strategies",
            "Number of active strategies"
        ).unwrap();

        // Register all metrics
        registry.register(Box::new(ticks_received.clone())).unwrap();
        registry.register(Box::new(ticks_processed.clone())).unwrap();
        registry.register(Box::new(ticks_rejected.clone())).unwrap();
        registry.register(Box::new(quote_cache_size.clone())).unwrap();
        registry.register(Box::new(adapter_health.clone())).unwrap();
        registry.register(Box::new(orders_submitted.clone())).unwrap();
        registry.register(Box::new(orders_filled.clone())).unwrap();
        registry.register(Box::new(orders_cancelled.clone())).unwrap();
        registry.register(Box::new(orders_rejected.clone())).unwrap();
        registry.register(Box::new(position_count.clone())).unwrap();
        registry.register(Box::new(ticks_written.clone())).unwrap();
        registry.register(Box::new(ticks_write_errors.clone())).unwrap();
        registry.register(Box::new(storage_latency.clone())).unwrap();
        registry.register(Box::new(active_adapters.clone())).unwrap();
        registry.register(Box::new(active_strategies.clone())).unwrap();

        Self {
            registry,
            ticks_received,
            ticks_processed,
            ticks_rejected,
            quote_cache_size,
            adapter_health,
            orders_submitted,
            orders_filled,
            orders_cancelled,
            orders_rejected,
            position_count,
            ticks_written,
            ticks_write_errors,
            storage_latency,
            active_adapters,
            active_strategies,
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
