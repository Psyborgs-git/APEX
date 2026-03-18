import type { QuoteDto, OrderDto, PositionDto, NewOrderRequestDto, RiskStatusDto, MLModelDto, MLTrainingRequestDto, MLTrainingResultDto, SystemHealthDto, AdapterHealthDto } from './types';

const IS_TAURI = typeof window !== 'undefined' && '__TAURI__' in window;

let mockPositions: PositionDto[] = [];
let mockOrders: OrderDto[] = [];
let mockModels: MLModelDto[] = [];
let mockTrainingCounter = 0;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (IS_TAURI) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(cmd, args);
  }
  
  if (cmd === 'get_quote') {
    const sym = (args as any).symbol;
    return { symbol: sym, bid: 150, ask: 150.5, last: 150.2, open: 149, high: 151, low: 148, volume: 1000, change_pct: 0.5, vwap: 150, updated_at: new Date().toISOString() } as any;
  }
  if (cmd === 'place_order') {
    const req = (args?.request as NewOrderRequestDto);
    const order: OrderDto = { id: Math.random().toString(), symbol: req.symbol, side: req.side, order_type: req.order_type, quantity: req.quantity, price: req.price, stop_price: req.stop_price, status: 'Filled', filled_qty: req.quantity, avg_price: 150.2, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), broker_id: req.broker_id, source: 'mock' };
    mockOrders.push(order);
    
    let p = mockPositions.find(x => x.symbol === req.symbol);
    if (!p) {
        p = { symbol: req.symbol, quantity: 0, avg_price: 150.2, side: req.side, pnl: Math.random()*100, pnl_pct: Math.random(), broker_id: req.broker_id };
        mockPositions.push(p);
    }
    p.quantity += (req.side.toUpperCase() === 'BUY' ? Number(req.quantity) : -Number(req.quantity));
    return order.id as any;
  }
  if (cmd === 'get_positions') return [...mockPositions] as any;
  if (cmd === 'get_open_orders') return [...mockOrders] as any;
  if (cmd === 'get_risk_status') return { session_pnl: mockPositions.reduce((a,b)=>a+b.pnl, 0), is_halted: false, max_daily_loss: 50000 } as any;
  if (cmd === 'list_ml_models') return [...mockModels] as any;
  if (cmd === 'train_ml_model') {
    const req = args?.request as MLTrainingRequestDto;
    mockTrainingCounter++;
    const modelId = `model_${mockTrainingCounter}`;
    const model: MLModelDto = {
      model_id: modelId,
      algorithm: req?.algorithm ?? 'random_forest',
      status: 'completed',
      metrics: { accuracy: 0.72, f1: 0.68, precision: 0.74, recall: 0.65 },
      feature_names: req?.feature_columns ?? ['sma_20', 'rsi_14', 'volume_lag_1'],
      created_at: new Date().toISOString(),
      data_path: req?.data_path ?? 'data/sample.csv',
      target_column: req?.target_column ?? 'signal',
    };
    mockModels.push(model);
    return { model_id: modelId, metrics: model.metrics, feature_names: model.feature_names, status: 'completed' } as any;
  }
  if (cmd === 'delete_ml_model') {
    const id = args?.model_id as string;
    mockModels = mockModels.filter(m => m.model_id !== id);
    return true as any;
  }
  if (cmd === 'get_system_health') {
    return {
      adapters: [
        { adapter_id: 'yahoo_finance', adapter_type: 'market_data', status: 'healthy', message: 'Connected', last_check: new Date().toISOString() },
        { adapter_id: 'paper_trading', adapter_type: 'execution', status: 'healthy', message: 'Active', last_check: new Date().toISOString() },
      ] as AdapterHealthDto[],
      uptime_secs: Math.floor((Date.now() - performance.timeOrigin) / 1000),
      memory_usage_mb: 128,
      active_subscriptions: 5,
      open_orders: mockOrders.length,
      active_strategies: 0,
    } as any;
  }

  console.log('[Mock IPC]', cmd, args);
  return {} as T;
}

export async function getQuote(s: string): Promise<QuoteDto> { return invoke<QuoteDto>('get_quote', { symbol: s }); }
export async function subscribeSymbols(s: string[]): Promise<void> { return invoke<void>('subscribe_symbols', { symbols: s }); }
export async function placeOrder(r: NewOrderRequestDto): Promise<string> { return invoke<string>('place_order', { request: r }); }
export async function cancelOrder(o: string, b: string): Promise<void> { return invoke<void>('cancel_order', { order_id: o, broker_id: b }); }
export async function getPositions(): Promise<PositionDto[]> { return invoke<PositionDto[]>('get_positions'); }
export async function getOpenOrders(): Promise<OrderDto[]> { return invoke<OrderDto[]>('get_open_orders'); }
export async function getRiskStatus(): Promise<RiskStatusDto> { return invoke<RiskStatusDto>('get_risk_status'); }
export async function resetHalt(): Promise<void> { return invoke<void>('reset_halt'); }
export async function addAlert(i: string, r: string): Promise<void> { return invoke<void>('add_alert', { id: i, rule_json: r }); }
export async function removeAlert(r: string): Promise<boolean> { return invoke<boolean>('remove_alert', { rule_id: r }); }

// ML Workbench
export async function listMLModels(): Promise<MLModelDto[]> { return invoke<MLModelDto[]>('list_ml_models'); }
export async function trainMLModel(r: MLTrainingRequestDto): Promise<MLTrainingResultDto> { return invoke<MLTrainingResultDto>('train_ml_model', { request: r }); }
export async function deleteMLModel(id: string): Promise<boolean> { return invoke<boolean>('delete_ml_model', { model_id: id }); }

// Health Monitor
export async function getSystemHealth(): Promise<SystemHealthDto> { return invoke<SystemHealthDto>('get_system_health'); }
