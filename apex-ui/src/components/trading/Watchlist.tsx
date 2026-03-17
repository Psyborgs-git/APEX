import React, { useState } from 'react';
import { useMarketStore } from '../../stores/marketStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice, formatVolume } from '../../lib/format';

interface WatchlistRowProps {
  symbol: string;
  selected: boolean;
  onSelect: (symbol: string) => void;
  onRemove: (symbol: string) => void;
}

const WatchlistRow: React.FC<WatchlistRowProps> = React.memo(({ symbol, selected, onSelect, onRemove }) => {
  const quote = useMarketStore((s) => s.quotes.get(symbol));

  if (!quote) {
    return (
      <tr
        className={`border-b border-[var(--border-color)] cursor-pointer hover:bg-surface-2 transition-colors duration-100 ${selected ? 'bg-surface-2' : ''}`}
        data-testid={`watchlist-item-${symbol}`}
      >
        <td className="px-3 py-1.5 font-mono text-sm font-medium" onClick={() => onSelect(symbol)}>{symbol}</td>
        <td className="px-3 py-1.5 text-text-muted font-mono text-sm" colSpan={2} onClick={() => onSelect(symbol)}>—</td>
        <td className="px-3 py-1.5 text-right">
          <button
            onClick={(e) => {
              e.stopPropagation();
              onRemove(symbol);
            }}
            className="text-red-500 hover:text-red-400 text-xs px-2 py-0.5"
            data-testid="watchlist-remove-button"
          >
            ✕
          </button>
        </td>
      </tr>
    );
  }

  return (
    <tr
      className={`border-b border-[var(--border-color)] hover:bg-surface-2 cursor-pointer transition-colors duration-100 ${selected ? 'bg-surface-2' : ''}`}
      data-testid={`watchlist-item-${symbol}`}
    >
      <td className="px-3 py-1.5 font-mono text-sm font-medium" onClick={() => onSelect(symbol)}>{symbol}</td>
      <td className="px-3 py-1.5 font-mono text-sm text-right" data-testid={`watchlist-price-${symbol}`} data-numeric onClick={() => onSelect(symbol)}>{formatPrice(quote.last)}</td>
      <td className="px-3 py-1.5 text-right" onClick={() => onSelect(symbol)}>
        <PnlValue value={quote.change_pct} type="percent" className="text-xs" />
      </td>
      <td className="px-3 py-1.5 text-right">
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove(symbol);
          }}
          className="text-red-500 hover:text-red-400 text-xs px-2 py-0.5"
          data-testid="watchlist-remove-button"
        >
          ✕
        </button>
      </td>
    </tr>
  );
});

WatchlistRow.displayName = 'WatchlistRow';

interface WatchlistProps {
  onSelectSymbol?: (symbol: string) => void;
  selectedSymbol?: string;
}

export const Watchlist: React.FC<WatchlistProps> = ({ onSelectSymbol, selectedSymbol }) => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const addToWatchlist = useMarketStore((s) => s.addToWatchlist);
  const removeFromWatchlist = useMarketStore((s) => s.removeFromWatchlist);
  const [isAdding, setIsAdding] = useState(false);
  const [newSymbol, setNewSymbol] = useState('');

  const handleAdd = () => {
    if (newSymbol.trim()) {
      addToWatchlist(newSymbol.trim().toUpperCase());
      setNewSymbol('');
      setIsAdding(false);
    }
  };

  return (
    <div className="flex flex-col h-full" data-testid="watchlist-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Watchlist</span>
        <div className="flex items-center gap-2">
          <span className="text-xs text-text-muted font-mono">{watchlist.length} symbols</span>
          <button
            onClick={() => setIsAdding(!isAdding)}
            className="text-primary-500 hover:text-primary-400 text-sm px-2 py-0.5 rounded border border-primary-500/50"
            data-testid="watchlist-add-button"
          >
            +
          </button>
        </div>
      </div>

      {isAdding && (
        <div className="px-3 py-2 border-b border-[var(--border-color)] flex gap-2">
          <input
            type="text"
            value={newSymbol}
            onChange={(e) => setNewSymbol(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
            placeholder="Symbol"
            className="flex-1 px-2 py-1 text-sm bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid="watchlist-symbol-input"
            autoFocus
          />
          <button
            onClick={handleAdd}
            className="px-3 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded"
            data-testid="watchlist-confirm-add"
          >
            Add
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto">
        <table className="w-full">
          <thead>
            <tr className="text-xs text-text-muted border-b border-[var(--border-color)]">
              <th className="px-3 py-1.5 text-left font-normal">Symbol</th>
              <th className="px-3 py-1.5 text-right font-normal">Last</th>
              <th className="px-3 py-1.5 text-right font-normal">Chg%</th>
              <th className="px-3 py-1.5 text-right font-normal w-12"></th>
            </tr>
          </thead>
          <tbody>
            {watchlist.map((symbol) => (
              <WatchlistRow
                key={symbol}
                symbol={symbol}
                selected={symbol === selectedSymbol}
                onSelect={onSelectSymbol ?? (() => {})}
                onRemove={removeFromWatchlist}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
