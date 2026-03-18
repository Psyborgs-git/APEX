import React, { useState, useCallback, useMemo } from 'react';
import { formatPrice } from '../../lib/format';
import { placeOrder } from '../../lib/tauri';

interface OrderEntryProps {
  defaultSymbol?: string;
}

export const OrderEntry: React.FC<OrderEntryProps> = ({ defaultSymbol }) => {
  const [symbol, setSymbol] = useState(defaultSymbol ?? '');
  const [side, setSide] = useState<'BUY' | 'SELL'>('BUY');
  const [orderType, setOrderType] = useState('MARKET');
  const [quantity, setQuantity] = useState('');
  const [price, setPrice] = useState('');
  const [status, setStatus] = useState<{ type: 'idle' | 'submitting' | 'success' | 'error'; message?: string }>({ type: 'idle' });

  React.useEffect(() => {
    if (defaultSymbol) setSymbol(defaultSymbol);
  }, [defaultSymbol]);

  const estimatedValue = useMemo(() => {
    const qty = parseFloat(quantity) || 0;
    const prc = parseFloat(price) || 0;
    return qty * prc;
  }, [quantity, price]);

  const handleSubmit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (!symbol || !quantity) return;

    setStatus({ type: 'submitting' });
    try {
      const orderId = await placeOrder({
        symbol,
        side: side.toLowerCase(),
        order_type: orderType.toLowerCase(),
        quantity: parseFloat(quantity),
        price: orderType !== 'MARKET' ? parseFloat(price) || null : null,
        stop_price: null,
        broker_id: 'paper',
        tag: null,
      });
      setStatus({ type: 'success', message: `Order placed: ${orderId}` });
      setQuantity('');
      setPrice('');
      setTimeout(() => setStatus({ type: 'idle' }), 3000);
    } catch (err) {
      setStatus({ type: 'error', message: String(err) });
      setTimeout(() => setStatus({ type: 'idle' }), 5000);
    }
  }, [symbol, side, orderType, quantity, price]);

  return (
    <form onSubmit={handleSubmit} className="p-3 h-full flex flex-col gap-2" data-testid="order-entry-panel">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Order Entry</span>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={() => setSide('BUY')}
            data-testid="order-side-buy"
            className={`px-3 py-1 text-xs font-mono rounded ${
              side === 'BUY' ? 'bg-bull text-black font-bold' : 'bg-surface-2 text-text-muted'
            }`}
          >
            BUY
          </button>
          <button
            type="button"
            onClick={() => setSide('SELL')}
            data-testid="order-side-sell"
            className={`px-3 py-1 text-xs font-mono rounded ${
              side === 'SELL' ? 'bg-bear text-white font-bold' : 'bg-surface-2 text-text-muted'
            }`}
          >
            SELL
          </button>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-2">
        <input
          type="text"
          value={symbol}
          onChange={(e) => setSymbol(e.target.value.toUpperCase())}
          placeholder="Symbol"
          data-testid="order-symbol-input"
          className="col-span-2 bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
          tabIndex={1}
        />
        <select
          value={orderType}
          onChange={(e) => setOrderType(e.target.value)}
          data-testid="order-type-select"
          className="bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
          tabIndex={2}
        >
          <option value="MARKET">Market</option>
          <option value="LIMIT">Limit</option>
          <option value="STOP">Stop</option>
          <option value="STOP_LIMIT">Stop Limit</option>
        </select>
        <input
          type="number"
          value={quantity}
          onChange={(e) => setQuantity(e.target.value)}
          placeholder="Qty"
          data-testid="order-quantity-input"
          className="bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
          tabIndex={3}
          step="1"
          min="0"
        />
      </div>

      {orderType !== 'MARKET' && (
        <div className="grid grid-cols-2 gap-2">
          <input
            type="number"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            placeholder="Price"
            data-testid="order-price-input"
            className="bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
            tabIndex={4}
            step="0.05"
          />
          <div className="flex items-center text-xs text-text-muted font-mono">
            Est: {estimatedValue > 0 ? formatPrice(estimatedValue) : '—'}
          </div>
        </div>
      )}

      {status.type === 'success' && (
        <div className="text-xs text-bull font-mono" data-testid="order-confirmation">{status.message}</div>
      )}
      {status.type === 'error' && (
        <div className="text-xs text-bear font-mono" data-testid="order-error-message">{status.message}</div>
      )}

      <button
        type="submit"
        disabled={status.type === 'submitting' || !symbol || !quantity}
        data-testid="order-submit-button"
        className={`mt-auto py-2 rounded font-mono text-sm font-bold ${
          side === 'BUY'
            ? 'bg-bull text-black hover:brightness-110'
            : 'bg-bear text-white hover:brightness-110'
        } transition-all disabled:opacity-50 disabled:cursor-not-allowed`}
        tabIndex={5}
      >
        {status.type === 'submitting' ? 'Placing...' : `${side} ${symbol || '...'}`}
      </button>
    </form>
  );
};
