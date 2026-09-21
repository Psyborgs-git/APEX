use crate::commands::ml::ModelRegistry;
use crate::commands::python_runtime;
use crate::commands::scanner::ScanCriterionDto;
use crate::commands::strategy::StrategyBacktestRequestDto;
use crate::commands::{data, ml, quant, scanner, strategy};
use crate::dto::MLTrainingRequestDto;
use crate::state::AppState;
use crate::validation;
use apex_core::application::quant as q;
use apex_core::domain::models::{Symbol, Timeframe};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
pub struct CopilotMessage {
    /// "user" or "assistant"
    pub role: String,
    pub content: String,
}

/// One step of the agentic tool-call loop, surfaced in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallTrace {
    pub name: String,
    pub detail: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopilotReply {
    pub reply: String,
    pub model: String,
    pub provider: String,
    pub tool_calls: Vec<ToolCallTrace>,
}

// ── Provider resolution ─────────────────────────────────────────────

/// Fully-resolved inference target for this request.
struct ResolvedProvider {
    id: String,
    base_url: String,
    model: String,
    /// "chat" | "responses" | "acp"
    api_kind: String,
    api_key: String,
    max_tokens: u32,
}

fn env_key(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn resolve_provider(state: &AppState) -> Result<ResolvedProvider, String> {
    // Provider registry takes precedence when an active id is set.
    if !state.llm.active.trim().is_empty() {
        let provider = state
            .llm
            .providers
            .iter()
            .find(|p| p.id == state.llm.active)
            .ok_or_else(|| {
                format!(
                    "Active LLM provider `{}` not found in llm.providers",
                    state.llm.active
                )
            })?;
        return Ok(ResolvedProvider {
            id: provider.id.clone(),
            base_url: provider.base_url.trim().trim_end_matches('/').to_string(),
            model: provider.model.clone(),
            api_kind: provider.api_kind.clone(),
            api_key: env_key(&provider.api_key_env).unwrap_or_default(),
            // 0 = "use the legacy [copilot] limit" (see LlmProviderConfig docs)
            max_tokens: if provider.max_tokens == 0 {
                state.copilot.max_tokens
            } else {
                provider.max_tokens
            },
        });
    }

    // Legacy fallback: [copilot] section + OPEN_ROUTER env var.
    if !state.copilot.enabled {
        return Err(
            "Copilot is disabled — enable it in settings or configure an LLM provider".to_string(),
        );
    }
    let api_key = ["OPEN_ROUTER", "OPENROUTER_API_KEY"]
        .iter()
        .find_map(|n| env_key(n))
        .ok_or_else(|| {
            "No LLM provider configured — set OPEN_ROUTER or add a provider in Settings".to_string()
        })?;
    Ok(ResolvedProvider {
        id: "copilot".to_string(),
        base_url: state
            .copilot
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_string(),
        model: state.copilot.model.clone(),
        api_kind: "chat".to_string(),
        api_key,
        max_tokens: state.copilot.max_tokens,
    })
}

// ── Tool schemas (OpenAI function-calling format) ───────────────────

fn tool_schemas() -> Value {
    let tools = [
        ("get_quote", "Latest quote for a watchlist symbol", json!({
            "type": "object", "properties": {"symbol": {"type": "string"}},
            "required": ["symbol"] })),
        ("get_ohlcv", "Recent OHLCV bars (last N closes) for a symbol", json!({
            "type": "object", "properties": {
                "symbol": {"type": "string"},
                "timeframe": {"type": "string", "default": "d1"},
                "limit": {"type": "integer", "default": 60} },
            "required": ["symbol"] })),
        ("get_quant_stats", "Quantitative summary (returns, vol, Sharpe, skew, drawdown, autocorr) for a symbol", json!({
            "type": "object", "properties": {
                "symbol": {"type": "string"},
                "timeframe": {"type": "string", "default": "d1"} },
            "required": ["symbol"] })),
        ("get_regression", "OLS regression of y returns on x returns (beta/alpha/R²) — e.g. a stock vs its index", json!({
            "type": "object", "properties": {
                "x_symbol": {"type": "string"},
                "y_symbol": {"type": "string"},
                "timeframe": {"type": "string", "default": "d1"} },
            "required": ["x_symbol", "y_symbol"] })),
        ("run_scan", "Screen symbols against criteria (price_above/below, pct_change_above/below, volume_above, above_sma/below_sma)", json!({
            "type": "object", "properties": {
                "symbols": {"type": "array", "items": {"type": "string"}},
                "criteria": {"type": "array", "items": {"type": "object"}},
                "timeframe": {"type": "string", "default": "d1"} },
            "required": ["criteria"] })),
        ("get_news", "Latest market news headlines", json!({
            "type": "object", "properties": {
                "limit": {"type": "integer", "default": 8},
                "symbol": {"type": "string"} } })),
        ("list_strategies", "List strategy .py files in the strategies directory", json!({
            "type": "object", "properties": {} })),
        ("read_strategy", "Read a strategy .py file's source", json!({
            "type": "object", "properties": {"path": {"type": "string"}},
            "required": ["path"] })),
        ("save_strategy", "Create or overwrite a strategy .py file (returns its path)", json!({
            "type": "object", "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"} },
            "required": ["path", "content"] })),
        ("run_backtest", "Backtest a strategy file on a symbol; returns metrics and equity summary", json!({
            "type": "object", "properties": {
                "path": {"type": "string"},
                "symbol": {"type": "string"},
                "timeframe": {"type": "string", "default": "d1"},
                "from": {"type": "string"}, "to": {"type": "string"},
                "quantity": {"type": "number", "default": 10},
                "initial_capital": {"type": "number", "default": 100000} },
            "required": ["path", "symbol"] })),
        ("export_bars_csv", "Export historical bars for a symbol to a CSV under data/exports — returns the path usable as data_path for train_ml_model", json!({
            "type": "object", "properties": {
                "symbol": {"type": "string"},
                "timeframe": {"type": "string", "default": "d1"},
                "limit": {"type": "integer", "default": 500} },
            "required": ["symbol"] })),
        ("list_ml_models", "List trained ML models with metrics", json!({
            "type": "object", "properties": {} })),
        ("train_ml_model", "Train an ML model on a CSV dataset (use export_bars_csv first); returns model_id and metrics", json!({
            "type": "object", "properties": {
                "algorithm": {"type": "string", "description": "random_forest, logistic_regression, linear_regression, gradient_boosting, svm"},
                "data_path": {"type": "string"},
                "target_column": {"type": "string"},
                "feature_columns": {"type": "array", "items": {"type": "string"}},
                "n_splits": {"type": "integer", "default": 5},
                "lag_periods": {"type": "array", "items": {"type": "integer"}, "default": [1]} },
            "required": ["algorithm", "data_path", "target_column", "feature_columns"] })),
    ];
    Value::Array(
        tools
            .iter()
            .map(|(name, desc, params)| {
                json!({
                    "type": "function",
                    "function": {"name": name, "description": desc, "parameters": params},
                })
            })
            .collect(),
    )
}

