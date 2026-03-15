use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by a circuit-breaker-wrapped call.
#[derive(Debug)]
pub enum CbError<E> {
    /// The circuit is open — calls are being rejected without executing.
    CircuitOpen,
    /// The inner function returned an error.
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CbError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CbError::CircuitOpen => write!(f, "circuit breaker is open"),
            CbError::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CbError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CbError::CircuitOpen => None,
            CbError::Inner(e) => Some(e),
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Internal state of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    /// Normal operation — calls are forwarded.
    Closed,
    /// Too many failures — calls are rejected immediately.
    Open,
    /// Trial phase — a limited number of calls are allowed through.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

/// A reusable circuit breaker that wraps an inner service `T`.
///
/// The breaker transitions between **Closed → Open → HalfOpen → Closed**
/// based on failure / success counts and a configurable timeout.
pub struct CircuitBreaker<T> {
    inner: T,
    state: Arc<RwLock<CbState>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure: Mutex<Option<Instant>>,
}

impl<T> CircuitBreaker<T> {
    /// Create a new circuit breaker wrapping `inner`.
    pub fn new(inner: T, failure_threshold: u32, success_threshold: u32, timeout: Duration) -> Self {
        Self {
            inner,
            state: Arc::new(RwLock::new(CbState::Closed)),
            failure_threshold,
            success_threshold,
            timeout,
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure: Mutex::new(None),
        }
    }

    /// Current state of the breaker.
    pub async fn state(&self) -> CbState {
        *self.state.read().await
    }

    /// Reference to the wrapped inner service.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Execute `f` through the circuit breaker.
    ///
    /// - **Closed**: the call is forwarded. Failures increment the counter; if
    ///   the threshold is reached the breaker opens.
    /// - **Open**: if the timeout has elapsed the breaker moves to HalfOpen and
    ///   the call is allowed; otherwise `CbError::CircuitOpen` is returned.
    /// - **HalfOpen**: the call is forwarded. A success increments the success
    ///   counter; when the threshold is reached the breaker closes. A failure
    ///   re-opens the breaker immediately.
    #[tracing::instrument(skip_all, fields(cb_state))]
    pub async fn call<'a, R, E>(
        &'a self,
        f: impl FnOnce(&'a T) -> Pin<Box<dyn Future<Output = Result<R, E>> + Send + 'a>>,
    ) -> Result<R, CbError<E>> {
        // Determine whether the call is allowed (and potentially transition).
        self.maybe_transition_to_half_open().await;

        let current = *self.state.read().await;
        if current == CbState::Open {
            warn!("Circuit breaker is OPEN — rejecting call");
            return Err(CbError::CircuitOpen);
        }

        // Execute the inner function.
        match f(&self.inner).await {
            Ok(val) => {
                self.record_success().await;
                Ok(val)
            }
            Err(e) => {
                self.record_failure().await;
                Err(CbError::Inner(e))
            }
        }
    }

    // -- Internal helpers --------------------------------------------------

    /// If we are Open and the timeout has elapsed, move to HalfOpen.
    async fn maybe_transition_to_half_open(&self) {
        let current = *self.state.read().await;
        if current != CbState::Open {
            return;
        }

        let last = self.last_failure.lock().await;
        if let Some(ts) = *last {
            if ts.elapsed() >= self.timeout {
                drop(last); // release mutex before write lock
                let mut state = self.state.write().await;
                if *state == CbState::Open {
                    info!("Circuit breaker transitioning Open → HalfOpen");
                    *state = CbState::HalfOpen;
                    self.success_count.store(0, Ordering::SeqCst);
                }
            }
        }
    }

