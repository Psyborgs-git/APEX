import React, { useCallback, useState } from 'react';
import { runScan } from '../../lib/tauri';
import type { ScanCriterionDto, ScanOutputDto } from '../../lib/types';
import { useMarketStore } from '../../stores/marketStore';
import { formatPrice, formatPct, formatVolume } from '../../lib/format';
import { useWorkspaceStore } from '../../stores/workspaceStore';

const CRITERIA: { kind: string; label: string; fields: ('value' | 'value2' | 'period')[] }[] = [
  { kind: 'price_above', label: 'Price above', fields: ['value'] },
  { kind: 'price_below', label: 'Price below', fields: ['value'] },
  { kind: 'price_between', label: 'Price between', fields: ['value', 'value2'] },
  { kind: 'volume_above', label: 'Volume above', fields: ['value'] },
  { kind: 'change_pct_above', label: '% change above', fields: ['value'] },
  { kind: 'change_pct_below', label: '% change below', fields: ['value'] },
  { kind: 'rsi_above', label: 'RSI above', fields: ['value', 'period'] },
  { kind: 'rsi_below', label: 'RSI below', fields: ['value', 'period'] },
  { kind: 'above_sma', label: 'Above SMA', fields: ['period'] },
  { kind: 'below_sma', label: 'Below SMA', fields: ['period'] },
];

interface CriterionRow extends ScanCriterionDto {
  rowId: number;
}

