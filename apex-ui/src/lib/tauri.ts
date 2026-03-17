// Typed wrappers for Tauri IPC commands
// In development without Tauri, these return mock data
import type { QuoteDto, OrderDto, PositionDto, NewOrderRequestDto, RiskStatusDto } from './types';

const IS_TAURI = typeof window !== 'undefined' && '__TAURI__' in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (IS_TAURI) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(cmd, args);
  }
  // Mock responses for development
  console.log(`[Mock IPC] ${cmd}`, args);
  return {} as T;
}

export async function getQuote(symbol: string): Promise<QuoteDto> {
  return invoke<QuoteDto>('get_quote', { symbol });
}

export async function subscribeSymbols(symbols: string[]): Promise<void> {
  return invoke<void>('subscribe_symbols', { symbols });
}

export async function placeOrder(request: NewOrderRequestDto): Promise<string> {
  return invoke<string>('place_order', { request });
}

export async function cancelOrder(orderId: string, brokerId: string): Promise<void> {
  return invoke<void>('cancel_order', { order_id: orderId, broker_id: brokerId });
}

export async function getPositions(): Promise<PositionDto[]> {
  return invoke<PositionDto[]>('get_positions');
}

export async function getOpenOrders(): Promise<OrderDto[]> {
  return invoke<OrderDto[]>('get_open_orders');
}

export async function getRiskStatus(): Promise<RiskStatusDto> {
  return invoke<RiskStatusDto>('get_risk_status');
}

export async function resetHalt(): Promise<void> {
  return invoke<void>('reset_halt');
}

export async function addAlert(id: string, ruleJson: string): Promise<void> {
  return invoke<void>('add_alert', { id, rule_json: ruleJson });
}

export async function removeAlert(ruleId: string): Promise<boolean> {
  return invoke<boolean>('remove_alert', { rule_id: ruleId });
}