    async fn record_success(&self) {
        let current = *self.state.read().await;
        match current {
            CbState::HalfOpen => {
                let prev = self.success_count.fetch_add(1, Ordering::SeqCst);
                if prev + 1 >= self.success_threshold {
                    let mut state = self.state.write().await;
                    info!("Circuit breaker transitioning HalfOpen → Closed");
                    *state = CbState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                }
            }
            CbState::Closed => {
                // Reset failure count on success while closed.
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CbState::Open => {} // should not happen
        }
    }

    async fn record_failure(&self) {
        let current = *self.state.read().await;
        match current {
            CbState::Closed => {
                let prev = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if prev + 1 >= self.failure_threshold {
                    let mut state = self.state.write().await;
                    warn!("Circuit breaker transitioning Closed → Open (threshold reached)");
                    *state = CbState::Open;
                    *self.last_failure.lock().await = Some(Instant::now());
                }
            }
            CbState::HalfOpen => {
                let mut state = self.state.write().await;
                warn!("Circuit breaker transitioning HalfOpen → Open (failure in half-open)");
                *state = CbState::Open;
                self.success_count.store(0, Ordering::SeqCst);
                *self.last_failure.lock().await = Some(Instant::now());
            }
            CbState::Open => {} // should not happen
        }
    }
}

// ---------------------------------------------------------------------------
// Crash recovery helper
// ---------------------------------------------------------------------------

/// Reconcile pending / open orders on startup.
///
/// This is meant to be called once during application boot so that orders
/// left in an intermediate state (e.g. after a crash) are brought back in
/// sync with the broker's view of the world.
#[tracing::instrument(skip(otm))]
pub async fn reconcile_on_startup(
    otm: &crate::application::order_trade_manager::OrderTradeManager,
    broker_ids: &[String],
) -> anyhow::Result<ReconcileReport> {
    let mut report = ReconcileReport::default();

    // Snapshot pending/open orders before reconciliation.
    let open = otm.open_orders();
    report.pending_orders = open
        .iter()
        .filter(|o| {
            o.status == crate::domain::models::OrderStatus::Pending
                || o.status == crate::domain::models::OrderStatus::Open
                || o.status == crate::domain::models::OrderStatus::PartiallyFilled
        })
        .count();

    // Reconcile positions for each registered broker.
    for broker_id in broker_ids {
        match otm.reconcile_positions(broker_id).await {
            Ok(()) => {
                info!(broker_id = %broker_id, "Position reconciliation succeeded");
                report.brokers_reconciled += 1;
            }
            Err(e) => {
                warn!(broker_id = %broker_id, error = %e, "Position reconciliation failed");
                report.errors.push(format!("{broker_id}: {e}"));
            }
        }
    }

    Ok(report)
}

/// Summary returned by [`reconcile_on_startup`].
#[derive(Debug, Default)]
pub struct ReconcileReport {
    /// Number of orders that were in a pending / open state.
    pub pending_orders: usize,
    /// Number of brokers successfully reconciled.
    pub brokers_reconciled: usize,
    /// Errors encountered during reconciliation.
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32 as StdAtomicU32;

    /// Trivial inner service for testing.
    struct DummyService {
        call_count: StdAtomicU32,
    }

    impl DummyService {
        fn new() -> Self {
            Self {
                call_count: StdAtomicU32::new(0),
            }
        }
        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_closed_passes_through() {
        let cb = CircuitBreaker::new(DummyService::new(), 3, 2, Duration::from_secs(1));

        let result: Result<i32, CbError<String>> = cb
            .call(|svc| Box::pin(async move {
                svc.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            }))
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(cb.inner().calls(), 1);
        assert_eq!(cb.state().await, CbState::Closed);
    }

    #[tokio::test]
    async fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(DummyService::new(), 2, 1, Duration::from_secs(60));

        // Two failures should trip the breaker.
        for _ in 0..2 {
            let _: Result<(), CbError<String>> = cb
                .call(|_| Box::pin(async { Err("boom".to_string()) }))
                .await;
        }

        assert_eq!(cb.state().await, CbState::Open);

