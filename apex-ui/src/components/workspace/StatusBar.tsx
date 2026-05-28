import React, { useState, useEffect } from 'react';
import { useRiskStore } from '../../stores/riskStore';
import { PnlValue } from '../common/PnlValue';
import { formatPrice } from '../../lib/format';
import { useBrokerStore } from '../../stores/brokerStore';

export const StatusBar: React.FC = () => {
  const riskStatus = useRiskStore((s) => s.status);
  const [time, setTime] = useState(() => new Date().toLocaleTimeString());
  const activeBrokerId = useBrokerStore((s) => s.activeBrokerId);
  const brokerConnections = useBrokerStore((s) => s.connections);
  const activeBroker = React.useMemo(
    () => brokerConnections.find((broker) => broker.broker_id === activeBrokerId),
    [activeBrokerId, brokerConnections],
  );

  const brokerLabel = activeBroker?.mode === 'paper'
    ? 'Paper'
    : activeBroker?.display_name ?? 'Paper';

  const brokerDotClass = activeBroker?.status === 'connected' || activeBroker?.status === 'ready'
    ? 'bg-bull'
    : activeBroker?.status === 'auth_required' || activeBroker?.status === 'not_configured'
      ? 'bg-warning'
      : 'bg-bear';

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
        <span className={`inline-block h-2 w-2 rounded-full ${brokerDotClass}`} />
        <span>{brokerLabel}</span>
        <span>•</span>
        <span>{time}</span>
      </div>
    </div>
  );
};
