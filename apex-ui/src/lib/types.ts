// DTO types matching the Rust backend
export interface QuoteDto {
  symbol: string;
  bid: number;
  ask: number;
  last: number;
  open: number;
  high: number;
  low: number;
  volume: number;
  change_pct: number;
  vwap: number;
  updated_at: string;
}

export interface OHLCVDto {
  time: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface OrderDto {
  id: string;
  symbol: string;
  side: string;
  order_type: string;
  quantity: number;
  price: number | null;
  stop_price: number | null;
  status: string;
  filled_qty: number;
  avg_price: number;
  created_at: string;
  updated_at: string;
  broker_id: string;
  source: string;
}

export interface PositionDto {
  symbol: string;
  quantity: number;
  avg_price: number;
  side: string;
  pnl: number;
  pnl_pct: number;
  broker_id: string;
}

export interface AlertDto {
  rule_id: string;
  message: string;
  severity: string;
}

export interface AlertRuleDto {
  id: string;
  rule: string;
  enabled: boolean;
}

export interface RiskStatusDto {
  session_pnl: number;
  is_halted: boolean;
  max_daily_loss: number;
}

export interface GeneralSettingsDto {
  data_dir: string;
}

export interface AdapterPreferenceDto {
  adapter: string;
  available_adapters: string[];
}

export interface RiskSettingsDto {
  max_daily_loss: number;
  max_order_value: number;
}

export interface StorageSettingsDto {
  backend: string;
  sqlite_path: string;
  postgres_url: string;
  wal_mode: boolean;
  pool_size: number;
  available_backends: string[];
}

export interface AppSettingsDto {
  config_path: string;
  runtime_storage_backend: string;
  runtime_storage_target: string;
  general: GeneralSettingsDto;
  market_data: AdapterPreferenceDto;
  execution: AdapterPreferenceDto;
  risk: RiskSettingsDto;
  storage: StorageSettingsDto;
}

export interface AppSettingsUpdateDto {
  general: GeneralSettingsDto;
  market_data: Pick<AdapterPreferenceDto, 'adapter'>;
  execution: Pick<AdapterPreferenceDto, 'adapter'>;
  risk: RiskSettingsDto;
  storage: Omit<StorageSettingsDto, 'available_backends'>;
}

export interface NewOrderRequestDto {
  symbol: string;
  side: string;
  order_type: string;
  quantity: number;
  price: number | null;
  stop_price: number | null;
  broker_id: string;
  tag: string | null;
}

export interface AccountBalanceDto {
  total_value: number;
  cash: number;
  margin_used: number;
  margin_available: number;
  unrealized_pnl: number;
  realized_pnl: number;
  currency: string;
}

export interface StrategyFileDto {
  name: string;
  path: string;
  content: string;
}

export interface StrategyExecutionResultDto {
  success: boolean;
  output: string[];
  error: string | null;
  exit_code: number | null;
  started_at: string;
  finished_at: string;
}

export interface StrategyBacktestRequestDto {
  path: string;
  symbol: string;
  timeframe: string;
  from: string;
  to: string;
  initial_capital?: number;
  commission_bps?: number;
  slippage_bps?: number;
  quantity?: number;
}

export interface BacktestMetricsDto {
  total_return: number;
  total_return_pct: number;
  annualized_return_pct: number;
  sharpe_ratio: number;
  max_drawdown: number;
  max_drawdown_pct: number;
  total_trades: number;
  winning_trades: number;
  losing_trades: number;
  win_rate: number;
  profit_factor: number;
  avg_trade_pnl: number;
  avg_win: number;
  avg_loss: number;
  max_consecutive_wins: number;
  max_consecutive_losses: number;
  final_equity: number;
}

export interface BacktestTradeDto {
  symbol: string;
  side: string;
  entry_time: string;
  entry_price: number;
  exit_time: string | null;
  exit_price: number | null;
  quantity: number;
  pnl: number;
  commission: number;
}

export interface EquityPointDto {
  time: string;
  equity: number;
  drawdown: number;
}

export interface StrategyBacktestResultDto {
  strategy_name: string;
  strategy_path: string;
  inferred_strategy: string;
  symbol: string;
  timeframe: string;
  bars_analyzed: number;
  metrics: BacktestMetricsDto;
  trades: BacktestTradeDto[];
  equity_curve: EquityPointDto[];
  notes: string[];
}

export interface NotebookCellDto {
  id: string;
  kind: 'code' | 'markdown';
  content: string;
  output: string | null;
}

export interface NotebookDocumentDto {
  title: string;
  path: string;
  cells: NotebookCellDto[];
  updated_at: string;
}

export interface NotebookSummaryDto {
  name: string;
  path: string;
  updated_at: string;
}

export interface NotebookRunRequestDto {
  path: string;
  cells: NotebookCellDto[];
  cell_id: string;
}

export interface NotebookCellExecutionDto {
  cell_id: string;
  success: boolean;
  stdout: string;
  stderr: string;
  finished_at: string;
}

// ML Workbench types
export interface MLModelDto {
  model_id: string;
  algorithm: string;
  status: 'idle' | 'training' | 'completed' | 'failed';
  metrics: Record<string, number>;
  feature_names: string[];
  created_at: string;
  data_path: string;
  target_column: string;
}

export interface MLTrainingRequestDto {
  algorithm: string;
  data_path: string;
  target_column: string;
  feature_columns: string[];
  hyperparams: Record<string, number | string>;
  n_splits: number;
  lag_periods: number[];
}

export interface MLTrainingResultDto {
  model_id: string;
  metrics: Record<string, number>;
  feature_names: string[];
  status: string;
}

// Health Monitor types
export interface AdapterHealthDto {
  adapter_id: string;
  adapter_type: string;
  status: 'healthy' | 'degraded' | 'unhealthy';
  message: string;
  last_check: string;
}

export interface SystemHealthDto {
  adapters: AdapterHealthDto[];
  uptime_secs: number;
  memory_usage_mb: number;
  active_subscriptions: number;
  open_orders: number;
  active_strategies: number;
}

export interface BrokerConnectionDto {
  broker_id: string;
  display_name: string;
  mode: 'paper' | 'live';
  status: 'ready' | 'connected' | 'auth_required' | 'not_configured' | 'degraded' | 'unhealthy' | string;
  configured: boolean;
  authenticated: boolean;
  execution_available: boolean;
  market_data_available: boolean;
  token_field_label: string;
  message: string;
}
