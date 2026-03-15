import React from 'react';
import { useOrderStore } from '../../stores/orderStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice, formatQuantity } from '../../lib/format';

export const PositionsPanel: React.FC = () => {
  const positions = useOrderStore((s) => s.positions);

  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Positions</span>
        <span className="text-xs text-text-muted font-mono">{positions.length} open</span>
      </div>
      <div className="flex-1 overflow-auto">
        {positions.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm">
            No open positions
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr className="text-xs text-text-muted border-b border-[var(--border-color)]">
                <th className="px-3 py-1.5 text-left font-normal">Symbol</th>
                <th className="px-3 py-1.5 text-right font-normal">Qty</th>
                <th className="px-3 py-1.5 text-right font-normal">Avg</th>
                <th className="px-3 py-1.5 text-right font-normal">P&L</th>
              </tr>
            </thead>
            <tbody>
              {positions.map((pos) => (
                <tr key={pos.symbol} className="border-b border-[var(--border-color)] hover:bg-surface-2">
                  <td className="px-3 py-1.5 font-mono text-sm">
                    <span className={pos.side === 'Buy' ? 'text-bull' : 'text-bear'}>
                      {pos.side === 'Buy' ? '▲' : '▼'}
                    </span>{' '}
                    {pos.symbol}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-sm text-right" data-numeric>
                    {formatQuantity(pos.quantity)}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-sm text-right" data-numeric>
                    {formatPrice(pos.avg_price)}
                  </td>
                  <td className="px-3 py-1.5 text-right">
                    <PnlValue value={pos.pnl} className="text-sm" />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
