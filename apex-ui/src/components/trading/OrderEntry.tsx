import React, { useState, useCallback, useMemo } from 'react';
import { formatPrice } from '../../lib/format';

export const OrderEntry: React.FC = () => {
  const [symbol, setSymbol] = useState('');
  const [side, setSide] = useState<'BUY' | 'SELL'>('BUY');
  const [orderType, setOrderType] = useState('MARKET');
  const [quantity, setQuantity] = useState('');
  const [price, setPrice] = useState('');

  const estimatedValue = useMemo(() => {
    const qty = parseFloat(quantity) || 0;
    const prc = parseFloat(price) || 0;
    return qty * prc;
  }, [quantity, price]);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    console.log('Submit order:', { symbol, side, orderType, quantity, price });
  }, [symbol, side, orderType, quantity, price]);

  return (
    <form onSubmit={handleSubmit} className="p-3 h-full flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Order Entry</span>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={() => setSide('BUY')}
            className={`px-3 py-1 text-xs font-mono rounded ${
              side === 'BUY' ? 'bg-bull text-black font-bold' : 'bg-surface-2 text-text-muted'
            }`}
          >
            BUY
          </button>
          <button
            type="button"
            onClick={() => setSide('SELL')}
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
          className="col-span-2 bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
          tabIndex={1}
        />
        <select
          value={orderType}
          onChange={(e) => setOrderType(e.target.value)}
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
            className="bg-surface-2 text-text-primary font-mono text-sm px-2 py-1 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
            tabIndex={4}
            step="0.05"
          />
          <div className="flex items-center text-xs text-text-muted font-mono">
            Est: {estimatedValue > 0 ? formatPrice(estimatedValue) : '—'}
          </div>
        </div>
      )}

      <button
        type="submit"
        className={`mt-auto py-2 rounded font-mono text-sm font-bold ${
          side === 'BUY'
            ? 'bg-bull text-black hover:brightness-110'
            : 'bg-bear text-white hover:brightness-110'
        } transition-all`}
        tabIndex={5}
      >
        {side} {symbol || '...'}
      </button>
    </form>
  );
};