        // Next call should be rejected without executing.
        let result: Result<(), CbError<String>> = cb
            .call(|svc| Box::pin(async move {
                svc.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
            .await;

        assert!(matches!(result, Err(CbError::CircuitOpen)));
        assert_eq!(cb.inner().calls(), 0); // inner was never called
    }

    #[tokio::test]
    async fn test_half_open_after_timeout() {
        let cb = CircuitBreaker::new(
            DummyService::new(),
            1,
            1,
            Duration::from_millis(50), // short timeout for test
        );

        // Trip the breaker.
        let _: Result<(), CbError<String>> = cb
            .call(|_| Box::pin(async { Err("fail".to_string()) }))
            .await;
        assert_eq!(cb.state().await, CbState::Open);

        // Wait for timeout.
        tokio::time::sleep(Duration::from_millis(80)).await;

        // Next call should go through (HalfOpen) and succeed → Closed.
        let result: Result<i32, CbError<String>> = cb
            .call(|svc| Box::pin(async move {
                svc.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(7)
            }))
            .await;

        assert_eq!(result.unwrap(), 7);
        assert_eq!(cb.state().await, CbState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_failure_reopens() {
        let cb = CircuitBreaker::new(DummyService::new(), 1, 2, Duration::from_millis(50));

        // Trip the breaker.
        let _: Result<(), CbError<String>> = cb
            .call(|_| Box::pin(async { Err("fail".to_string()) }))
            .await;
        assert_eq!(cb.state().await, CbState::Open);

        // Wait for timeout.
        tokio::time::sleep(Duration::from_millis(80)).await;

        // Fail again in HalfOpen → should reopen.
        let _: Result<(), CbError<String>> = cb
            .call(|_| Box::pin(async { Err("still broken".to_string()) }))
            .await;

        assert_eq!(cb.state().await, CbState::Open);
    }

    #[tokio::test]
    async fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new(DummyService::new(), 3, 1, Duration::from_secs(60));

        // Two failures, then a success should reset the failure count.
        for _ in 0..2 {
            let _: Result<(), CbError<String>> = cb
                .call(|_| Box::pin(async { Err("fail".to_string()) }))
                .await;
        }
        assert_eq!(cb.state().await, CbState::Closed); // threshold is 3

        let _: Result<(), CbError<String>> = cb.call(|_| Box::pin(async { Ok(()) })).await;

        // Now two more failures should not trip because counter was reset.
        for _ in 0..2 {
            let _: Result<(), CbError<String>> = cb
                .call(|_| Box::pin(async { Err("fail".to_string()) }))
                .await;
        }
        assert_eq!(cb.state().await, CbState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_needs_multiple_successes() {
        let cb = CircuitBreaker::new(DummyService::new(), 1, 3, Duration::from_millis(50));

        // Trip the breaker.
        let _: Result<(), CbError<String>> = cb
            .call(|_| Box::pin(async { Err("fail".to_string()) }))
            .await;

        tokio::time::sleep(Duration::from_millis(80)).await;

        // First success in HalfOpen — still HalfOpen (need 3).
        let _: Result<(), CbError<String>> = cb.call(|_| Box::pin(async { Ok(()) })).await;
        assert_eq!(cb.state().await, CbState::HalfOpen);

        // Second success — still HalfOpen.
        let _: Result<(), CbError<String>> = cb.call(|_| Box::pin(async { Ok(()) })).await;
        assert_eq!(cb.state().await, CbState::HalfOpen);

        // Third success — should close.
        let _: Result<(), CbError<String>> = cb.call(|_| Box::pin(async { Ok(()) })).await;
        assert_eq!(cb.state().await, CbState::Closed);
    }

    #[test]
    fn test_cb_error_display() {
        let open: CbError<String> = CbError::CircuitOpen;
        assert_eq!(format!("{open}"), "circuit breaker is open");

        let inner: CbError<String> = CbError::Inner("db down".into());
        assert_eq!(format!("{inner}"), "db down");
    }
}
