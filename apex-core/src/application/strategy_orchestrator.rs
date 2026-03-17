use anyhow::{Context, Result};
use dashmap::DashMap;
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
    strategies: Arc<DashMap<String, RunningStrategy>>,
    strategy_dir: PathBuf,
    python_executable: PathBuf,
    signal_tx: mpsc::UnboundedSender<TradingSignal>,
}

/// A running strategy instance
struct RunningStrategy {
    id: String,
    name: String,
    script_path: PathBuf,
    process: Option<Child>,
    status: StrategyStatus,
    metrics: StrategyMetrics,
}

/// Strategy execution status
#[derive(Debug, Clone, PartialEq)]
pub enum StrategyStatus {
    Stopped,
    Starting,
    Running,
    Paused,
    Failed(String),
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
        let script_path = self.strategy_dir.join(script_name);

        if !script_path.exists() {
            return Err(anyhow::anyhow!("Strategy script not found: {:?}", script_path));
        }

        let strategy_id = Uuid::new_v4().to_string();

        let strategy = RunningStrategy {
            id: strategy_id.clone(),
            name: script_name.to_string(),
            script_path,
            process: None,
            status: StrategyStatus::Stopped,
            metrics: StrategyMetrics::default(),
        };

        self.strategies.insert(strategy_id.clone(), strategy);

        info!("Loaded strategy '{}' with ID {}", script_name, strategy_id);
        Ok(strategy_id)
    }

    /// Start a loaded strategy
    pub async fn start_strategy(&self, strategy_id: &str, params: serde_json::Value) -> Result<()> {
        let mut strategy = self.strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status == StrategyStatus::Running {
            return Err(anyhow::anyhow!("Strategy already running"));
        }

        // Update status to Starting
        strategy.status = StrategyStatus::Starting;

        // Spawn Python process
        let mut child = Command::new(&self.python_executable)
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
        strategy.status = StrategyStatus::Running;

        info!("Started strategy '{}' (ID: {})", strategy.name, strategy_id);
        Ok(())
    }

    /// Stop a running strategy
    pub async fn stop_strategy(&self, strategy_id: &str) -> Result<()> {
        let mut strategy = self.strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status != StrategyStatus::Running {
            return Err(anyhow::anyhow!("Strategy not running"));
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
        let mut strategy = self.strategies
            .get_mut(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status != StrategyStatus::Running {
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
        let strategy = self.strategies
            .get(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status != StrategyStatus::Paused {
            return Err(anyhow::anyhow!("Strategy not paused"));
        }

        drop(strategy); // Release the read lock before calling start_strategy

        self.start_strategy(strategy_id, params).await
    }

    /// Unload a strategy (must be stopped)
    pub async fn unload_strategy(&self, strategy_id: &str) -> Result<()> {
        let strategy = self.strategies
            .get(strategy_id)
            .ok_or_else(|| anyhow::anyhow!("Strategy not found: {}", strategy_id))?;

        if strategy.status == StrategyStatus::Running {
            return Err(anyhow::anyhow!("Cannot unload running strategy. Stop it first."));
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

            if strategy.status == StrategyStatus::Running {
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
        let orchestrator = StrategyOrchestrator::new(
            "strategies/",
            "python3",
            tx,
        );

        // Test load
        let result = orchestrator.load_strategy("test_strategy.py").await;
        // Will fail because file doesn't exist, but tests the API
        assert!(result.is_err());
    }

    #[test]
    fn test_strategy_metrics_default() {
        let metrics = StrategyMetrics::default();
        assert_eq!(metrics.signals_emitted, 0);
        assert_eq!(metrics.win_rate, 0.0);
    }
}
