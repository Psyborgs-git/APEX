import React, { useEffect, useCallback } from 'react';
import { useHealthStore } from '../../stores/healthStore';
import { getSystemHealth } from '../../lib/tauri';

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

  const poll = useCallback(async () => {
    try {
      const h = await getSystemHealth();
      if (h) setHealth(h);
    } catch {
      /* backend not available */
    }
  }, [setHealth]);

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