fn parse_timeframe(s: &str) -> Timeframe {
    match s.to_lowercase().as_str() {
        "1m" | "m1" => Timeframe::M1,
        "5m" | "m5" => Timeframe::M5,
        "15m" | "m15" => Timeframe::M15,
        "1h" | "h1" => Timeframe::H1,
        "4h" | "h4" => Timeframe::H4,
        "1w" | "w1" => Timeframe::W1,
        _ => Timeframe::D1,
    }
}

fn clamp_limit(v: Option<&Value>, default: usize, max: usize) -> usize {
    v.and_then(|x| x.as_u64())
        .map(|n| (n as usize).clamp(1, max))
        .unwrap_or(default)
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing `{key}`"))
}

/// Execute one agentic tool against live terminal state. Returns a compact
/// JSON payload (or an `{"error": ...}` payload — tool failures are fed back
/// to the model so it can retry with different arguments).
async fn exec_tool(
    name: &str,
    args: &Value,
    state: &AppState,
    models: &ModelRegistry,
    runtime_paths: &python_runtime::RuntimePaths,
) -> Result<Value, String> {
    match name {
        "get_quote" => {
            let sym = arg_str(args, "symbol")?.to_uppercase();
            match state.aggregator.get_cached_quote(&sym) {
                Some(q) => Ok(json!({
                    "symbol": sym, "last": q.last, "change_pct": q.change_pct,
                    "volume": q.volume, "high": q.high, "low": q.low,
                })),
                None => Err(format!(
                    "no cached quote for {sym} — add it to the watchlist"
                )),
            }
        }
        "get_ohlcv" => {
            let sym = arg_str(args, "symbol")?;
            let tf = args.get("timeframe").and_then(|v| v.as_str());
            let limit = clamp_limit(args.get("limit"), 60, 300);
            let bars = data::load_bars(state, sym, tf, limit).await?;
            let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
            Ok(json!({
                "symbol": sym, "bars": bars.len(),
                "first": bars.first().map(|b| b.time.to_rfc3339()),
                "last": bars.last().map(|b| b.time.to_rfc3339()),
                "closes": closes.iter().rev().take(limit).rev().collect::<Vec<_>>(),
            }))
        }
        "get_quant_stats" => {
            let sym = arg_str(args, "symbol")?;
            let tf = args.get("timeframe").and_then(|v| v.as_str());
            let bars = data::load_bars(state, sym, tf, 800).await?;
            if bars.len() < 30 {
                return Err(format!("only {} bars for {sym} — need 30+", bars.len()));
            }
            let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
            let s = q::summarize(&closes, quant::periods_per_year(&bars));
            Ok(serde_json::to_value(&s).map_err(|e| e.to_string())?)
        }
        "get_regression" => {
            let x = arg_str(args, "x_symbol")?;
            let y = arg_str(args, "y_symbol")?;
            let tf = args.get("timeframe").and_then(|v| v.as_str());
            let r = quant::regression_inner(x.to_string(), y.to_string(), tf, state).await?;
            Ok(json!({"x": r.x_symbol, "y": r.y_symbol, "n": r.n,
                      "alpha": r.alpha, "beta": r.beta, "r_squared": r.r_squared}))
        }
        "run_scan" => {
            let criteria_raw = args
                .get("criteria")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let criteria: Vec<_> = criteria_raw
                .iter()
                .map(|c| {
                    scanner::to_criterion(
                        &serde_json::from_value::<ScanCriterionDto>(c.clone())
                            .map_err(|e| format!("bad criterion: {e}"))?,
                    )
                })
                .collect::<Result<_, String>>()?;
            if criteria.is_empty() {
                return Err("criteria must be a non-empty array".into());
            }
            let universe: Vec<Symbol> = match args.get("symbols").and_then(|v| v.as_array()) {
                Some(list) if !list.is_empty() => list
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| Symbol(s.to_uppercase())))
                    .collect(),
                _ => state
                    .aggregator
                    .quote_cache()
                    .iter()
                    .map(|e| Symbol(e.key().clone()))
                    .collect(),
            };
            if universe.is_empty() {
                return Err("no symbols in universe — add symbols to the watchlist".into());
            }
            let tf = parse_timeframe(
                args.get("timeframe")
                    .and_then(|v| v.as_str())
                    .unwrap_or("d1"),
            );
            let out = state
                .scanner
                .run_scan(&apex_core::application::scanner::ScanConfig {
                    name: "copilot scan".into(),
                    universe,
                    criteria,
                    timeframe: tf,
                    lookback_bars: 60,
                })
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "matched": out.results.len(),
                "results": out.results.iter().take(25).map(|r| json!({
                    "symbol": r.symbol.0,
                    "last": r.last_price,
                    "change_pct": r.change_pct,
                })).collect::<Vec<_>>(),
            }))
        }
        "get_news" => {
            let limit = clamp_limit(args.get("limit"), 8, 25);
            let items = match args.get("symbol").and_then(|v| v.as_str()) {
                Some(sym) => state
                    .news
                    .get_news_for_symbol(&Symbol(sym.to_uppercase()), limit),
                None => state.news.latest_news(limit),
            };
            Ok(
                json!({"count": items.len(), "items": items.iter().map(|i| json!({
                "headline": i.headline, "source": i.source,
            })).collect::<Vec<_>>()}),
            )
        }
        "list_strategies" => {
            let files = strategy::list_strategy_files_inner(runtime_paths)?;
            Ok(json!({"files": files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()}))
        }
        "read_strategy" => {
            let path = arg_str(args, "path")?;
            let full = strategy::normalize_strategy_path(path, runtime_paths)?;
            let content =
                std::fs::read_to_string(&full).map_err(|e| format!("cannot read {path}: {e}"))?;
            Ok(json!({"path": path, "content": content.chars().take(12000).collect::<String>()}))
        }
        "save_strategy" => {
            let path = arg_str(args, "path")?;
            let content = arg_str(args, "content")?;
            let full = strategy::normalize_strategy_path(path, runtime_paths)?;
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&full, content).map_err(|e| format!("cannot write {path}: {e}"))?;
            Ok(json!({"path": path, "saved": true}))
        }
        "run_backtest" => {
            let req = StrategyBacktestRequestDto {
                path: arg_str(args, "path")?.to_string(),
                symbol: arg_str(args, "symbol")?.to_uppercase(),
                timeframe: args
                    .get("timeframe")
                    .and_then(|v| v.as_str())
                    .unwrap_or("d1")
                    .to_string(),
                from: args
                    .get("from")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        (chrono::Utc::now() - chrono::Duration::days(730))
                            .format("%Y-%m-%d")
                            .to_string()
                    }),
                to: args
                    .get("to")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string()),
                initial_capital: args.get("initial_capital").and_then(|v| v.as_f64()),
                commission_bps: args.get("commission_bps").and_then(|v| v.as_f64()),
                slippage_bps: args.get("slippage_bps").and_then(|v| v.as_f64()),
                quantity: args.get("quantity").and_then(|v| v.as_f64()),
            };
            let r = strategy::run_backtest_inner(req, state, runtime_paths).await?;
            Ok(json!({
                "strategy": r.strategy_name, "inferred": r.inferred_strategy,
                "symbol": r.symbol, "bars": r.bars_analyzed,
                "metrics": r.metrics,
                "trades": r.trades.len(),
                "equity_final": r.equity_curve.last().map(|p| p.equity),
                "notes": r.notes,
            }))
        }
        "export_bars_csv" => {
            let sym = arg_str(args, "symbol")?;
            let tf = args
                .get("timeframe")
                .and_then(|v| v.as_str())
                .unwrap_or("d1");
            let limit = clamp_limit(args.get("limit"), 500, 2000);
            let bars = data::load_bars(state, sym, Some(tf), limit).await?;
            if bars.is_empty() {
                return Err(format!("no bars for {sym}"));
            }
            let dir = runtime_paths.data_dir().join("exports");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let safe = sym.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-', "_");
            let path = dir.join(format!("{safe}_{tf}.csv"));
            let mut out = String::from("time,open,high,low,close,volume\n");
            for b in &bars {
                out.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    b.time.format("%Y-%m-%dT%H:%M:%SZ"),
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.volume
                ));
            }
            std::fs::write(&path, &out).map_err(|e| e.to_string())?;
            // Return a user-relative path usable by train_ml_model / strategy files.
            let rel = path
                .strip_prefix(runtime_paths.data_dir())
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            Ok(json!({"path": rel, "bars": bars.len()}))
        }
        "list_ml_models" => {
            let models = ml::load_registered_models(&models.models_dir)?;
            Ok(
                json!({"count": models.len(), "models": models.iter().map(|m| json!({
                "id": m.model_id, "algorithm": m.algorithm,
                "metrics": m.metrics, "target": m.target_column,
            })).collect::<Vec<_>>()}),
            )
        }
        "train_ml_model" => {
            let req = MLTrainingRequestDto {
                algorithm: arg_str(args, "algorithm")?.to_string(),
                data_path: arg_str(args, "data_path")?.to_string(),
                target_column: arg_str(args, "target_column")?.to_string(),
                feature_columns: args
                    .get("feature_columns")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                hyperparams: args
                    .get("hyperparams")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                n_splits: clamp_limit(args.get("n_splits"), 5, 20),
                lag_periods: args
                    .get("lag_periods")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
                    .unwrap_or_else(|| vec![1]),
            };
            let r = ml::train_ml_model_inner(req, &models.models_dir, runtime_paths).await?;
            Ok(json!({"model_id": r.model_id, "metrics": r.metrics,
                      "features": r.feature_names}))
        }
        other => Err(format!("unknown tool `{other}`")),
    }
}

