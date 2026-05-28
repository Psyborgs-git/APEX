import React, { useEffect, useCallback } from 'react';
import { useHealthStore } from '../../stores/healthStore';
import { getRiskStatus, getSystemHealth, resetHalt } from '../../lib/tauri';
import { useRiskStore } from '../../stores/riskStore';
import { PnlValue } from '../common/PnlValue';

const HEALTH_POLL_MS = 5000;

const STATUS_COLORS: Record<string, string> = {
  healthy: 'text-bull',
  degraded: 'text-warning',
  unhealthy: 'text-bear',
};

const STATUS_DOTS: Record<string, string> = {
  healthy: 'bg-bull',
  degraded: 'bg-warning',
  unhealthy: 'bg-bear',
};

export const HealthMonitor: React.FC = React.memo(() => {
  const health = useHealthStore((s) => s.health);
  const setHealth = useHealthStore((s) => s.setHealth);
  const riskStatus = useRiskStore((s) => s.status);
  const setRiskStatus = useRiskStore((s) => s.setStatus);

  const poll = useCallback(async () => {
    try {
      const [h, risk] = await Promise.all([
        getSystemHealth(),
        getRiskStatus().catch(() => null),
      ]);
      if (h) setHealth(h);
      if (risk) setRiskStatus(risk);
    } catch {
      /* backend not available */
    }
  }, [setHealth, setRiskStatus]);

  const handleResetHalt = useCallback(async () => {
    try {
      await resetHalt();
      const refreshed = await getRiskStatus();
      setRiskStatus(refreshed);
    } catch {
      /* backend not available */
    }
  }, [setRiskStatus]);

  useEffect(() => {
    poll();
    const id = setInterval(poll, HEALTH_POLL_MS);
    return () => clearInterval(id);
  }, [poll]);

  const formatUptime = (secs: number): string => {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${h}h ${m}m ${s}s`;
  };

  return (
    <div className="h-full flex flex-col" data-testid="health-monitor">
      <div className="px-3 py-2 border-b border-[var(--border-color)] bg-surface-1">
        <h3 className="text-sm font-medium text-text-primary">System Health</h3>
      </div>

      <div className="flex-1 overflow-auto p-3 space-y-4">
        {/* System overview */}
        <div className="grid grid-cols-2 gap-3" data-testid="health-overview">
          <MetricCard
            label="Uptime"
            value={health ? formatUptime(health.uptime_secs) : '--'}
            testId="health-uptime"
          />
          <MetricCard
            label="Memory"
            value={health ? `${health.memory_usage_mb} MB` : '--'}
            testId="health-memory"
          />
          <MetricCard
            label="Subscriptions"
            value={health ? String(health.active_subscriptions) : '--'}
            testId="health-subscriptions"
          />
          <MetricCard
            label="Open Orders"
            value={health ? String(health.open_orders) : '--'}
            testId="health-open-orders"
          />
          <MetricCard
            label="Active Strategies"
            value={health ? String(health.active_strategies) : '--'}
            testId="health-active-strategies"
          />
        </div>

        <div className="rounded border border-[var(--border-color)] bg-surface-0 p-3" data-testid="health-risk-panel">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h4 className="text-xs font-medium text-text-secondary">Risk Guardrails</h4>
              <p className="mt-1 text-xs text-text-muted">
                Monitor the live daily-loss circuit breaker and reset it explicitly when you are ready.
              </p>
            </div>
            <button
              type="button"
              onClick={() => void handleResetHalt()}
              disabled={!riskStatus.is_halted}
              className="rounded border border-[var(--border-color)] bg-surface-1 px-3 py-2 text-xs hover:bg-surface-2 disabled:opacity-50"
              data-testid="health-reset-halt"
            >
              Reset Halt
            </button>
          </div>

          <div className="mt-3 grid grid-cols-3 gap-3">
            <div className="rounded border border-[var(--border-color)] bg-surface-1 p-3">
              <div className="text-xs text-text-muted">Session P&L</div>
              <div className="mt-1 text-sm font-mono text-text-primary">
                <PnlValue value={riskStatus.session_pnl} className="text-sm" />
              </div>
            </div>

            <MetricCard
              label="Max Daily Loss"
              value={new Intl.NumberFormat('en-IN', {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              }).format(riskStatus.max_daily_loss)}
              testId="health-risk-max-loss"
            />

            <div className="rounded border border-[var(--border-color)] bg-surface-1 p-3">
              <div className="text-xs text-text-muted">Trading State</div>
              <div className={`mt-1 text-sm font-medium ${riskStatus.is_halted ? 'text-bear' : 'text-bull'}`}>
                {riskStatus.is_halted ? 'Halted' : 'Active'}
              </div>
            </div>
          </div>
        </div>

        {/* Adapter list */}
        <div>
          <h4 className="text-xs font-medium text-text-secondary mb-2">Adapter Status</h4>
          <div className="space-y-1" data-testid="health-adapters">
            {health && health.adapters.length > 0 ? (
              health.adapters.map((adapter) => (
                <div
                  key={adapter.adapter_id}
                  className="flex items-center justify-between p-2 bg-surface-0 border border-[var(--border-color)] rounded"
                  data-testid={`adapter-${adapter.adapter_id}`}
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`w-2 h-2 rounded-full ${STATUS_DOTS[adapter.status] ?? 'bg-text-muted'}`}
                    />
                    <span className="text-xs font-mono text-text-primary">
                      {adapter.adapter_id}
                    </span>
                    <span className="text-xs text-text-muted">
                      ({adapter.adapter_type})
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className={`text-xs ${STATUS_COLORS[adapter.status] ?? 'text-text-muted'}`}>
                      {adapter.status}
                    </span>
                    <span className="text-xs text-text-muted">{adapter.message}</span>
                  </div>
                </div>
              ))
            ) : (
              <p className="text-xs text-text-muted" data-testid="health-no-adapters">
                No adapter data available.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
});

HealthMonitor.displayName = 'HealthMonitor';

/* ---------- Metric card ---------- */

interface MetricCardProps {
  label: string;
  value: string;
  testId: string;
}

const MetricCard: React.FC<MetricCardProps> = ({ label, value, testId }) => (
  <div
    className="p-2 bg-surface-0 border border-[var(--border-color)] rounded"
    data-testid={testId}
  >
    <div className="text-xs text-text-muted">{label}</div>
    <div className="text-sm font-mono text-text-primary">{value}</div>
  </div>
);
