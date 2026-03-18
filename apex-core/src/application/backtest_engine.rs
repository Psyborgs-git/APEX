use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::domain::models::*;

/// Configuration for a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Unique identifier for this backtest run
    pub run_id: String,
    /// Symbols to include in the backtest
    pub symbols: Vec<Symbol>,
    /// Start date of the backtest period
    pub start: DateTime<Utc>,
    /// End date of the backtest period
    pub end: DateTime<Utc>,
    /// Initial capital
    pub initial_capital: f64,
    /// Currency
    pub currency: String,
    /// Commission in basis points (e.g. 3 = 0.03%)
    pub commission_bps: f64,
    /// Slippage in basis points (e.g. 2 = 0.02%)
    pub slippage_bps: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            run_id: Uuid::new_v4().to_string(),
            symbols: vec![],
            start: Utc::now(),
            end: Utc::now(),
            initial_capital: 1_000_000.0,
            currency: "INR".into(),
            commission_bps: 3.0,
            slippage_bps: 2.0,
        }
    }
}

/// Result of a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub run_id: String,
    pub config: BacktestConfig,
    pub metrics: BacktestMetrics,
    pub trades: Vec<BacktestTrade>,
    pub equity_curve: Vec<EquityPoint>,
}

/// Performance metrics from a backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub total_return: f64,
    pub total_return_pct: f64,
    pub annualized_return_pct: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_trade_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub max_consecutive_wins: u32,
    pub max_consecutive_losses: u32,
    pub final_equity: f64,
}

/// A single trade in the backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTrade {
    pub symbol: Symbol,
    pub side: OrderSide,
    pub entry_time: DateTime<Utc>,
    pub entry_price: f64,
    pub exit_time: Option<DateTime<Utc>>,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub pnl: f64,
    pub commission: f64,
}

/// A point on the equity curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub time: DateTime<Utc>,
    pub equity: f64,
    pub drawdown: f64,
}

/// Internal position state during backtest
#[derive(Debug, Clone)]
struct SimPosition {
    symbol: Symbol,
    side: OrderSide,
    quantity: f64,
    avg_price: f64,
    entry_time: DateTime<Utc>,
}

/// Backtest engine — replays historical OHLCV bars through a signal
/// generator and simulates order execution with slippage & commission.
pub struct BacktestEngine {
    config: BacktestConfig,
    positions: HashMap<String, SimPosition>,
    cash: f64,
    trades: Vec<BacktestTrade>,
    equity_curve: Vec<EquityPoint>,
    peak_equity: f64,
}

impl BacktestEngine {
    /// Create a new backtest engine with the given configuration.
    pub fn new(config: BacktestConfig) -> Self {
        let initial = config.initial_capital;
        Self {
            config,
            positions: HashMap::new(),
            cash: initial,
            trades: Vec::new(),
            equity_curve: Vec::new(),
            peak_equity: initial,
        }
    }

    /// Run a backtest over the provided OHLCV data and signals.
    ///
    /// `data` is a map of symbol → sorted OHLCV bars.
    /// `signal_fn` is a callback that receives the current bar for each
    /// symbol and returns an optional signal.
    pub fn run<F>(
        &mut self,
        data: &HashMap<String, Vec<OHLCV>>,
        mut signal_fn: F,
    ) -> Result<BacktestResult>
    where
        F: FnMut(&str, &OHLCV, &HashMap<String, SimPosition>, f64) -> Option<BacktestSignal>,
    {
        info!(
            run_id = %self.config.run_id,
            symbols = ?self.config.symbols.iter().map(|s| &s.0).collect::<Vec<_>>(),
            "Starting backtest"
        );

        // Build a merged timeline of all bars sorted by time
        let mut all_bars: Vec<(&str, &OHLCV)> = Vec::new();
        for (symbol, bars) in data {
            for bar in bars {
                all_bars.push((symbol.as_str(), bar));
            }
        }
        all_bars.sort_by_key(|(_, b)| b.time);

        if all_bars.is_empty() {
            return Err(anyhow!("No data provided for backtest"));
        }

        // Replay bars
        for (symbol, bar) in &all_bars {
            // Generate signal
            if let Some(signal) = signal_fn(symbol, bar, &self.positions, self.cash) {
                self.process_signal(signal, bar)?;
            }

            // Record equity curve point
            let equity = self.calculate_equity(bar);
            if equity > self.peak_equity {
                self.peak_equity = equity;
            }
            let drawdown = if self.peak_equity > 0.0 {
                (self.peak_equity - equity) / self.peak_equity
            } else {
                0.0
            };
            self.equity_curve.push(EquityPoint {
                time: bar.time,
                equity,
                drawdown,
            });
        }

        // Close any remaining open positions at last available price
        let last_bars: HashMap<String, &OHLCV> = data
            .iter()
            .filter_map(|(sym, bars)| bars.last().map(|b| (sym.clone(), b)))
            .collect();
        self.close_all_positions(&last_bars);

        // Calculate final metrics
        let metrics = self.calculate_metrics();

        let result = BacktestResult {
            run_id: self.config.run_id.clone(),
            config: self.config.clone(),
            metrics,
            trades: self.trades.clone(),
            equity_curve: self.equity_curve.clone(),
        };

        info!(
            run_id = %result.run_id,
            total_trades = result.metrics.total_trades,
            total_return_pct = format!("{:.2}%", result.metrics.total_return_pct),
            sharpe = format!("{:.3}", result.metrics.sharpe_ratio),
            max_dd = format!("{:.2}%", result.metrics.max_drawdown_pct),
            "Backtest complete"
        );

        Ok(result)
    }

