import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createChart } from 'lightweight-charts';
import type { IChartApi, UTCTimestamp } from 'lightweight-charts';
import { listStrategyFiles, runStrategyBacktest } from '../../lib/tauri';
import type { StrategyBacktestResultDto, StrategyFileDto } from '../../lib/types';
import { useMarketStore } from '../../stores/marketStore';
import { chartTheme, useThemeTick, withAlpha } from '../../lib/chartTheme';

const TF_OPTIONS = ['d1', '1h', '15m', '5m', '1m', 'w1'];

function fmtDate(d: Date): string {
  return d.toISOString().slice(0, 10);
}

function toTs(time: string): UTCTimestamp {
  return Math.floor(new Date(time).getTime() / 1000) as UTCTimestamp;
}

/** Chart chrome resolved from the active theme at mount. */
function chartOpts() {
  const t = chartTheme();
  return {
    height: 160,
    layout: {
      background: { color: 'transparent' },
      textColor: t.textMuted,
      fontSize: 10,
    },
    grid: {
      vertLines: { color: withAlpha(t.textMuted, 10) },
      horzLines: { color: withAlpha(t.textMuted, 10) },
    },
    timeScale: { borderColor: t.border },
    rightPriceScale: { borderColor: t.border },
    localization: { locale: 'en-US' },
  } as const;
}

const EquityChart: React.FC<{ result: StrategyBacktestResultDto }> = ({ result }) => {
  const eqRef = useRef<HTMLDivElement>(null);
  const ddRef = useRef<HTMLDivElement>(null);
  const themeTick = useThemeTick();

  useEffect(() => {
    if (!eqRef.current || !ddRef.current || result.equity_curve.length === 0) return;
    const t = chartTheme();
    const eqChart: IChartApi = createChart(eqRef.current, { ...chartOpts(), height: 170 });
    const ddChart: IChartApi = createChart(ddRef.current, { ...chartOpts(), height: 90 });
    const eq = eqChart.addLineSeries({ color: t.accent, lineWidth: 2 });
    const dd = ddChart.addAreaSeries({
      lineColor: t.bear,
      topColor: withAlpha(t.bear, 15),
      bottomColor: withAlpha(t.bear, 45),
      lineWidth: 1,
    });
    eq.setData(result.equity_curve.map((p) => ({ time: toTs(p.time), value: p.equity })));
    dd.setData(result.equity_curve.map((p) => ({ time: toTs(p.time), value: -Math.abs(p.drawdown) * 100 })));
    eqChart.timeScale().fitContent();
    ddChart.timeScale().fitContent();
    return () => {
      eqChart.remove();
      ddChart.remove();
    };
     
  }, [result, themeTick]);

  return (
    <div className="space-y-1">
      <div ref={eqRef} data-testid="backtest-equity-chart" />
      <div className="text-[10px] uppercase tracking-wide text-text-muted px-1">Drawdown %</div>
      <div ref={ddRef} data-testid="backtest-drawdown-chart" />
    </div>
  );
};

const MetricCell: React.FC<{ label: string; value: string; tone?: 'bull' | 'bear' }> = ({ label, value, tone }) => (
  <div className="rounded border border-[var(--border-color)] bg-surface-1 px-2.5 py-1.5">
    <div className="text-[10px] uppercase tracking-wide text-text-muted">{label}</div>
    <div className={`font-mono text-sm ${tone === 'bull' ? 'text-bull' : tone === 'bear' ? 'text-bear' : 'text-text-primary'}`}>
      {value}
    </div>
  </div>
);

