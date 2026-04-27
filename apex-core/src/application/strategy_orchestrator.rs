use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::domain::models::*;

/// Strategy orchestrator manages Python strategy scripts
///
/// Responsibilities:
/// - Load and start strategy scripts
/// - Monitor strategy health
/// - Collect strategy metrics
/// - Route signals to Order & Trade Manager
/// - Handle hot-reload on file changes
pub struct StrategyOrchestrator {
    strategies:        Arc<DashMap<String, RunningStrategy>>,
    strategy_dir:      PathBuf,
    python_executable: PathBuf,
    signal_tx:         mpsc::UnboundedSender<TradingSignal>,
}

/// A running strategy instance
struct RunningStrategy {
    id:             String,
    name:           String,
    script_path:    PathBuf,
    process:        Option<Child>,
    status:         StrategyStatus,
    metrics:        StrategyMetrics,
    resource_limits: ResourceLimits,
    replay_config:  Option<ReplayConfig>,
    state_json:     Option<String>,
    loaded_at:      DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// P3: Formal lifecycle state machine
// ---------------------------------------------------------------------------

/// Formal strategy lifecycle states (P3).
///
/// ```text
/// ┌──────────┐                  ┌──────────┐
/// │  Loaded  │ ──start()──▶     │ Starting │
/// └──────────┘                  └──────────┘
///     ▲                              │
///     │ unload()                     │ (process spawned)
///     │                              ▼
/// ┌──────────┐  stop()         ┌──────────┐
/// │ Stopped  │ ◀── (any) ──    │ Running  │
/// └──────────┘                  └──────────┘
///                                    │
///            pause() ────────▶  ┌──────────┐
///                               │  Paused  │
///                               └──────────┘
///                                    │
///            resume() ◀─────────     │
///
/// Any state → Failed(msg) on unhandled process error
/// Running  → Replaying when replay_mode is set
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StrategyStatus {
    /// Script loaded, not yet started
    Loaded,
    /// Process being spawned
    Starting,
    /// Process running normally
    Running,
    /// Process suspended (kill-signalled but state preserved)
    Paused,
    /// Graceful stop completed
    Stopped,
    /// Deterministic historical replay in progress
    Replaying,
    /// Error condition
    Failed(String),
}

/// Configuration for deterministic replay mode (P3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    /// Start of replay window (UTC)
    pub from:           DateTime<Utc>,
    /// End of replay window (UTC)
    pub to:             DateTime<Utc>,
    /// Speed multiplier (1.0 = real-time, 0.0 = as fast as possible)
    pub speed_factor:   f64,
    /// If true, use the exact same random seed as the original run
    pub deterministic:  bool,
}

/// Per-strategy resource controls (P3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum number of signals the strategy may emit per minute
    pub max_signals_per_min: u32,
    /// Maximum number of open orders at any time
    pub max_open_orders: u32,
    /// Maximum gross notional value of all positions (0 = unlimited)
    pub max_notional: f64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_signals_per_min: 60,
            max_open_orders:     10,
            max_notional:        0.0,
        }
    }
}

/// Performance metrics for a strategy
#[derive(Debug, Clone)]
pub struct StrategyMetrics {
    pub signals_emitted: u64,
    pub orders_placed: u64,
    pub trades_completed: u64,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub avg_execution_time_ms: f64,
}

impl Default for StrategyMetrics {
    fn default() -> Self {
        Self {
            signals_emitted: 0,
            orders_placed: 0,
            trades_completed: 0,
            win_rate: 0.0,
            total_pnl: 0.0,
            sharpe_ratio: 0.0,
            max_drawdown: 0.0,
            avg_execution_time_ms: 0.0,
        }
    }
}

impl StrategyOrchestrator {
    /// Create a new strategy orchestrator
    ///
    /// # Arguments
    /// * `strategy_dir` - Directory containing strategy Python scripts
    /// * `python_executable` - Path to Python interpreter
    /// * `signal_tx` - Channel to send trading signals to OTM
    pub fn new(
        strategy_dir: impl AsRef<Path>,
        python_executable: impl AsRef<Path>,
        signal_tx: mpsc::UnboundedSender<TradingSignal>,
    ) -> Self {
        Self {
            strategies: Arc::new(DashMap::new()),
            strategy_dir: strategy_dir.as_ref().to_path_buf(),
            python_executable: python_executable.as_ref().to_path_buf(),
            signal_tx,
        }
    }

