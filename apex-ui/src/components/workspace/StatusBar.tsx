import React, { useState, useEffect } from 'react';
import { useRiskStore } from '../../stores/riskStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice } from '../../lib/format';

export const StatusBar: React.FC = () => {
  const riskStatus = useRiskStore((s) => s.status);
  const [time, setTime] = useState(() => new Date().toLocaleTimeString());

  useEffect(() => {
    const interval = setInterval(() => setTime(new Date().toLocaleTimeString()), 1000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="h-7 bg-surface-1 border-t border-[var(--border-color)] flex items-center px-4 justify-between text-xs">
      <div className="flex items-center gap-4">
        <span className="text-text-muted">Session P&L:</span>
        <PnlValue value={riskStatus.session_pnl} />
        <span className="text-text-muted">|</span>
        <span className="text-text-muted">Max Loss: {formatPrice(riskStatus.max_daily_loss)}</span>
        {riskStatus.is_halted && (
          <span className="text-bear font-bold animate-pulse">⚠ TRADING HALTED</span>
        )}
      </div>
      <div className="flex items-center gap-3 text-text-muted">
        <span>Paper</span>
        <span>•</span>
        <span>{time}</span>
      </div>
    </div>
  );
};
