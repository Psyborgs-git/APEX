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
