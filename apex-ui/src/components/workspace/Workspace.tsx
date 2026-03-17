import React, { useState } from 'react';
import { Watchlist } from '../trading/Watchlist';
import { OrderEntry } from '../trading/OrderEntry';
import { PositionsPanel } from '../trading/PositionsPanel';
import { CandleChart } from '../charts/CandleChart';
import { useMarketStore } from '../../stores/marketStore';

export const Workspace: React.FC = () => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const [selectedSymbol, setSelectedSymbol] = useState(watchlist[0] ?? 'RELIANCE.NS');

  return (
    <div className="grid grid-cols-12 gap-1 h-full p-1 bg-surface-0">
      {/* Left: Watchlist */}
      <div className="col-span-3 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
        <Watchlist onSelectSymbol={setSelectedSymbol} selectedSymbol={selectedSymbol} />
      </div>

      {/* Center: Chart + Order Entry */}
      <div className="col-span-6 flex flex-col gap-1">
        <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
          <CandleChart symbol={selectedSymbol} />
        </div>
        <div className="h-48 bg-surface-1 rounded-lg border border-[var(--border-color)]">
          <OrderEntry defaultSymbol={selectedSymbol} />
        </div>
      </div>

      {/* Right: Positions & Orders */}
      <div className="col-span-3 flex flex-col gap-1">
        <div className="flex-1 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
          <PositionsPanel />
        </div>
        <div className="h-40 bg-surface-1 rounded-lg border border-[var(--border-color)] p-3">
          <AlertConsole />
        </div>
      </div>
    </div>
  );
};

const AlertConsole: React.FC = () => {
  return (
    <div className="h-full flex flex-col">
      <span className="text-sm font-medium text-text-secondary mb-2">Alerts</span>
      <div className="flex-1 overflow-y-auto text-xs font-mono text-text-muted">
        <p>No alerts triggered.</p>
      </div>
    </div>
  );
};