export const BacktestPanel: React.FC<{ defaultSymbol?: string }> = ({ defaultSymbol }) => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const [strategies, setStrategies] = useState<StrategyFileDto[]>([]);
  const [path, setPath] = useState('');
  const [symbol, setSymbol] = useState(defaultSymbol ?? watchlist[0] ?? 'RELIANCE.NS');
  const [timeframe, setTimeframe] = useState('d1');
  const [from, setFrom] = useState(() => fmtDate(new Date(Date.now() - 730 * 86400_000)));
  const [to, setTo] = useState(() => fmtDate(new Date()));
  const [quantity, setQuantity] = useState(10);
  const [capital, setCapital] = useState(100_000);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<StrategyBacktestResultDto | null>(null);

  useEffect(() => {
    listStrategyFiles()
      .then((files) => {
        setStrategies(files);
        if (!path && files.length > 0) setPath(files[0].path);
      })
      .catch(() => setStrategies([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = useCallback(async () => {
    if (!path || !symbol.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await runStrategyBacktest({
        path,
        symbol: symbol.trim().toUpperCase(),
        timeframe,
        from,
        to,
        quantity,
        initial_capital: capital,
      });
      setResult(res);
    } catch (err) {
      setResult(null);
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Backtest failed');
    } finally {
      setBusy(false);
    }
  }, [path, symbol, timeframe, from, to, quantity, capital, busy]);

  const m = result?.metrics;
  const metricCells = useMemo(() => {
    if (!result || !m) return [];
    const retTone = m.total_return_pct >= 0 ? 'bull' as const : 'bear' as const;
    return [
      { label: 'Return', value: `${m.total_return_pct.toFixed(2)}%`, tone: retTone },
      { label: 'Ann. Return', value: `${m.annualized_return_pct.toFixed(2)}%`, tone: retTone },
      { label: 'Sharpe', value: m.sharpe_ratio.toFixed(2) },
      { label: 'Max DD', value: `${m.max_drawdown_pct.toFixed(2)}%`, tone: 'bear' as const },
      { label: 'Trades', value: `${m.total_trades}` },
      { label: 'Win Rate', value: `${m.win_rate.toFixed(1)}%` },
      { label: 'Profit Factor', value: m.profit_factor.toFixed(2) },
      { label: 'Avg Trade', value: m.avg_trade_pnl.toFixed(2), tone: m.avg_trade_pnl >= 0 ? retTone : 'bear' as const },
      { label: 'Final Equity', value: m.final_equity.toFixed(0) },
      { label: 'Bars', value: `${result.bars_analyzed}` },
    ];
  }, [result, m]);

  const inputCls = 'px-2 py-1 text-sm bg-surface-0 border border-[var(--border-color)] rounded min-w-0';

  return (
    <div className="flex flex-col h-full overflow-auto" data-testid="backtest-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium text-text-secondary mr-1">Backtest</span>
        <select
          value={path}
          onChange={(e) => setPath(e.target.value)}
          className={inputCls}
          data-testid="backtest-strategy-select"
        >
          {strategies.map((f) => (
            <option key={f.path} value={f.path}>{f.name}</option>
          ))}
          {strategies.length === 0 && <option value="">(no strategy files)</option>}
        </select>
        <input
          value={symbol}
          onChange={(e) => setSymbol(e.target.value)}
          placeholder="SYMBOL"
          className={`${inputCls} w-32 font-mono`}
          data-testid="backtest-symbol"
        />
        <select value={timeframe} onChange={(e) => setTimeframe(e.target.value)} className={inputCls} data-testid="backtest-timeframe">
          {TF_OPTIONS.map((t) => <option key={t} value={t}>{t}</option>)}
        </select>
        <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} className={inputCls} data-testid="backtest-from" />
        <input type="date" value={to} onChange={(e) => setTo(e.target.value)} className={inputCls} data-testid="backtest-to" />
        <input
          type="number" value={quantity} min={1} onChange={(e) => setQuantity(Number(e.target.value) || 1)}
          className={`${inputCls} w-20`} title="Quantity" data-testid="backtest-quantity"
        />
        <input
          type="number" value={capital} min={1} step={10000} onChange={(e) => setCapital(Number(e.target.value) || 100_000)}
          className={`${inputCls} w-28`} title="Initial capital" data-testid="backtest-capital"
        />
        <button
          onClick={() => void run()}
          disabled={busy || !path || !symbol.trim()}
          className="px-3 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded disabled:opacity-50"
          data-testid="backtest-run"
        >
          {busy ? 'Running…' : 'Run'}
        </button>
      </div>

      <div className="flex-1 p-3 space-y-3">
        {error && <p className="text-xs text-bear" data-testid="backtest-error">{error}</p>}

        {!result && !error && (
          <p className="text-xs text-text-muted" data-testid="backtest-empty">
            Pick a strategy file and symbol, then run a backtest. Results show the equity
            curve, drawdown profile, headline metrics, and the full trade log.
          </p>
        )}

        {result && (
          <>
            <div className="flex items-center gap-2 text-xs text-text-muted" data-testid="backtest-meta">
              <span className="text-text-primary font-medium">{result.strategy_name}</span>
              <span className="rounded bg-surface-2 px-2 py-0.5">{result.inferred_strategy}</span>
              <span className="font-mono">{result.symbol} · {result.timeframe}</span>
            </div>

            <div className="grid grid-cols-5 gap-2" data-testid="backtest-metrics">
              {metricCells.map((c) => <MetricCell key={c.label} {...c} />)}
            </div>

            <EquityChart result={result} />

            <div data-testid="backtest-trades">
              <div className="text-[10px] uppercase tracking-wide text-text-muted mb-1">Trades ({result.trades.length})</div>
              <div className="rounded border border-[var(--border-color)] overflow-auto max-h-56">
                <table className="w-full text-xs font-mono">
                  <thead className="bg-surface-1 text-text-muted sticky top-0">
                    <tr>
                      <th className="text-left px-2 py-1">Side</th>
                      <th className="text-left px-2 py-1">Entry</th>
                      <th className="text-right px-2 py-1">Px</th>
                      <th className="text-left px-2 py-1">Exit</th>
                      <th className="text-right px-2 py-1">Px</th>
                      <th className="text-right px-2 py-1">Qty</th>
                      <th className="text-right px-2 py-1">P&L</th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.trades.map((t, i) => (
                      <tr key={i} className="border-t border-[var(--border-color)]">
                        <td className={`px-2 py-1 ${t.side.toLowerCase() === 'buy' ? 'text-bull' : 'text-bear'}`}>{t.side}</td>
                        <td className="px-2 py-1">{t.entry_time.slice(0, 10)}</td>
                        <td className="px-2 py-1 text-right">{t.entry_price.toFixed(2)}</td>
                        <td className="px-2 py-1">{t.exit_time?.slice(0, 10) ?? '—'}</td>
                        <td className="px-2 py-1 text-right">{t.exit_price?.toFixed(2) ?? '—'}</td>
                        <td className="px-2 py-1 text-right">{t.quantity}</td>
                        <td className={`px-2 py-1 text-right ${t.pnl >= 0 ? 'text-bull' : 'text-bear'}`}>{t.pnl.toFixed(2)}</td>
                      </tr>
                    ))}
                    {result.trades.length === 0 && (
                      <tr><td colSpan={7} className="px-2 py-3 text-center text-text-muted">No trades generated</td></tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>

            {result.notes.length > 0 && (
              <div className="rounded border border-[var(--border-color)] bg-surface-1 px-3 py-2 text-xs text-text-muted space-y-0.5" data-testid="backtest-notes">
                {result.notes.map((n, i) => <p key={i}>· {n}</p>)}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
};
