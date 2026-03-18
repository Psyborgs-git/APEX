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

export interface RiskStatusDto {
  session_pnl: number;
  is_halted: boolean;
  max_daily_loss: number;
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
