import React from 'react';
import { useMarketStore } from '../../stores/marketStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice, formatVolume } from '../../lib/format';

interface WatchlistRowProps {
  symbol: string;
  selected: boolean;
  onSelect: (symbol: string) => void;
}

const WatchlistRow: React.FC<WatchlistRowProps> = React.memo(({ symbol, selected, onSelect }) => {
  const quote = useMarketStore((s) => s.quotes.get(symbol));

  if (!quote) {
    return (
      <tr
        className={`border-b border-[var(--border-color)] cursor-pointer hover:bg-surface-2 transition-colors duration-100 ${selected ? 'bg-surface-2' : ''}`}
        onClick={() => onSelect(symbol)}
      >
        <td className="px-3 py-1.5 font-mono text-sm font-medium">{symbol}</td>
        <td className="px-3 py-1.5 text-text-muted font-mono text-sm" colSpan={3}>—</td>
      </tr>
    );
  }

  return (
    <tr
      className={`border-b border-[var(--border-color)] hover:bg-surface-2 cursor-pointer transition-colors duration-100 ${selected ? 'bg-surface-2' : ''}`}
      onClick={() => onSelect(symbol)}
    >
      <td className="px-3 py-1.5 font-mono text-sm font-medium">{symbol}</td>
      <td className="px-3 py-1.5 font-mono text-sm text-right" data-numeric>{formatPrice(quote.last)}</td>
      <td className="px-3 py-1.5 text-right">
        <PnlValue value={quote.change_pct} type="percent" className="text-xs" />
      </td>
      <td className="px-3 py-1.5 font-mono text-xs text-text-muted text-right" data-numeric>
        {formatVolume(quote.volume)}
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

  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Watchlist</span>
        <span className="text-xs text-text-muted font-mono">{watchlist.length} symbols</span>
      </div>
      <div className="flex-1 overflow-auto">
        <table className="w-full">
          <thead>
            <tr className="text-xs text-text-muted border-b border-[var(--border-color)]">
              <th className="px-3 py-1.5 text-left font-normal">Symbol</th>
              <th className="px-3 py-1.5 text-right font-normal">Last</th>
              <th className="px-3 py-1.5 text-right font-normal">Chg%</th>
              <th className="px-3 py-1.5 text-right font-normal">Vol</th>
            </tr>
          </thead>
          <tbody>
            {watchlist.map((symbol) => (
              <WatchlistRow
                key={symbol}
                symbol={symbol}
                selected={symbol === selectedSymbol}
                onSelect={onSelectSymbol ?? (() => {})}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
