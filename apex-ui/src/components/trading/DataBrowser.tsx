import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { getHistoricalData } from '../../lib/tauri';
import type { OHLCVDto } from '../../lib/types';

const TIMEFRAMES = ['1m', '5m', '15m', '1h', '4h', '1d', '1w'] as const;

interface DataBrowserProps {
  defaultSymbol?: string;
}

const formatNumber = (value: number) => new Intl.NumberFormat('en-IN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
}).format(value);

export const DataBrowser: React.FC<DataBrowserProps> = ({ defaultSymbol }) => {
  const [symbol, setSymbol] = useState(defaultSymbol ?? 'RELIANCE.NS');
  const [timeframe, setTimeframe] = useState<(typeof TIMEFRAMES)[number]>('1d');
  const [limit, setLimit] = useState(100);
  const [rows, setRows] = useState<OHLCVDto[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSymbol) {
      setSymbol(defaultSymbol);
    }
  }, [defaultSymbol]);

  const loadRows = useCallback(async () => {
    const cleanedSymbol = symbol.trim().toUpperCase();
    if (!cleanedSymbol) {
      setError('Enter a symbol to browse stored OHLCV data.');
      return;
    }

    setLoading(true);
    try {
      const bars = await getHistoricalData(cleanedSymbol, timeframe, limit);
      setRows(Array.isArray(bars) ? bars : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unable to query stored data');
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, [limit, symbol, timeframe]);

  useEffect(() => {
    void loadRows();
  }, [loadRows]);

  const stats = useMemo(() => {
    if (rows.length === 0) {
      return null;
    }

    const first = rows[0];
    const last = rows[rows.length - 1];
    const totalVolume = rows.reduce((sum, row) => sum + row.volume, 0);
    const high = Math.max(...rows.map((row) => row.high));
    const low = Math.min(...rows.map((row) => row.low));

    return {
      range: `${new Date(first.time).toLocaleDateString()} → ${new Date(last.time).toLocaleDateString()}`,
      lastClose: last.close,
      high,
      low,
      totalVolume,
    };
  }, [rows]);

  return (
    <div className="h-full flex flex-col" data-testid="data-browser">
      <div className="px-3 py-2 border-b border-[var(--border-color)] bg-surface-1 flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-xs text-text-muted">
          <span>Symbol</span>
          <input
            type="text"
            value={symbol}
            onChange={(event) => setSymbol(event.target.value)}
            className="min-w-[180px] rounded border border-[var(--border-color)] bg-surface-0 px-3 py-2 text-sm text-text-primary"
            placeholder="RELIANCE.NS"
            data-testid="data-browser-symbol"
          />
        </label>

        <label className="flex flex-col gap-1 text-xs text-text-muted">
          <span>Timeframe</span>
          <select
            value={timeframe}
            onChange={(event) => setTimeframe(event.target.value as (typeof TIMEFRAMES)[number])}
            className="rounded border border-[var(--border-color)] bg-surface-0 px-3 py-2 text-sm text-text-primary"
            data-testid="data-browser-timeframe"
          >
            {TIMEFRAMES.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1 text-xs text-text-muted">
          <span>Rows</span>
          <input
            type="number"
            min={10}
            max={1000}
            value={limit}
            onChange={(event) => setLimit(Math.max(10, Math.min(1000, Number(event.target.value) || 100)))}
            className="w-28 rounded border border-[var(--border-color)] bg-surface-0 px-3 py-2 text-sm text-text-primary"
            data-testid="data-browser-limit"
          />
        </label>

        <button
          type="button"
          onClick={() => void loadRows()}
          disabled={loading}
          className="rounded bg-primary-500 px-3 py-2 text-xs text-white hover:bg-primary-600 disabled:opacity-50"
          data-testid="data-browser-load"
        >
          {loading ? 'Loading…' : 'Load Stored Data'}
        </button>
      </div>

      <div className="flex-1 overflow-hidden p-3 space-y-3">
        {stats && (
          <div className="grid grid-cols-4 gap-3" data-testid="data-browser-stats">
            <MetricCard label="Rows" value={String(rows.length)} />
            <MetricCard label="Latest Close" value={formatNumber(stats.lastClose)} />
            <MetricCard label="Range High / Low" value={`${formatNumber(stats.high)} / ${formatNumber(stats.low)}`} />
            <MetricCard label="Total Volume" value={new Intl.NumberFormat('en-IN').format(stats.totalVolume)} />
          </div>
        )}

        <div className="rounded border border-[var(--border-color)] bg-surface-0 p-3 text-xs text-text-muted">
          {error ? (
            <span className="text-bear">{error}</span>
          ) : rows.length > 0 ? (
            <span>
              Showing <strong className="text-text-primary">{rows.length}</strong> stored bars for{' '}
              <strong className="text-text-primary">{symbol.toUpperCase()}</strong>
              {stats ? ` (${stats.range})` : ''}.
            </span>
          ) : (
            <span>No stored OHLCV rows returned for the current query.</span>
          )}
        </div>

        <div className="flex-1 overflow-auto rounded border border-[var(--border-color)] bg-surface-0">
          <table className="w-full min-w-[720px]">
            <thead>
              <tr className="border-b border-[var(--border-color)] text-left text-xs text-text-muted">
                <th className="px-3 py-2 font-normal">Time</th>
                <th className="px-3 py-2 text-right font-normal">Open</th>
                <th className="px-3 py-2 text-right font-normal">High</th>
                <th className="px-3 py-2 text-right font-normal">Low</th>
                <th className="px-3 py-2 text-right font-normal">Close</th>
                <th className="px-3 py-2 text-right font-normal">Volume</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={`${row.time}-${row.close}`} className="border-b border-[var(--border-color)] text-sm hover:bg-surface-1">
                  <td className="px-3 py-2 font-mono text-text-primary">{new Date(row.time).toLocaleString()}</td>
                  <td className="px-3 py-2 text-right font-mono">{formatNumber(row.open)}</td>
                  <td className="px-3 py-2 text-right font-mono text-bull">{formatNumber(row.high)}</td>
                  <td className="px-3 py-2 text-right font-mono text-bear">{formatNumber(row.low)}</td>
                  <td className="px-3 py-2 text-right font-mono text-text-primary">{formatNumber(row.close)}</td>
                  <td className="px-3 py-2 text-right font-mono">{new Intl.NumberFormat('en-IN').format(row.volume)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};

interface MetricCardProps {
  label: string;
  value: string;
}

const MetricCard: React.FC<MetricCardProps> = ({ label, value }) => (
  <div className="rounded border border-[var(--border-color)] bg-surface-0 p-3">
    <div className="text-xs text-text-muted">{label}</div>
    <div className="mt-1 text-sm font-mono text-text-primary">{value}</div>
  </div>
);