    fn process_signal(&mut self, signal: BacktestSignal, bar: &OHLCV) -> Result<()> {
        match signal {
            BacktestSignal::Buy { symbol, quantity } => {
                let fill_price = bar.close * (1.0 + self.config.slippage_bps / 10_000.0);
                let commission = fill_price * quantity * self.config.commission_bps / 10_000.0;
                let cost = fill_price * quantity + commission;

                if cost > self.cash {
                    debug!(
                        symbol = %symbol.0,
                        cost = cost,
                        cash = self.cash,
                        "Insufficient cash for buy order"
                    );
                    return Ok(());
                }

                self.cash -= cost;

                // Merge with existing position or create new
                if let Some(pos) = self.positions.get_mut(&symbol.0) {
                    let new_qty = pos.quantity + quantity;
                    pos.avg_price =
                        (pos.avg_price * pos.quantity + fill_price * quantity) / new_qty;
                    pos.quantity = new_qty;
                } else {
                    self.positions.insert(
                        symbol.0.clone(),
                        SimPosition {
                            symbol: symbol.clone(),
                            side: OrderSide::Buy,
                            quantity,
                            avg_price: fill_price,
                            entry_time: bar.time,
                        },
                    );
                }

                self.trades.push(BacktestTrade {
                    symbol,
                    side: OrderSide::Buy,
                    entry_time: bar.time,
                    entry_price: fill_price,
                    exit_time: None,
                    exit_price: None,
                    quantity,
                    pnl: 0.0,
                    commission,
                });
            }
            BacktestSignal::Sell { symbol, quantity } => {
                let fill_price = bar.close * (1.0 - self.config.slippage_bps / 10_000.0);
                let commission = fill_price * quantity * self.config.commission_bps / 10_000.0;

                if let Some(pos) = self.positions.get_mut(&symbol.0) {
                    let sell_qty = quantity.min(pos.quantity);
                    let pnl = (fill_price - pos.avg_price) * sell_qty - commission;

                    self.cash += fill_price * sell_qty - commission;
                    pos.quantity -= sell_qty;

                    self.trades.push(BacktestTrade {
                        symbol: symbol.clone(),
                        side: OrderSide::Sell,
                        entry_time: pos.entry_time,
                        entry_price: pos.avg_price,
                        exit_time: Some(bar.time),
                        exit_price: Some(fill_price),
                        quantity: sell_qty,
                        pnl,
                        commission,
                    });

                    if pos.quantity <= 0.001 {
                        self.positions.remove(&symbol.0);
                    }
                } else {
                    debug!(symbol = %symbol.0, "No position to sell");
                }
            }
            BacktestSignal::Close { symbol } => {
                if let Some(pos) = self.positions.remove(&symbol.0) {
                    let fill_price = bar.close * (1.0 - self.config.slippage_bps / 10_000.0);
                    let commission =
                        fill_price * pos.quantity * self.config.commission_bps / 10_000.0;
                    let pnl = (fill_price - pos.avg_price) * pos.quantity - commission;
                    self.cash += fill_price * pos.quantity - commission;

                    self.trades.push(BacktestTrade {
                        symbol,
                        side: OrderSide::Sell,
                        entry_time: pos.entry_time,
                        entry_price: pos.avg_price,
                        exit_time: Some(bar.time),
                        exit_price: Some(fill_price),
                        quantity: pos.quantity,
                        pnl,
                        commission,
                    });
                }
            }
        }
        Ok(())
    }