/// Describe a tool call for the UI trace (name + compact arg summary).
fn trace_detail(name: &str, args: &Value) -> String {
    let pick = |keys: &[&str]| -> String {
        keys.iter()
            .filter_map(|k| {
                args.get(*k).map(|v| {
                    format!(
                        "{k}={}",
                        v.as_str()
                            .map(String::from)
                            .unwrap_or_else(|| v.to_string())
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    match name {
        "save_strategy" | "read_strategy" => pick(&["path"]),
        "run_backtest" => pick(&["path", "symbol", "timeframe"]),
        "train_ml_model" => pick(&["algorithm", "data_path", "target_column"]),
        "get_regression" => pick(&["x_symbol", "y_symbol"]),
        _ => pick(&["symbol", "timeframe", "path"]),
    }
}

// ── Chat-completions tool loop ──────────────────────────────────────

async fn chat_round(
    http: &reqwest::Client,
    provider: &ResolvedProvider,
    messages: &[Value],
    tools: &Value,
) -> Result<Value, String> {
    let resp = http
        .post(format!("{}/chat/completions", provider.base_url))
        .bearer_auth(&provider.api_key)
        .header("HTTP-Referer", "https://apex.local")
        .header("X-Title", "APEX Terminal")
        .json(&json!({
            "model": provider.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "max_tokens": provider.max_tokens,
        }))
        .send()
        .await
        .map_err(|e| format!("{} request failed: {e}", provider.id))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{} error {}: {}",
            provider.id,
            status,
            text.chars().take(300).collect::<String>()
        ));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse {} response: {e}", provider.id))
}

// ── Responses-API tool loop ─────────────────────────────────────────
// /responses uses flat tool defs and output items instead of messages.

fn chat_tools_to_responses(tools: &Value) -> Value {
    tools
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|t| {
                    let f = &t["function"];
                    json!({"type": "function", "name": f["name"],
                           "description": f["description"], "parameters": f["parameters"]})
                })
                .collect::<Vec<_>>()
        })
        .map(Value::Array)
        .unwrap_or(Value::Array(vec![]))
}

async fn responses_round(
    http: &reqwest::Client,
    provider: &ResolvedProvider,
    input: &[Value],
    tools: &Value,
) -> Result<Value, String> {
    let resp = http
        .post(format!("{}/responses", provider.base_url))
        .bearer_auth(&provider.api_key)
        .header("HTTP-Referer", "https://apex.local")
        .header("X-Title", "APEX Terminal")
        .json(&json!({
            "model": provider.model,
            "input": input,
            "tools": tools,
            "max_output_tokens": provider.max_tokens,
        }))
        .send()
        .await
        .map_err(|e| format!("{} request failed: {e}", provider.id))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{} error {}: {}",
            provider.id,
            status,
            text.chars().take(300).collect::<String>()
        ));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse {} response: {e}", provider.id))
}

