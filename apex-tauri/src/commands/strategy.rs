use super::python_runtime;
use crate::validation;
use crate::state::AppState;
use apex_adapters::market_data::yahoo_finance::YahooFinanceAdapter;
use apex_core::application::backtest_engine::{
    BacktestConfig, BacktestEngine, BacktestMetrics, BacktestSignal, BacktestTrade, EquityPoint,
    SimPosition,
};
use apex_core::domain::models::{OHLCV, OHLCVQuery, Symbol, Timeframe};
use apex_core::ports::market_data::MarketDataPort;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{command, State};
use tokio::process::Command;
use uuid::Uuid;

const DEFAULT_STRATEGY_TEMPLATE: &str = r#"\"\"\"
APEX Strategy Template
Subclass Strategy and override on_bar / on_tick.
\"\"\"
from apex_sdk import Strategy, Bar, Signal, Timeframe


class MyStrategy(Strategy):
    def on_init(self, params: dict) -> None:
        self.subscribe([\"RELIANCE.NS\"], Timeframe.M5)
        self.log(\"Strategy initialized\")

    def on_bar(self, symbol: str, bar: Bar) -> None:
        sma = self.indicator(\"sma\", symbol, 20)
        if bar.close > sma:
            self.emit(Signal(
                symbol=symbol,
                direction=\"long\",
                strength=0.8,
                metadata={\"reason\": \"price_above_sma\"},
            ))

    def on_stop(self) -> None:
        self.log(\"Strategy stopped\")
"#;

#[derive(Debug, Clone, Serialize)]
pub struct StrategyFileDto {
    pub name: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyExecutionResultDto {
    pub success: bool,
    pub output: Vec<String>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyBacktestRequestDto {
    pub path: String,
    pub symbol: String,
    pub timeframe: String,
    pub from: String,
    pub to: String,
    pub initial_capital: Option<f64>,
    pub commission_bps: Option<f64>,
    pub slippage_bps: Option<f64>,
    pub quantity: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyBacktestResultDto {
    pub strategy_name: String,
    pub strategy_path: String,
    pub inferred_strategy: String,
    pub symbol: String,
    pub timeframe: String,
    pub bars_analyzed: usize,
    pub metrics: BacktestMetrics,
    pub trades: Vec<BacktestTrade>,
    pub equity_curve: Vec<EquityPoint>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
enum InferredBacktestStrategy {
    BuyAndHold,
    PriceVsSma { period: usize },
    SmaCrossover { fast: usize, slow: usize },
}

impl InferredBacktestStrategy {
    fn label(&self) -> String {
        match self {
            Self::BuyAndHold => "Buy & Hold".to_string(),
            Self::PriceVsSma { period } => format!("Price vs SMA({period})"),
            Self::SmaCrossover { fast, slow } => format!("SMA crossover ({fast}/{slow})"),
        }
    }

    fn required_periods(&self) -> Vec<usize> {
        match self {
            Self::BuyAndHold => Vec::new(),
            Self::PriceVsSma { period } => vec![*period],
            Self::SmaCrossover { fast, slow } => vec![*fast, *slow],
        }
    }
}

#[derive(Debug, Clone)]
struct StrategyBacktestContext {
    strategy: InferredBacktestStrategy,
    quantity: f64,
    index_lookup: HashMap<String, HashMap<i64, usize>>,
    sma_lookup: HashMap<String, HashMap<usize, Vec<Option<f64>>>>,
}

impl StrategyBacktestContext {
    fn new(
        strategy: InferredBacktestStrategy,
        quantity: f64,
        data: &HashMap<String, Vec<OHLCV>>,
    ) -> Self {
        let required_periods = strategy.required_periods();
        let index_lookup = data
            .iter()
            .map(|(symbol, bars)| {
                let indices = bars
                    .iter()
                    .enumerate()
                    .map(|(index, bar)| (bar.time.timestamp(), index))
                    .collect::<HashMap<_, _>>();
                (symbol.clone(), indices)
            })
            .collect::<HashMap<_, _>>();

        let sma_lookup = data
            .iter()
            .map(|(symbol, bars)| {
                let series = required_periods
                    .iter()
                    .copied()
                    .map(|period| (period, compute_sma_series(bars, period)))
                    .collect::<HashMap<_, _>>();
                (symbol.clone(), series)
            })
            .collect::<HashMap<_, _>>();

        Self {
            strategy,
            quantity,
            index_lookup,
            sma_lookup,
        }
    }

    fn signal_for(
        &self,
        symbol: &str,
        bar: &OHLCV,
        positions: &HashMap<String, SimPosition>,
    ) -> Option<BacktestSignal> {
        let has_position = positions.contains_key(symbol);
        let index = self
            .index_lookup
            .get(symbol)
            .and_then(|lookup| lookup.get(&bar.time.timestamp()).copied())?;

        match self.strategy {
            InferredBacktestStrategy::BuyAndHold => {
                if !has_position && index == 0 {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.to_string()),
                        quantity: self.quantity,
                    })
                } else {
                    None
                }
            }
            InferredBacktestStrategy::PriceVsSma { period } => {
                let sma = self
                    .sma_lookup
                    .get(symbol)
                    .and_then(|lookup| lookup.get(&period))
                    .and_then(|series| series.get(index))
                    .and_then(|value| *value)?;

                if !has_position && bar.close > sma {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.to_string()),
                        quantity: self.quantity,
                    })
                } else if has_position && bar.close < sma {
                    Some(BacktestSignal::Close {
                        symbol: Symbol(symbol.to_string()),
                    })
                } else {
                    None
                }
            }
            InferredBacktestStrategy::SmaCrossover { fast, slow } => {
                let fast_value = self
                    .sma_lookup
                    .get(symbol)
                    .and_then(|lookup| lookup.get(&fast))
                    .and_then(|series| series.get(index))
                    .and_then(|value| *value)?;
                let slow_value = self
                    .sma_lookup
                    .get(symbol)
                    .and_then(|lookup| lookup.get(&slow))
                    .and_then(|series| series.get(index))
                    .and_then(|value| *value)?;

                if !has_position && fast_value > slow_value {
                    Some(BacktestSignal::Buy {
                        symbol: Symbol(symbol.to_string()),
                        quantity: self.quantity,
                    })
                } else if has_position && fast_value < slow_value {
                    Some(BacktestSignal::Close {
                        symbol: Symbol(symbol.to_string()),
                    })
                } else {
                    None
                }
            }
        }
    }
}

fn strategy_root(runtime_paths: &python_runtime::RuntimePaths) -> Result<PathBuf, String> {
    let root = runtime_paths.strategies_dir().to_path_buf();
    fs::create_dir_all(&root).map_err(|e| format!("Failed to create strategies directory: {e}"))?;
    Ok(root)
}

fn make_strategy_path(root: &Path, full_path: &Path) -> String {
    let relative = full_path
        .strip_prefix(root)
        .unwrap_or(full_path)
        .to_string_lossy()
        .replace('\\', "/");
    format!("strategies/{relative}")
}

fn strategy_name(full_path: &Path) -> Result<String, String> {
    full_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Invalid strategy file name: {:?}", full_path))
}

fn collect_strategy_files(
    directory: &Path,
    root: &Path,
    files: &mut Vec<StrategyFileDto>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|e| format!("Failed to read strategy directory {:?}: {e}", directory))?
    {
        let entry = entry.map_err(|e| format!("Failed to inspect strategy entry: {e}"))?;
        let path = entry.path();

        if path.is_dir() {
            collect_strategy_files(&path, root, files)?;
            continue;
        }

        if path.extension() != Some(OsStr::new("py")) {
            continue;
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read strategy file {:?}: {e}", path))?;
        files.push(StrategyFileDto {
            name: strategy_name(&path)?,
            path: make_strategy_path(root, &path),
            content,
        });
    }

    Ok(())
}

fn ensure_default_strategy(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_strategy_files(root, root, &mut files)?;

    if files.is_empty() {
        fs::write(root.join("my_strategy.py"), DEFAULT_STRATEGY_TEMPLATE)
            .map_err(|e| format!("Failed to create starter strategy: {e}"))?;
    }

    Ok(())
}

fn normalize_strategy_path(
    path: &str,
    runtime_paths: &python_runtime::RuntimePaths,
) -> Result<PathBuf, String> {
    validation::validate_path(path)?;

    let trimmed = path.trim().trim_start_matches("strategies/");
    if trimmed.is_empty() {
        return Err("Strategy path must not be empty".into());
    }
    if Path::new(trimmed).extension() != Some(OsStr::new("py")) {
        return Err("Strategy files must end in .py".into());
    }

    Ok(strategy_root(runtime_paths)?.join(trimmed))
}

fn parse_timeframe(value: &str) -> Result<Timeframe, String> {
    match value.trim().to_lowercase().as_str() {
        "1m" | "m1" => Ok(Timeframe::M1),
        "5m" | "m5" => Ok(Timeframe::M5),
        "15m" | "m15" => Ok(Timeframe::M15),
        "1h" | "h1" => Ok(Timeframe::H1),
        "4h" | "h4" => Ok(Timeframe::H4),
        "1d" | "d1" => Ok(Timeframe::D1),
        "1w" | "w1" => Ok(Timeframe::W1),
        other => Err(format!("Unsupported timeframe: {other}")),
    }
}

fn parse_date_input(value: &str, end_of_day: bool) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date '{value}': {e}"))?;
    let naive = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    }
    .ok_or_else(|| format!("Invalid date boundary for {value}"))?;

    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn currency_for_symbol(symbol: &str) -> &'static str {
    if symbol.ends_with(".NS") || symbol.ends_with(".BO") {
        "INR"
    } else {
        "USD"
    }
}

fn extract_sma_periods(content: &str) -> Vec<usize> {
    let mut periods = BTreeSet::new();

    for line in content.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("indicator") || !lower.contains("sma") {
            continue;
        }

        for segment in line.split(',').rev() {
            let candidate = segment
                .chars()
                .filter(|ch| ch.is_ascii_digit())
                .collect::<String>();

            if let Ok(period) = candidate.parse::<usize>() {
                if (2..=400).contains(&period) {
                    periods.insert(period);
                    break;
                }
            }
        }
    }

    periods.into_iter().collect()
}

fn infer_backtest_strategy(content: &str) -> (InferredBacktestStrategy, Vec<String>) {
    let periods = extract_sma_periods(content);

    if periods.len() >= 2 {
        let fast = periods[0];
        let slow = *periods.last().unwrap_or(&periods[0]);
        return (
            InferredBacktestStrategy::SmaCrossover { fast, slow },
            vec![format!(
                "Detected multiple SMA indicators in the strategy file; using a long-only SMA crossover model ({fast}/{slow})."
            )],
        );
    }

    if let Some(period) = periods.first().copied() {
        return (
            InferredBacktestStrategy::PriceVsSma { period },
            vec![format!(
                "Detected an SMA indicator in the strategy file; using a long-only price-vs-SMA({period}) model."
            )],
        );
    }

    (
        InferredBacktestStrategy::BuyAndHold,
        vec![
            "No supported indicator pattern was detected in the active strategy file; falling back to buy-and-hold for this backtest run.".to_string(),
        ],
    )
}

fn compute_sma_series(bars: &[OHLCV], period: usize) -> Vec<Option<f64>> {
    let mut values = vec![None; bars.len()];
    if period == 0 {
        return values;
    }

    let mut sum = 0.0;
    for (index, bar) in bars.iter().enumerate() {
        sum += bar.close;
        if index >= period {
            sum -= bars[index - period].close;
        }
        if index + 1 >= period {
            values[index] = Some(sum / period as f64);
        }
    }

    values
}

async fn load_backtest_bars(
    state: &AppState,
    symbol: &Symbol,
    timeframe: Timeframe,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(Vec<OHLCV>, String), String> {
    let query = OHLCVQuery {
        symbol: symbol.clone(),
        timeframe: timeframe.clone(),
        from,
        to,
        limit: Some(5_000),
    };

    match state.storage.query_ohlcv(query).await {
        Ok(bars) if !bars.is_empty() => {
            return Ok((bars, "Loaded historical bars from local storage cache.".to_string()));
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(?error, "Local historical cache lookup failed; falling back to Yahoo Finance");
        }
    }

    let adapter = YahooFinanceAdapter::new();
    let bars = adapter
        .get_historical_ohlcv(symbol, timeframe, from, to)
        .await
        .map_err(|e| format!("Failed to fetch historical bars for {}: {e}", symbol.0))?;

    if bars.is_empty() {
        return Err(format!(
            "No historical bars were available for {} in the requested date range.",
            symbol.0
        ));
    }

    Ok((
        bars,
        "Fetched historical bars live from Yahoo Finance for this backtest run.".to_string(),
    ))
}

#[command]
pub async fn list_strategy_files(
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<Vec<StrategyFileDto>, String> {
    let root = strategy_root(runtime_paths.inner())?;
    ensure_default_strategy(&root)?;

    let mut files = Vec::new();
    collect_strategy_files(&root, &root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

#[command]
pub async fn create_strategy_file(
    path: String,
    content: Option<String>,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<StrategyFileDto, String> {
    let runtime_paths = runtime_paths.inner();
    let root = strategy_root(runtime_paths)?;
    let full_path = normalize_strategy_path(&path, runtime_paths)?;

    if full_path.exists() {
        return Err(format!("Strategy file already exists: {}", path));
    }

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create strategy subdirectory {:?}: {e}", parent))?;
    }

    let strategy_content = content.unwrap_or_else(|| DEFAULT_STRATEGY_TEMPLATE.to_string());
    fs::write(&full_path, &strategy_content)
        .map_err(|e| format!("Failed to create strategy file {:?}: {e}", full_path))?;

    Ok(StrategyFileDto {
        name: strategy_name(&full_path)?,
        path: make_strategy_path(&root, &full_path),
        content: strategy_content,
    })
}

#[command]
pub async fn save_strategy_file(
    path: String,
    content: String,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<StrategyFileDto, String> {
    let runtime_paths = runtime_paths.inner();
    let root = strategy_root(runtime_paths)?;
    let full_path = normalize_strategy_path(&path, runtime_paths)?;

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create strategy subdirectory {:?}: {e}", parent))?;
    }

    fs::write(&full_path, &content)
        .map_err(|e| format!("Failed to save strategy file {:?}: {e}", full_path))?;

    Ok(StrategyFileDto {
        name: strategy_name(&full_path)?,
        path: make_strategy_path(&root, &full_path),
        content,
    })
}

#[command]
pub async fn delete_strategy_file(
    path: String,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<bool, String> {
    let full_path = normalize_strategy_path(&path, runtime_paths.inner())?;

    if !full_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&full_path)
        .map_err(|e| format!("Failed to delete strategy file {:?}: {e}", full_path))?;
    Ok(true)
}

#[command]
pub async fn run_strategy_file(
    path: String,
    params_json: Option<String>,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<StrategyExecutionResultDto, String> {
    let runtime_paths = runtime_paths.inner();
    let full_path = normalize_strategy_path(&path, runtime_paths)?;
    if !full_path.exists() {
        return Err(format!("Strategy file not found: {}", path));
    }

    let params_value = match params_json.as_deref() {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| format!("Invalid strategy params JSON: {e}"))?,
        _ => serde_json::json!({}),
    };

    if !params_value.is_object() {
        return Err("Strategy params must be a JSON object".into());
    }

    let python = python_runtime::resolve_python_executable(
        runtime_paths,
        "APEX_STRATEGY_PYTHON_PATH",
        &[],
    )?;
    let socket = std::env::var("APEX_SIDECAR_SOCKET").unwrap_or_else(|_| "/tmp/apex_strategy.sock".into());
    let started_at = Utc::now();
    let output = Command::new(&python)
        .current_dir(runtime_paths.work_root())
        .env("APEX_SIDECAR_SOCKET", &socket)
        .env("PYTHONPATH", python_runtime::build_python_path(runtime_paths)?)
        .env("PYTHONIOENCODING", "utf-8")
        .arg("-m")
        .arg("runtime.strategy_runner")
        .arg("--id")
        .arg(format!("ide-{}", Uuid::new_v4()))
        .arg("--script")
        .arg(full_path.to_string_lossy().to_string())
        .arg("--params")
        .arg(serde_json::to_string(&params_value).map_err(|e| format!("Failed to encode strategy params: {e}"))?)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute strategy with {}: {e}", python.display()))?;
    let finished_at = Utc::now();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    lines.extend(
        stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string),
    );

    if lines.is_empty() {
        lines.push("Strategy completed without console output.".into());
    }

    let error = if output.status.success() {
        None
    } else {
        let stderr_text = stderr.trim();
        let stdout_text = stdout.trim();
        Some(
            if !stderr_text.is_empty() {
                python_runtime::enrich_python_error(
                    stderr_text,
                    &["APEX_STRATEGY_PYTHON_PATH"],
                )
            } else if !stdout_text.is_empty() {
                python_runtime::enrich_python_error(
                    stdout_text,
                    &["APEX_STRATEGY_PYTHON_PATH"],
                )
            } else {
                "Strategy execution failed without a captured error message.".into()
            },
        )
    };

    Ok(StrategyExecutionResultDto {
        success: output.status.success(),
        output: lines,
        error,
        exit_code: output.status.code(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
    })
}

#[command]
pub async fn run_strategy_backtest(
    request: StrategyBacktestRequestDto,
    state: State<'_, AppState>,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<StrategyBacktestResultDto, String> {
    validation::validate_symbol(&request.symbol)?;
    validation::validate_path(&request.path)?;

    let full_path = normalize_strategy_path(&request.path, runtime_paths.inner())?;
    if !full_path.exists() {
        return Err(format!("Strategy file not found: {}", request.path));
    }

    let timeframe = parse_timeframe(&request.timeframe)?;
    let from = parse_date_input(&request.from, false)?;
    let to = parse_date_input(&request.to, true)?;
    if from >= to {
        return Err("Backtest start date must be earlier than the end date".into());
    }

    let initial_capital = request.initial_capital.unwrap_or(100_000.0);
    if !initial_capital.is_finite() || initial_capital <= 0.0 {
        return Err("Initial capital must be a positive finite number".into());
    }

    let commission_bps = request.commission_bps.unwrap_or(3.0);
    if !commission_bps.is_finite() || commission_bps < 0.0 {
        return Err("Commission must be a non-negative finite number".into());
    }

    let slippage_bps = request.slippage_bps.unwrap_or(2.0);
    if !slippage_bps.is_finite() || slippage_bps < 0.0 {
        return Err("Slippage must be a non-negative finite number".into());
    }

    let quantity = request.quantity.unwrap_or(10.0);
    validation::validate_quantity(quantity)?;

    let symbol_text = request.symbol.trim().to_uppercase();
    let symbol = Symbol(symbol_text.clone());
    let content = fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read strategy file {:?}: {e}", full_path))?;
    let (strategy, mut notes) = infer_backtest_strategy(&content);

    let (bars, data_source_note) = load_backtest_bars(&state, &symbol, timeframe.clone(), from, to).await?;
    let bars_analyzed = bars.len();
    if bars_analyzed < 2 {
        return Err("Backtest requires at least two historical bars".into());
    }

    notes.push(data_source_note);
    notes.push(format!(
        "Configured trade size: {quantity:.2} shares/contracts with {:.2} bps commission and {:.2} bps slippage.",
        commission_bps,
        slippage_bps,
    ));

    let mut data = HashMap::new();
    data.insert(symbol_text.clone(), bars);

    let context = StrategyBacktestContext::new(strategy.clone(), quantity, &data);
    let config = BacktestConfig {
        run_id: format!("bt-{}", Uuid::new_v4()),
        symbols: vec![symbol.clone()],
        start: from,
        end: to,
        initial_capital,
        currency: currency_for_symbol(&symbol_text).to_string(),
        commission_bps,
        slippage_bps,
    };
    let mut engine = BacktestEngine::new(config);
    let backtest = engine
        .run(&data, {
            let context = context.clone();
            move |symbol, bar, positions, _cash| context.signal_for(symbol, bar, positions)
        })
        .map_err(|e| format!("Backtest execution failed: {e}"))?;

    Ok(StrategyBacktestResultDto {
        strategy_name: strategy_name(&full_path)?,
        strategy_path: request.path,
        inferred_strategy: strategy.label(),
        symbol: symbol_text,
        timeframe: request.timeframe.to_lowercase(),
        bars_analyzed,
        metrics: backtest.metrics,
        trades: backtest.trades,
        equity_curve: backtest.equity_curve,
        notes,
    })
}