    /// Load a strategy script from file
    ///
    /// Returns the strategy ID
    pub async fn load_strategy(&self, script_name: &str) -> Result<String> {
        self.load_strategy_with_limits(script_name, ResourceLimits::default()).await
    }

    /// Load a strategy with explicit resource limits
    pub async fn load_strategy_with_limits(
        &self,
        script_name: &str,
        limits: ResourceLimits,
    ) -> Result<String> {
        let script_path = self.strategy_dir.join(script_name);

        if !script_path.exists() {
            return Err(anyhow::anyhow!("Strategy script not found: {:?}", script_path));
        }

        let strategy_id = Uuid::new_v4().to_string();

        let strategy = RunningStrategy {
            id:              strategy_id.clone(),
            name:            script_name.to_string(),
            script_path,
            process:         None,
            status:          StrategyStatus::Loaded,
            metrics:         StrategyMetrics::default(),
            resource_limits: limits,
            replay_config:   None,
            state_json:      None,
            loaded_at:       Utc::now(),
        };

        self.strategies.insert(strategy_id.clone(), strategy);

        info!("Loaded strategy '{}' with ID {}", script_name, strategy_id);
        Ok(strategy_id)
    }

    /// Start a loaded strategy
    pub async fn start_strategy(&self, strategy_id: &str, params: serde_json::Value) -> Result<()> {
        let mut strategy = self
            .strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status == StrategyStatus::Running {
            return Err(anyhow::anyhow!("Strategy already running"));
        }

        // Update status to Starting
        strategy.status = StrategyStatus::Starting;

        // Spawn Python process
        let child = Command::new(&self.python_executable)
            .arg(&strategy.script_path)
            .arg("--params")
            .arg(serde_json::to_string(&params)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn strategy process")?;

        // Store process handle
        strategy.process = Some(child);
        strategy.status  = StrategyStatus::Running;

        info!("Started strategy '{}' (ID: {})", strategy.name, strategy_id);
        Ok(())
    }

    /// Start strategy in deterministic replay mode (P3)
    pub async fn start_replay(
        &self,
        strategy_id: &str,
        params: serde_json::Value,
        replay: ReplayConfig,
    ) -> Result<()> {
        {
            let mut strategy = self
                .strategies
                .get_mut(strategy_id)
                .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

            if strategy.status == StrategyStatus::Running
                || strategy.status == StrategyStatus::Replaying
            {
                return Err(anyhow::anyhow!("Strategy already active"));
            }
            strategy.replay_config = Some(replay.clone());
            strategy.status        = StrategyStatus::Starting;
        }

        let mut enhanced_params = params.clone();
        if let Some(obj) = enhanced_params.as_object_mut() {
            obj.insert(
                "replay_from".into(),
                serde_json::Value::String(replay.from.to_rfc3339()),
            );
            obj.insert(
                "replay_to".into(),
                serde_json::Value::String(replay.to.to_rfc3339()),
            );
            obj.insert(
                "replay_speed".into(),
                serde_json::json!(replay.speed_factor),
            );
            obj.insert(
                "deterministic".into(),
                serde_json::json!(replay.deterministic),
            );
        }

        {
            let mut strategy = self
                .strategies
                .get_mut(strategy_id)
                .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

            let child = Command::new(&self.python_executable)
                .arg(&strategy.script_path)
                .arg("--params")
                .arg(serde_json::to_string(&enhanced_params)?)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn replay strategy process")?;

            strategy.process = Some(child);
            strategy.status  = StrategyStatus::Replaying;
        }

        info!("Started replay for strategy {}", strategy_id);
        Ok(())
    }

    /// Persist strategy state JSON (called by strategy before shutdown / on checkpoint)
    pub fn save_state(&self, strategy_id: &str, state_json: String) {
        if let Some(mut strategy) = self.strategies.get_mut(strategy_id) {
            strategy.state_json = Some(state_json);
        }
    }

    /// Retrieve the last saved state for a strategy
    pub fn load_state(&self, strategy_id: &str) -> Option<String> {
        self.strategies
            .get(strategy_id)
            .and_then(|s| s.state_json.clone())
    }

    /// Get resource limits for a strategy
    pub fn get_resource_limits(&self, strategy_id: &str) -> Option<ResourceLimits> {
        self.strategies
            .get(strategy_id)
            .map(|s| s.resource_limits.clone())
    }

    /// Stop a running strategy
    pub async fn stop_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategy = self
            .strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if !matches!(
            strategy.status,
            StrategyStatus::Running | StrategyStatus::Replaying | StrategyStatus::Starting
        ) {
            return Err(anyhow::anyhow!("Strategy not active"));
        }

        // Kill the process
        if let Some(mut process) = strategy.process.take() {
            match process.kill() {
                Ok(_) => {
                    let _ = process.wait(); // Clean up zombie process
                    strategy.status = StrategyStatus::Stopped;
                    info!("Stopped strategy '{}' (ID: {})", strategy.name, strategy_id);
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to kill strategy process: {}", e);
                    strategy.status = StrategyStatus::Failed(e.to_string());
                    Err(anyhow::anyhow!("Failed to stop strategy: {}", e))
                }
            }
        } else {
            strategy.status = StrategyStatus::Stopped;
            Ok(())
        }
    }