    fn close_all_positions(&mut self, last_bars: &HashMap<String, &OHLCV>) {
        let symbols: Vec<String> = self.positions.keys().cloned().collect();
        for symbol in symbols {
            if let Some(bar) = last_bars.get(&symbol) {
                if let Some(pos) = self.positions.remove(&symbol) {
                    let fill_price = bar.close;
                    let commission =
                        fill_price * pos.quantity * self.config.commission_bps / 10_000.0;
                    let pnl = (fill_price - pos.avg_price) * pos.quantity - commission;
                    self.cash += fill_price * pos.quantity - commission;

                    self.trades.push(BacktestTrade {
                        symbol: pos.symbol,
                        side: OrderSide::Sell,
                        entry_time: pos.entry_time,
                        entry_price: pos.avg_price,
                        exit_time: Some(bar.time),
                        exit_price: Some(fill_price),
                        quantity: pos.quantity,
                        pnl,
                        commission,
                    });
                }
            } else {
                warn!(symbol = %symbol, "No last bar for open position at end of backtest");
            }
        }
    }

    fn calculate_equity(&self, current_bar: &OHLCV) -> f64 {
        let positions_value: f64 = self
            .positions
            .values()
            .map(|pos| {
                if pos.symbol.0 == current_bar.symbol.0 {
                    pos.quantity * current_bar.close
                } else {
                    pos.quantity * pos.avg_price // fallback
                }
            })
            .sum();
        self.cash + positions_value
    }

