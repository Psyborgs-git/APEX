import React from 'react';
import { Watchlist } from '../trading/Watchlist';
import { OrderEntry } from '../trading/OrderEntry';
import { PositionsPanel } from '../trading/PositionsPanel';

export const Workspace: React.FC = () => {
  return (
    <div className="grid grid-cols-12 gap-1 h-full p-1 bg-surface-0">
      {/* Left: Watchlist */}
      <div className="col-span-3 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
        <Watchlist />
      </div>

      {/* Center: Chart placeholder + Order Entry */}
      <div className="col-span-6 flex flex-col gap-1">
        <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] flex items-center justify-center">
          <span className="text-text-muted font-mono text-sm">Chart — Candlestick (lightweight-charts)</span>
        </div>
        <div className="h-48 bg-surface-1 rounded-lg border border-[var(--border-color)]">
          <OrderEntry />
        </div>
      </div>

      {/* Right: Positions & Orders */}
      <div className="col-span-3 flex flex-col gap-1">
        <div className="flex-1 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
          <PositionsPanel />
        </div>
        <div className="h-40 bg-surface-1 rounded-lg border border-[var(--border-color)] flex items-center justify-center">
          <span className="text-text-muted font-mono text-sm">Alert Console</span>
        </div>
      </div>
    </div>
  );
};