    /// Pause a running strategy (stop without removing)
    pub async fn pause_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategy = self
            .strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if !matches!(strategy.status, StrategyStatus::Running | StrategyStatus::Replaying) {
            return Err(anyhow::anyhow!("Strategy not running"));
        }

        // Kill the process but keep the strategy loaded
        if let Some(mut process) = strategy.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        strategy.status = StrategyStatus::Paused;
        info!("Paused strategy '{}' (ID: {})", strategy.name, strategy_id);
        Ok(())
    }

    /// Resume a paused strategy
    pub async fn resume_strategy(&self, strategy_id: &str, params: serde_json::Value) -> Result<()> {
        let strategy = self
            .strategies
            .get(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status != StrategyStatus::Paused {
            return Err(anyhow::anyhow!("Strategy not paused"));
        }

        drop(strategy); // Release the read lock before calling start_strategy

        self.start_strategy(strategy_id, params).await
    }

    /// Unload a strategy (must be stopped or loaded, not running)
    pub async fn unload_strategy(&self, strategy_id: &str) -> Result<()> {
        let strategy = self
            .strategies
            .get(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if matches!(
            strategy.status,
            StrategyStatus::Running | StrategyStatus::Replaying | StrategyStatus::Starting
        ) {
            return Err(anyhow::anyhow!("Cannot unload active strategy. Stop it first."));
        }

        drop(strategy); // Release the lock

        self.strategies.remove(strategy_id);

        info!("Unloaded strategy ID: {}", strategy_id);
        Ok(())
    }

    /// Get status of a strategy
    pub fn get_strategy_status(&self, strategy_id: &str) -> Option<StrategyStatus> {
        self.strategies.get(strategy_id).map(|s| s.status.clone())
    }

    /// Get metrics for a strategy
    pub fn get_strategy_metrics(&self, strategy_id: &str) -> Option<StrategyMetrics> {
        self.strategies.get(strategy_id).map(|s| s.metrics.clone())
    }

    /// List all loaded strategies
    pub fn list_strategies(&self) -> Vec<(String, String, StrategyStatus)> {
        self.strategies
            .iter()
            .map(|entry| {
                let strategy = entry.value();
                (
                    strategy.id.clone(),
                    strategy.name.clone(),
                    strategy.status.clone(),
                )
            })
            .collect()
    }

    /// Emit a trading signal (called by strategy via IPC)
    pub async fn emit_signal(&self, strategy_id: &str, signal: TradingSignal) -> Result<()> {
        // Update metrics
        if let Some(mut strategy) = self.strategies.get_mut(strategy_id) {
            strategy.metrics.signals_emitted += 1;
        }

        // Send signal to Order & Trade Manager
        self.signal_tx.send(signal)
            .context("Failed to send signal to OTM")?;

        debug!("Emitted signal from strategy {}", strategy_id);
        Ok(())
    }

    /// Record an order placed by a strategy
    pub fn record_order_placed(&self, strategy_id: &str) {
        if let Some(mut strategy) = self.strategies.get_mut(strategy_id) {
            strategy.metrics.orders_placed += 1;
        }
    }

    /// Record a trade completion with P&L
    pub fn record_trade_completed(&self, strategy_id: &str, pnl: f64) {
        if let Some(mut strategy) = self.strategies.get_mut(strategy_id) {
            strategy.metrics.trades_completed += 1;
            strategy.metrics.total_pnl += pnl;

            // Update win rate
            let wins = if pnl > 0.0 { 1 } else { 0 };
            let total = strategy.metrics.trades_completed as f64;
            strategy.metrics.win_rate = (wins as f64 / total) * 100.0;
        }
    }

    /// Update strategy metrics (called periodically or after trades)
    pub fn update_metrics(
        &self,
        strategy_id: &str,
        sharpe: f64,
        max_dd: f64,
        avg_exec_time_ms: f64,
    ) {
        if let Some(mut strategy) = self.strategies.get_mut(strategy_id) {
            strategy.metrics.sharpe_ratio = sharpe;
            strategy.metrics.max_drawdown = max_dd;
            strategy.metrics.avg_execution_time_ms = avg_exec_time_ms;
        }
    }

    /// Health check - monitor all running strategies
    pub async fn health_check(&self) {
        for mut entry in self.strategies.iter_mut() {
            let strategy = entry.value_mut();

            if matches!(
                strategy.status,
                StrategyStatus::Running | StrategyStatus::Replaying
            ) {
                // Check if process is still alive
                if let Some(ref mut process) = strategy.process {
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            // Process has exited
                            let exit_msg = format!("Process exited with status: {}", status);
                            error!("Strategy '{}' (ID: {}) {}", strategy.name, strategy.id, exit_msg);
                            strategy.status = StrategyStatus::Failed(exit_msg);
                        }
                        Ok(None) => {
                            // Process is still running
                            debug!("Strategy '{}' is healthy", strategy.name);
                        }
                        Err(e) => {
                            // Error checking process status
                            error!("Failed to check strategy '{}' process: {}", strategy.name, e);
                            strategy.status = StrategyStatus::Failed(e.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Start periodic health check loop
    pub async fn start_health_check_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                self.health_check().await;
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_lifecycle() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let orchestrator = StrategyOrchestrator::new("strategies/", "python3", tx);

        // Test load — fails because file doesn't exist (expected)
        let result = orchestrator.load_strategy("test_strategy.py").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_strategy_metrics_default() {
        let metrics = StrategyMetrics::default();
        assert_eq!(metrics.signals_emitted, 0);
        assert_eq!(metrics.win_rate, 0.0);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_signals_per_min, 60);
        assert_eq!(limits.max_open_orders, 10);
        assert_eq!(limits.max_notional, 0.0);
    }

    #[test]
    fn test_strategy_status_loaded_not_running() {
        // Verify the new Loaded state is distinct from Running
        let loaded  = StrategyStatus::Loaded;
        let running = StrategyStatus::Running;
        assert_ne!(loaded, running);
    }

    #[test]
    fn test_strategy_status_failed_contains_reason() {
        let s = StrategyStatus::Failed("process exited with code 1".into());
        match s {
            StrategyStatus::Failed(msg) => assert!(msg.contains("code 1")),
            _ => panic!("Expected Failed"),
        }
    }

    #[test]
    fn test_replay_config_creation() {
        let cfg = ReplayConfig {
            from:         chrono::Utc::now() - chrono::Duration::days(30),
            to:           chrono::Utc::now(),
            speed_factor: 10.0,
            deterministic: true,
        };
        assert!(cfg.speed_factor > 1.0);
        assert!(cfg.deterministic);
    }

    #[tokio::test]
    async fn test_save_and_load_state() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let orch = StrategyOrchestrator::new("/tmp/nonexistent_strat_dir", "python3", tx);

        // We can't load a real script, so directly insert a dummy RunningStrategy
        let id = "test-id".to_string();
        orch.strategies.insert(
            id.clone(),
            RunningStrategy {
                id:              id.clone(),
                name:            "dummy".into(),
                script_path:     std::path::PathBuf::from("/tmp/dummy.py"),
                process:         None,
                status:          StrategyStatus::Paused,
                metrics:         StrategyMetrics::default(),
                resource_limits: ResourceLimits::default(),
                replay_config:   None,
                state_json:      None,
                loaded_at:       chrono::Utc::now(),
            },
        );

        assert!(orch.load_state(&id).is_none());
        orch.save_state(&id, r#"{"position":10}"#.into());
        let state = orch.load_state(&id).unwrap();
        assert!(state.contains("position"));
    }
}