const MAX_AGENT_ROUNDS: usize = 10;

/// Chat with the AI copilot. Resolves the configured provider
/// ([[llm.providers]] or legacy [copilot]), then runs an agentic
/// tool-call loop that can read live quotes, run scans/backtests,
/// write strategies, and train models — feeding results back so the
/// model can iterate (build → test → optimise).
#[tauri::command]
pub async fn copilot_chat(
    message: String,
    history: Option<Vec<CopilotMessage>>,
    state: State<'_, AppState>,
    models: State<'_, ModelRegistry>,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<CopilotReply, String> {
    validation::validate_string_length(&message, "message")?;
    let provider = resolve_provider(&state)?;

    // ACP providers hand the prompt to an external agent process.
    if provider.api_kind == "acp" {
        let reply = crate::commands::acp::acp_prompt(
            state.inner(),
            &message,
            history.as_deref().unwrap_or(&[]),
        )
        .await?;
        return Ok(CopilotReply {
            reply,
            model: provider.model,
            provider: provider.id,
            tool_calls: vec![],
        });
    }

    // Live context for the system prompt.
    let positions = state.otm.get_positions();
    let quotes: Vec<String> = state
        .aggregator
        .quote_cache()
        .iter()
        .take(20)
        .map(|e| {
            let q = e.value();
            format!("{} last={:.2} chg={:+.2}%", e.key(), q.last, q.change_pct)
        })
        .collect();
    let pos_text: Vec<String> = positions
        .iter()
        .take(20)
        .map(|p| {
            format!(
                "{} qty={:.0} avg={:.2} pnl={:+.2}",
                p.symbol.0, p.quantity, p.avg_price, p.pnl
            )
        })
        .collect();

    let system_prompt = format!(
        "You are APEX Copilot, an agentic assistant inside a local trading terminal. \
         You have tools to read live market data, run scans/regressions/quant stats, \
         write strategy files, backtest them, export OHLCV datasets, and train ML models. \
         When asked to build or optimise a strategy or model, iterate: fetch data, write \
         the strategy, run a backtest, inspect metrics, adjust, and re-run. \
         Answer tersely, never invent prices — use tools or the live context below. \
         This is decision support, not financial advice.\n\n\
         LIVE WATCHLIST QUOTES:\n{}\n\nOPEN POSITIONS:\n{}\n\nSESSION P&L: {:.2}",
        if quotes.is_empty() {
            "(no quotes loaded)".into()
        } else {
            quotes.join("\n")
        },
        if pos_text.is_empty() {
            "(no open positions)".into()
        } else {
            pos_text.join("\n")
        },
        state.risk.session_pnl(),
    );

    let mut trace: Vec<ToolCallTrace> = Vec::new();
    let state_ref = state.inner();
    let models_ref = models.inner();
    let paths_ref = runtime_paths.inner();

    match provider.api_kind.as_str() {
        "responses" => {
            let tools = chat_tools_to_responses(&tool_schemas());
            let mut input: Vec<Value> = vec![json!({
                "role": "developer",
                "content": [{"type": "input_text", "text": system_prompt}],
            })];
            if let Some(history) = &history {
                for m in history.iter().rev().take(16).rev() {
                    let role = if m.role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    let kind = if role == "assistant" {
                        "output_text"
                    } else {
                        "input_text"
                    };
                    input.push(json!({"role": role,
                        "content": [{"type": kind, "text": m.content}]}));
                }
            }
            input.push(json!({"role": "user",
                "content": [{"type": "input_text", "text": message}]}));

            let mut reply_text = String::new();
            let mut model_name = provider.model.clone();
            for _ in 0..MAX_AGENT_ROUNDS {
                let resp = responses_round(&state.http, &provider, &input, &tools).await?;
                if let Some(m) = resp.get("model").and_then(|v| v.as_str()) {
                    model_name = m.to_string();
                }
                let output = resp
                    .get("output")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let calls: Vec<&Value> = output
                    .iter()
                    .filter(|o| o.get("type").and_then(|t| t.as_str()) == Some("function_call"))
                    .collect();
                // Collect final text from message/output_text items.
                for o in &output {
                    if o.get("type").and_then(|t| t.as_str()) == Some("message") {
                        if let Some(content) = o.get("content").and_then(|c| c.as_array()) {
                            reply_text = content
                                .iter()
                                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n");
                        }
                    }
                }
                if calls.is_empty() {
                    break;
                }
                for call in calls {
                    input.push(call.clone());
                    let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let args: Value = call
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!({}));
                    let call_id = call
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let result = exec_tool(name, &args, state_ref, models_ref, paths_ref).await;
                    let (payload, ok) = match result {
                        Ok(v) => (v, true),
                        Err(e) => (json!({"error": e}), false),
                    };
                    trace.push(ToolCallTrace {
                        name: name.to_string(),
                        detail: trace_detail(name, &args),
                        ok,
                    });
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "output": payload.to_string(),
                    }));
                }
            }
            Ok(CopilotReply {
                reply: if reply_text.is_empty() {
                    "(empty response)".into()
                } else {
                    reply_text
                },
                model: model_name,
                provider: provider.id,
                tool_calls: trace,
            })
        }
        _ => {
            // OpenAI chat/completions loop (default + "chat").
            let tools = tool_schemas();
            let mut messages = vec![json!({"role": "system", "content": system_prompt})];
            if let Some(history) = &history {
                for m in history.iter().rev().take(16).rev() {
                    let role = if m.role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    messages.push(json!({"role": role, "content": m.content}));
                }
            }
            messages.push(json!({"role": "user", "content": message}));

            let mut reply_text = String::new();
            let mut model_name = provider.model.clone();
            for _ in 0..MAX_AGENT_ROUNDS {
                let resp = chat_round(&state.http, &provider, &messages, &tools).await?;
                if let Some(m) = resp.get("model").and_then(|v| v.as_str()) {
                    model_name = m.to_string();
                }
                let msg = resp
                    .get("choices")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|c| c.get("message"))
                    .cloned()
                    .unwrap_or(json!({}));
                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                    reply_text = content.to_string();
                }
                let calls = msg
                    .get("tool_calls")
                    .and_then(|t| t.as_array())
                    .cloned()
                    .unwrap_or_default();
                if calls.is_empty() {
                    break;
                }
                messages.push(msg);
                for call in calls {
                    let fname = call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args: Value = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!({}));
                    let id = call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let result = exec_tool(fname, &args, state_ref, models_ref, paths_ref).await;
                    let (payload, ok) = match result {
                        Ok(v) => (v, true),
                        Err(e) => (json!({"error": e}), false),
                    };
                    trace.push(ToolCallTrace {
                        name: fname.to_string(),
                        detail: trace_detail(fname, &args),
                        ok,
                    });
                    messages.push(json!({
                        "role": "tool", "tool_call_id": id,
                        "content": payload.to_string(),
                    }));
                }
            }
            Ok(CopilotReply {
                reply: if reply_text.is_empty() {
                    "(empty response)".into()
                } else {
                    reply_text
                },
                model: model_name,
                provider: provider.id,
                tool_calls: trace,
            })
        }
    }
}
