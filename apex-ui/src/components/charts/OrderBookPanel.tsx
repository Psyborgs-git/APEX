import React, { useCallback, useEffect, useState } from 'react';
import { getOrderBook } from '../../lib/tauri';
import type { OrderBookDto } from '../../lib/types';
import { useMarketStore } from '../../stores/marketStore';
import { OrderBookHeatmap } from './OrderBookHeatmap';
import { formatPrice } from '../../lib/format';

const POLL_MS = 2_000;

export const OrderBookPanel: React.FC<{ defaultSymbol?: string }> = ({ defaultSymbol }) => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const [symbol, setSymbol] = useState(defaultSymbol ?? watchlist[0] ?? 'RELIANCE.NS');
  const [book, setBook] = useState<OrderBookDto | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSymbol) setSymbol(defaultSymbol);
  }, [defaultSymbol]);

  const refresh = useCallback(async () => {
    try {
      setBook(await getOrderBook(symbol));
      setError(null);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'No order book available');
      setBook(null);
    }
  }, [symbol]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const spread = book && book.bids.length > 0 && book.asks.length > 0
    ? book.asks[0].price - book.bids[0].price
    : null;

  return (
    <div className="flex flex-col h-full" data-testid="order-book-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center gap-2">
        <span className="text-sm font-medium text-text-secondary">Order Book</span>
        <select
          value={symbol}
          onChange={(e) => setSymbol(e.target.value)}
          className="px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded font-mono"
          data-testid="book-symbol-select"
        >
          {watchlist.map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
          {!watchlist.includes(symbol) && <option value={symbol}>{symbol}</option>}
        </select>
        <span className="ml-auto flex items-center gap-2 text-[10px] font-mono text-text-muted">
          {book && (
            <>
              <span>{book.source === 'binance' ? 'LIVE L2 · BINANCE' : 'SYNTHETIC'}</span>
              {spread !== null && <span>spread {formatPrice(spread)}</span>}
            </>
          )}
        </span>
      </div>
      <div className="flex-1 min-h-0">
        {error ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm px-4 text-center">
            {error}
          </div>
        ) : book ? (
          <OrderBookHeatmap bids={book.bids} asks={book.asks} spread={spread ?? undefined} />
        ) : (
          <div className="flex items-center justify-center h-full text-text-muted text-sm">
            Loading depth…
          </div>
        )}
      </div>
    </div>
  );
};