export const ScannerPanel: React.FC = () => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const setCommandSymbol = useWorkspaceStore((s) => s.setCommandSymbol);
  const setCommandTab = useWorkspaceStore((s) => s.setCommandTab);

  const [rows, setRows] = useState<CriterionRow[]>([
    { rowId: 1, kind: 'change_pct_above', value: 1 },
  ]);
  const [customUniverse, setCustomUniverse] = useState('');
  const [timeframe, setTimeframe] = useState('d1');
  const [result, setResult] = useState<ScanOutputDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nextRowId = React.useRef(2);

  const addRow = () => {
    setRows((current) => [...current, { rowId: nextRowId.current++, kind: 'price_above', value: 0 }]);
  };

  const removeRow = (rowId: number) => {
    setRows((current) => current.filter((r) => r.rowId !== rowId));
  };

  const updateRow = (rowId: number, patch: Partial<CriterionRow>) => {
    setRows((current) => current.map((r) => (r.rowId === rowId ? { ...r, ...patch } : r)));
  };

  const run = useCallback(async () => {
    const criteria: ScanCriterionDto[] = rows
      .filter((r) => r.kind)
      .map((r) => ({
        kind: r.kind,
        value: r.value,
        value2: r.value2,
        period: r.period,
      }));
    if (criteria.length === 0) {
      setError('Add at least one criterion');
      return;
    }

    const symbols = customUniverse
      .split(/[\s,]+/)
      .map((s) => s.trim().toUpperCase())
      .filter(Boolean);

    setBusy(true);
    try {
      setResult(
        await runScan({
          symbols: symbols.length > 0 ? symbols : [],
          criteria,
          timeframe,
        }),
      );
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Scan failed');
    } finally {
      setBusy(false);
    }
  }, [customUniverse, rows, timeframe]);

  const openChart = (symbol: string) => {
    setCommandSymbol(symbol);
    setCommandTab('chart');
  };

  return (
    <div className="flex flex-col h-full" data-testid="scanner-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] space-y-2">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-text-secondary">Scanner</span>
          <select
            value={timeframe}
            onChange={(e) => setTimeframe(e.target.value)}
            className="px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid="scanner-timeframe"
          >
            <option value="d1">Daily</option>
            <option value="h1">Hourly</option>
            <option value="m15">15m</option>
            <option value="w1">Weekly</option>
          </select>
          <input
            type="text"
            value={customUniverse}
            onChange={(e) => setCustomUniverse(e.target.value)}
            placeholder={`Universe (blank = watchlist: ${watchlist.length} symbols)`}
            className="flex-1 min-w-0 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded font-mono"
            data-testid="scanner-universe"
          />
        </div>

        <div className="space-y-1">
          {rows.map((row) => {
            const spec = CRITERIA.find((c) => c.kind === row.kind) ?? CRITERIA[0];
            return (
              <div key={row.rowId} className="flex items-center gap-2" data-testid="scanner-criterion">
                <select
                  value={row.kind}
                  onChange={(e) => updateRow(row.rowId, { kind: e.target.value })}
                  className="w-40 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
                >
                  {CRITERIA.map((c) => (
                    <option key={c.kind} value={c.kind}>{c.label}</option>
                  ))}
                </select>
                {spec.fields.includes('value') && (
                  <input
                    type="number"
                    value={row.value ?? ''}
                    onChange={(e) => updateRow(row.rowId, { value: e.target.value === '' ? undefined : Number(e.target.value) })}
                    placeholder="value"
                    className="w-24 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded font-mono"
                    data-testid="scanner-value"
                  />
                )}
                {spec.fields.includes('value2') && (
                  <input
                    type="number"
                    value={row.value2 ?? ''}
                    onChange={(e) => updateRow(row.rowId, { value2: e.target.value === '' ? undefined : Number(e.target.value) })}
                    placeholder="to"
                    className="w-24 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded font-mono"
                  />
                )}
                {spec.fields.includes('period') && (
                  <input
                    type="number"
                    value={row.period ?? ''}
                    onChange={(e) => updateRow(row.rowId, { period: e.target.value === '' ? undefined : Number(e.target.value) })}
                    placeholder="period (14)"
                    className="w-24 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded font-mono"
                  />
                )}
                <button
                  onClick={() => removeRow(row.rowId)}
                  className="text-xs text-bear hover:brightness-125"
                  data-testid="scanner-remove-criterion"
                >
                  ✕
                </button>
              </div>
            );
          })}
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={addRow}
            className="px-2 py-0.5 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
            data-testid="scanner-add-criterion"
          >
            + Criterion
          </button>
          <button
            onClick={() => void run()}
            disabled={busy}
            className="px-3 py-0.5 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded disabled:opacity-50"
            data-testid="scanner-run"
          >
            {busy ? 'Scanning…' : 'Run Scan'}
          </button>
          {error && <span className="text-xs text-bear">{error}</span>}
          {result && (
            <span className="ml-auto text-xs text-text-muted font-mono">
              {result.matched_count}/{result.scanned_count} matched
            </span>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-auto">
        {!result ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm">
            Configure criteria and run a scan
          </div>
        ) : result.results.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm">
            No matches — {result.scanned_count} symbols scanned
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr className="text-xs text-text-muted border-b border-[var(--border-color)]">
                <th className="px-3 py-1.5 text-left font-normal">Symbol</th>
                <th className="px-3 py-1.5 text-right font-normal">Last</th>
                <th className="px-3 py-1.5 text-right font-normal">% Chg</th>
                <th className="px-3 py-1.5 text-right font-normal">Volume</th>
              </tr>
            </thead>
            <tbody>
              {result.results.map((r) => (
                <tr
                  key={r.symbol}
                  className="border-b border-[var(--border-color)] hover:bg-surface-2 cursor-pointer"
                  onClick={() => openChart(r.symbol)}
                  data-testid="scanner-result-row"
                >
                  <td className="px-3 py-1.5 font-mono text-sm text-accent">{r.symbol}</td>
                  <td className="px-3 py-1.5 font-mono text-sm text-right" data-numeric>
                    {formatPrice(r.last_price)}
                  </td>
                  <td className={`px-3 py-1.5 font-mono text-sm text-right ${r.change_pct >= 0 ? 'text-bull' : 'text-bear'}`} data-numeric>
                    {formatPct(r.change_pct)}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-sm text-right text-text-muted" data-numeric>
                    {formatVolume(r.volume)}
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