    fn calculate_metrics(&self) -> BacktestMetrics {
        let initial = self.config.initial_capital;
        let final_equity = self.cash;
        let total_return = final_equity - initial;
        let total_return_pct = if initial > 0.0 {
            (total_return / initial) * 100.0
        } else {
            0.0
        };

        // Count wins/losses from closed trades
        let closed: Vec<&BacktestTrade> = self.trades.iter().filter(|t| t.exit_time.is_some()).collect();
        let total_trades = closed.len() as u64;
        let winning: Vec<&&BacktestTrade> = closed.iter().filter(|t| t.pnl > 0.0).collect();
        let losing: Vec<&&BacktestTrade> = closed.iter().filter(|t| t.pnl <= 0.0).collect();
        let winning_trades = winning.len() as u64;
        let losing_trades = losing.len() as u64;

        let win_rate = if total_trades > 0 {
            (winning_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let total_wins: f64 = winning.iter().map(|t| t.pnl).sum();
        let total_losses: f64 = losing.iter().map(|t| t.pnl.abs()).sum();

        let profit_factor = if total_losses > 0.0 {
            total_wins / total_losses
        } else if total_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let avg_trade_pnl = if total_trades > 0 {
            closed.iter().map(|t| t.pnl).sum::<f64>() / total_trades as f64
        } else {
            0.0
        };

        let avg_win = if winning_trades > 0 {
            total_wins / winning_trades as f64
        } else {
            0.0
        };

        let avg_loss = if losing_trades > 0 {
            total_losses / losing_trades as f64
        } else {
            0.0
        };

        // Consecutive wins/losses
        let (max_cons_wins, max_cons_losses) = Self::max_consecutive(&closed);

        // Sharpe ratio from equity curve
        let sharpe = self.calculate_sharpe();

        // Max drawdown
        let (max_dd, max_dd_pct) = self.max_drawdown();

        // Annualized return
        let days = if !self.equity_curve.is_empty() {
            let first = self.equity_curve.first().unwrap().time;
            let last = self.equity_curve.last().unwrap().time;
            (last - first).num_days().max(1) as f64
        } else {
            365.0
        };
        let years = days / 365.0;
        let annualized = if years > 0.0 {
            ((final_equity / initial).powf(1.0 / years) - 1.0) * 100.0
        } else {
            total_return_pct
        };

        BacktestMetrics {
            total_return,
            total_return_pct,
            annualized_return_pct: annualized,
            sharpe_ratio: sharpe,
            max_drawdown: max_dd,
            max_drawdown_pct: max_dd_pct,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            profit_factor,
            avg_trade_pnl,
            avg_win,
            avg_loss,
            max_consecutive_wins: max_cons_wins,
            max_consecutive_losses: max_cons_losses,
            final_equity,
        }
    }

    fn calculate_sharpe(&self) -> f64 {
        if self.equity_curve.len() < 2 {
            return 0.0;
        }

        let returns: Vec<f64> = self
            .equity_curve
            .windows(2)
            .map(|w| {
                if w[0].equity > 0.0 {
                    (w[1].equity - w[0].equity) / w[0].equity
                } else {
                    0.0
                }
            })
            .collect();

        if returns.is_empty() {
            return 0.0;
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev > 0.0 {
            // Annualize: assume 252 trading days
            (mean / std_dev) * (252.0_f64).sqrt()
        } else {
            0.0
        }
    }

    fn max_drawdown(&self) -> (f64, f64) {
        let mut peak = self.config.initial_capital;
        let mut max_dd = 0.0_f64;
        let mut max_dd_pct = 0.0_f64;

        for pt in &self.equity_curve {
            if pt.equity > peak {
                peak = pt.equity;
            }
            let dd = peak - pt.equity;
            let dd_pct = if peak > 0.0 { (dd / peak) * 100.0 } else { 0.0 };
            max_dd = max_dd.max(dd);
            max_dd_pct = max_dd_pct.max(dd_pct);
        }

        (max_dd, max_dd_pct)
    }

    fn max_consecutive(trades: &[&BacktestTrade]) -> (u32, u32) {
        let mut max_wins = 0_u32;
        let mut max_losses = 0_u32;
        let mut curr_wins = 0_u32;
        let mut curr_losses = 0_u32;

        for trade in trades {
            if trade.pnl > 0.0 {
                curr_wins += 1;
                curr_losses = 0;
                max_wins = max_wins.max(curr_wins);
            } else {
                curr_losses += 1;
                curr_wins = 0;
                max_losses = max_losses.max(curr_losses);
            }
        }

        (max_wins, max_losses)
    }
}

/// Signal type for backtest signal generation
#[derive(Debug, Clone)]
pub enum BacktestSignal {
    Buy { symbol: Symbol, quantity: f64 },
    Sell { symbol: Symbol, quantity: f64 },
    Close { symbol: Symbol },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_bars(symbol: &str, prices: &[(i64, f64)]) -> Vec<OHLCV> {
        prices
            .iter()
            .map(|(day, close)| {
                let time = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(*day);
                OHLCV {
                    time,
                    symbol: Symbol(symbol.into()),
                    open: close - 1.0,
                    high: close + 2.0,
                    low: close - 2.0,
                    close: *close,
                    volume: 100_000,
                }
            })
            .collect()
    }

    #[test]
    fn test_backtest_no_data() {
        let config = BacktestConfig::default();
        let mut engine = BacktestEngine::new(config);
        let data = HashMap::new();
        let result = engine.run(&data, |_, _, _, _| None);
        assert!(result.is_err());
    }

    #[test]
    fn test_backtest_no_signals() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert("AAPL".into(), make_bars("AAPL", &[(0, 100.0), (1, 101.0), (2, 102.0)]));

        let result = engine.run(&data, |_, _, _, _| None).unwrap();
        assert_eq!(result.metrics.total_trades, 0);
        assert!((result.metrics.final_equity - 100_000.0).abs() < 0.01);
    }

    #[test]
    fn test_backtest_buy_and_sell() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 110.0), (2, 120.0)]),
        );

        let result = engine
            .run(&data, |symbol, bar, positions, _cash| {
                if bar.close == 100.0 {
                    // Buy on first bar
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.into()),
                        quantity: 100.0,
                    })
                } else if bar.close == 120.0 && positions.contains_key(symbol) {
                    // Sell on third bar
                    Some(BacktestSignal::Sell {
                        symbol: Symbol(symbol.into()),
                        quantity: 100.0,
                    })
                } else {
                    None
                }
            })
            .unwrap();

        // Bought at 100, sold at 120, 100 shares = $2000 profit
        // 1 buy entry + 1 sell close = 2 trades total, 1 with exit_time (the sell)
        assert_eq!(result.metrics.total_trades, 1); // only closed trades count
        assert!(result.metrics.total_return > 0.0);
        assert_eq!(result.metrics.winning_trades, 1);
        assert!(result.metrics.win_rate > 0.0);
    }

    #[test]
    fn test_backtest_insufficient_cash() {
        let config = BacktestConfig {
            initial_capital: 500.0, // Very low capital
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 110.0)]),
        );

        let result = engine
            .run(&data, |symbol, bar, _positions, _cash| {
                if bar.close == 100.0 {
                    // Try to buy 100 shares at $100 = $10,000 but only have $500
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.into()),
                        quantity: 100.0,
                    })
                } else {
                    None
                }
            })
            .unwrap();

        // Order should be skipped due to insufficient cash
        assert_eq!(result.metrics.total_trades, 0);
        assert!((result.metrics.final_equity - 500.0).abs() < 0.01);
    }

    #[test]
    fn test_backtest_with_commission_and_slippage() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            commission_bps: 10.0, // 0.1%
            slippage_bps: 5.0,    // 0.05%
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 110.0)]),
        );

        let result = engine
            .run(&data, |symbol, bar, positions, _cash| {
                if bar.close == 100.0 {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.into()),
                        quantity: 10.0,
                    })
                } else if positions.contains_key(symbol) {
                    Some(BacktestSignal::Close {
                        symbol: Symbol(symbol.into()),
                    })
                } else {
                    None
                }
            })
            .unwrap();

        // With commission and slippage, profit should be less than pure 10*10=$100
        assert!(result.metrics.final_equity < 100_100.0);
        assert!(result.metrics.final_equity > 100_000.0); // Should still be profitable
    }

    #[test]
    fn test_backtest_close_signal() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 90.0)]),
        );

        let result = engine
            .run(&data, |symbol, bar, positions, _cash| {
                if bar.close == 100.0 {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.into()),
                        quantity: 10.0,
                    })
                } else if positions.contains_key(symbol) {
                    Some(BacktestSignal::Close {
                        symbol: Symbol(symbol.into()),
                    })
                } else {
                    None
                }
            })
            .unwrap();

        // Bought at 100, closed at 90, 10 shares = -$100 loss
        assert!(result.metrics.total_return < 0.0);
        assert_eq!(result.metrics.losing_trades, 1);
    }

    #[test]
    fn test_backtest_equity_curve() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 105.0), (2, 110.0)]),
        );

        let result = engine.run(&data, |_, _, _, _| None).unwrap();

        // Should have 3 equity points
        assert_eq!(result.equity_curve.len(), 3);
        // All should equal initial capital (no trades)
        for pt in &result.equity_curve {
            assert!((pt.equity - 100_000.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_backtest_max_drawdown() {
        let config = BacktestConfig {
            initial_capital: 100_000.0,
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        // Price goes up then down
        data.insert(
            "AAPL".into(),
            make_bars("AAPL", &[(0, 100.0), (1, 120.0), (2, 90.0), (3, 110.0)]),
        );

        let result = engine
            .run(&data, |symbol, bar, positions, _cash| {
                if bar.close == 100.0 {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.into()),
                        quantity: 100.0,
                    })
                } else if bar.close == 110.0 && positions.contains_key(symbol) {
                    Some(BacktestSignal::Close {
                        symbol: Symbol(symbol.into()),
                    })
                } else {
                    None
                }
            })
            .unwrap();

        // Should have recorded a drawdown when price went from 120 to 90
        assert!(result.metrics.max_drawdown > 0.0);
        assert!(result.metrics.max_drawdown_pct > 0.0);
    }

    #[test]
    fn test_backtest_consecutive_wins_losses() {
        let config = BacktestConfig {
            initial_capital: 1_000_000.0,
            commission_bps: 0.0,
            slippage_bps: 0.0,
            ..BacktestConfig::default()
        };
        let mut engine = BacktestEngine::new(config);
        let mut data = HashMap::new();
        data.insert(
            "AAPL".into(),
            make_bars(
                "AAPL",
                &[
                    (0, 100.0),
                    (1, 110.0), // win
                    (2, 120.0),
                    (3, 130.0), // win
                    (4, 140.0),
                    (5, 130.0), // loss
                ],
            ),
        );

        let mut trade_num = 0;
        let result = engine
            .run(&data, |symbol, bar, positions, _cash| {
                if !positions.contains_key(symbol) && trade_num < 3 {
                    // Buy on even bars
                    if [100.0, 120.0, 140.0].contains(&bar.close) {
                        trade_num += 1;
                        return Some(BacktestSignal::Buy {
                            symbol: Symbol(symbol.into()),
                            quantity: 10.0,
                        });
                    }
                } else if positions.contains_key(symbol) {
                    // Sell on odd bars
                    if [110.0, 130.0, 130.0].contains(&bar.close) {
                        return Some(BacktestSignal::Close {
                            symbol: Symbol(symbol.into()),
                        });
                    }
                }
                None
            })
            .unwrap();

        // Should have some consecutive stats
        assert!(result.metrics.total_trades > 0);
    }

    #[test]
    fn test_backtest_config_default() {
        let config = BacktestConfig::default();
        assert_eq!(config.initial_capital, 1_000_000.0);
        assert_eq!(config.commission_bps, 3.0);
        assert_eq!(config.slippage_bps, 2.0);
        assert_eq!(config.currency, "INR");
    }
}
