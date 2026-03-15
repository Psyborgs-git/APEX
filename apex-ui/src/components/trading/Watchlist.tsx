import React from 'react';
import { useMarketStore } from '../../stores/marketStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice, formatVolume } from '../../lib/format';

const WatchlistRow: React.FC<{ symbol: string }> = React.memo(({ symbol }) => {
  const quote = useMarketStore((s) => s.quotes.get(symbol));

  if (!quote) {
    return (
      <tr className="border-b border-[var(--border-color)]">
        <td className="px-3 py-1.5 font-mono text-sm font-medium">{symbol}</td>
        <td className="px-3 py-1.5 text-text-muted font-mono text-sm" colSpan={3}>—</td>
      </tr>
    );
  }

  return (
    <tr className="border-b border-[var(--border-color)] hover:bg-surface-2 cursor-pointer transition-colors duration-100">
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

export const Watchlist: React.FC = () => {
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
              <WatchlistRow key={symbol} symbol={symbol} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
