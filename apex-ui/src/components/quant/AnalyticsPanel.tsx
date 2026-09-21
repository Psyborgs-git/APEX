import React, { useEffect, useRef, useState } from 'react';
import {
  createChart,
  type IChartApi,
  ColorType,
} from 'lightweight-charts';
import { getQuantStats, getRegression } from '../../lib/tauri';
import type { QuantStatsDto, RegressionDto, SeriesPointDto } from '../../lib/types';

interface AnalyticsPanelProps {
  defaultSymbol?: string;
}

const PANE_BG = '#0a0a0f';
const PANE_TEXT = '#a0a0b8';
const PANE_GRID = '#1a1a25';
const PANE_BORDER = '#2a2a3a';

function toTime(iso: string) {
  return (new Date(iso).getTime() / 1000) as import('lightweight-charts').Time;
}

/** Small lwc line chart for rolling stat series. */
const LinePane: React.FC<{ points: SeriesPointDto[]; color: string; height?: number; title: string }> = ({
  points,
  color,
  height = 130,
  title,
}) => {
  const ref = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    const chart = createChart(ref.current, {
      layout: {
        background: { type: ColorType.Solid, color: PANE_BG },
        textColor: PANE_TEXT,
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 10,
      },
      grid: { vertLines: { color: PANE_GRID }, horzLines: { color: PANE_GRID } },
      rightPriceScale: { borderColor: PANE_BORDER },
      localization: { locale: 'en-US' },
      timeScale: { borderColor: PANE_BORDER, timeVisible: true, secondsVisible: false },
      width: ref.current.clientWidth,
      height,
    });
    chartRef.current = chart;
    return () => {
      chart.remove();
      chartRef.current = null;
    };
  }, [height]);

  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;
    const seen = new Set<number>();
    const data = points
      .filter((p) => {
        const t = new Date(p.time).getTime();
        if (!Number.isFinite(t) || seen.has(t)) return false;
        seen.add(t);
        return Number.isFinite(p.value);
      })
      .map((p) => ({ time: toTime(p.time), value: p.value }));
    const line = chart.addLineSeries({ color, lineWidth: 1, priceLineVisible: false });
    line.setData(data);
    chart.timeScale().fitContent();
  }, [points, color]);

  return (
    <div className="flex flex-col">
      <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-wider text-text-muted">{title}</div>
      <div ref={ref} style={{ height }} />
    </div>
  );
};

function fmt(v: number, digits = 4): string {
  if (!Number.isFinite(v)) return '—';
  if (Math.abs(v) >= 1000) return v.toFixed(2);
  return v.toFixed(digits);
}

const StatCell: React.FC<{ label: string; value: string; tone?: 'bull' | 'bear' | 'muted' }> = ({ label, value, tone }) => (
  <div className="px-3 py-2 border-r border-b border-[var(--border-color)]">
    <div className="text-[9px] font-mono uppercase tracking-wider text-text-muted">{label}</div>
    <div
      className={`text-sm font-mono ${
        tone === 'bull' ? 'text-bull' : tone === 'bear' ? 'text-bear' : tone === 'muted' ? 'text-text-muted' : 'text-text-primary'
      }`}
    >
      {value}
    </div>
  </div>
);

const AnalyticsPanelInner: React.FC<AnalyticsPanelProps> = ({ defaultSymbol }) => {
  const [symbol, setSymbol] = useState(defaultSymbol ?? 'AAPL');
  const [benchmark, setBenchmark] = useState('SPY');
  const [timeframe, setTimeframe] = useState('1d');
  const [stats, setStats] = useState<QuantStatsDto | null>(null);
  const [reg, setReg] = useState<RegressionDto | null>(null);
  const [regError, setRegError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSymbol) setSymbol(defaultSymbol);
  }, [defaultSymbol]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void Promise.all([
      getQuantStats(symbol, timeframe),
      getRegression(benchmark, symbol, timeframe).catch((e: unknown) => {
        setRegError(typeof e === 'string' ? e : 'regression unavailable');
        return null;
      }),
    ])
      .then(([s, r]) => {
        if (cancelled) return;
        setStats(s);
        setReg(r);
        if (r) setRegError(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setStats(null);
          setReg(null);
          setError(typeof e === 'string' ? e : 'quant stats failed');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [symbol, benchmark, timeframe]);

  // Scatter plot bounds
  const scatterBox = { w: 320, h: 200, pad: 26 };
  const scatterRender = (() => {
    if (!reg || reg.scatter.length === 0) return null;
    const xs = reg.scatter.map((p) => p[0]);
    const ys = reg.scatter.map((p) => p[1]);
    const xMin = Math.min(...xs);
    const xMax = Math.max(...xs);
    const yMin = Math.min(...ys);
    const yMax = Math.max(...ys);
    const sx = (x: number) => scatterBox.pad + ((x - xMin) / (xMax - xMin || 1)) * (scatterBox.w - 2 * scatterBox.pad);
    const sy = (y: number) => scatterBox.h - scatterBox.pad - ((y - yMin) / (yMax - yMin || 1)) * (scatterBox.h - 2 * scatterBox.pad);
    return { sx, sy };
  })();

  const acfMax = stats ? Math.max(0.3, ...stats.autocorr.map(Math.abs)) : 1;

  return (
    <div className="flex flex-col h-full overflow-hidden" data-testid="analytics-panel">
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center gap-2">
        <span className="text-[10px] font-mono uppercase tracking-wider text-text-muted">Quant</span>
        <input
          value={symbol}
          onChange={(e) => setSymbol(e.target.value.toUpperCase())}
          className="w-20 px-1.5 py-0.5 text-xs font-mono bg-surface-2 border border-[var(--border-color)] rounded text-text-primary uppercase"
          data-testid="quant-symbol-input"
        />
        <span className="text-[10px] font-mono text-text-muted">vs</span>
        <input
          value={benchmark}
          onChange={(e) => setBenchmark(e.target.value.toUpperCase())}
          className="w-16 px-1.5 py-0.5 text-xs font-mono bg-surface-2 border border-[var(--border-color)] rounded text-text-primary uppercase"
          title="Regression benchmark"
          data-testid="quant-benchmark-input"
        />
        <select
          value={timeframe}
          onChange={(e) => setTimeframe(e.target.value)}
          className="px-1 py-0.5 text-[10px] font-mono bg-surface-2 border border-[var(--border-color)] rounded text-text-primary"
          data-testid="quant-timeframe-select"
        >
          <option value="5m">5m</option>
          <option value="15m">15m</option>
          <option value="1h">1H</option>
          <option value="1d">1D</option>
        </select>
        {loading && <span className="text-[10px] font-mono text-text-muted animate-pulse">computing…</span>}
      </div>

      {error && (
        <div className="px-3 py-2 text-xs font-mono text-bear" data-testid="quant-error">{error}</div>
      )}

      <div className="flex-1 overflow-y-auto">
        {stats && (
          <>
            <div className="grid grid-cols-6 border-b border-[var(--border-color)]" data-testid="quant-stats-grid">
              <StatCell label="Sharpe" value={fmt(stats.sharpe, 2)} tone={stats.sharpe >= 0 ? 'bull' : 'bear'} />
              <StatCell label="Sortino" value={fmt(stats.sortino, 2)} tone={stats.sortino >= 0 ? 'bull' : 'bear'} />
              <StatCell label="Omega" value={fmt(stats.omega, 2)} tone={stats.omega >= 1 ? 'bull' : 'bear'} />
              <StatCell label="Ann Vol" value={`${(stats.ann_volatility * 100).toFixed(1)}%`} />
              <StatCell label="Max DD" value={`${(stats.max_drawdown * 100).toFixed(1)}%`} tone="bear" />
              <StatCell label="N" value={String(stats.n)} tone="muted" />
              <StatCell label="Mean μ" value={fmt(stats.mean)} />
              <StatCell label="Stdev σ" value={fmt(stats.std_dev)} />
              <StatCell label="Skew" value={fmt(stats.skewness, 2)} tone={stats.skewness >= 0 ? 'bull' : 'bear'} />
              <StatCell label="Kurtosis" value={fmt(stats.kurtosis, 2)} />
              <StatCell label="JB" value={fmt(stats.jarque_bera, 1)} />
              <StatCell label="Normal?" value={stats.normal ? 'YES' : 'NO'} tone={stats.normal ? 'bull' : 'bear'} />
              <StatCell label="Min" value={`${(stats.min * 100).toFixed(2)}%`} tone="bear" />
              <StatCell label="Q05" value={`${(stats.q05 * 100).toFixed(2)}%`} />
              <StatCell label="Q25" value={`${(stats.q25 * 100).toFixed(2)}%`} />
              <StatCell label="Median" value={`${(stats.median * 100).toFixed(2)}%`} />
              <StatCell label="Q75" value={`${(stats.q75 * 100).toFixed(2)}%`} />
              <StatCell label="Q95 / Max" value={`${(stats.q95 * 100).toFixed(2)}% / ${(stats.max * 100).toFixed(2)}%`} />
            </div>

            <div className="grid grid-cols-2 gap-0">
              <div className="border-r border-[var(--border-color)]">
                <LinePane points={stats.rolling_vol} color="#f59e0b" title="Rolling Volatility (21, ann.)" />
              </div>
              <LinePane points={stats.rolling_sharpe} color="#22d3ee" title="Rolling Sharpe (21)" />
            </div>

            <div className="border-t border-[var(--border-color)]" data-testid="quant-acf">
              <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-wider text-text-muted">
                Autocorrelation of Returns (lags 1–10)
              </div>
              <div className="flex items-end gap-1 px-3 pb-2 h-24">
                {stats.autocorr.map((v, i) => (
                  <div key={i} className="flex-1 flex flex-col items-center justify-end h-full">
                    <div
                      className={`w-full rounded-sm opacity-70 ${v >= 0 ? 'bg-accent' : 'bg-bear'}`}
                      style={{ height: `${Math.min(100, (Math.abs(v) / acfMax) * 88) + 4}%` }}
                      title={`lag ${i + 1}: ${v.toFixed(3)}`}
                    />
                    <span className="text-[8px] font-mono text-text-muted mt-0.5">{i + 1}</span>
                  </div>
                ))}
              </div>
            </div>
          </>
        )}

        {stats && !reg && (
          <div className="border-t border-[var(--border-color)] px-3 py-2 text-xs font-mono text-text-muted" data-testid="quant-regression-empty">
            OLS regression unavailable{regError ? `: ${regError}` : ` — insufficient overlapping data between ${symbol} and ${benchmark}`}
          </div>
        )}

        {reg && scatterRender && (
          <div className="border-t border-[var(--border-color)]" data-testid="quant-regression">
            <div className="px-2 py-1 text-[9px] font-mono uppercase tracking-wider text-text-muted">
              OLS — {reg.y_symbol} on {reg.x_symbol}
            </div>
            <div className="flex">
              <svg width={scatterBox.w} height={scatterBox.h} className="shrink-0">
                {reg.scatter.map(([x, y], i) => (
                  <circle key={i} cx={scatterRender.sx(x)} cy={scatterRender.sy(y)} r={2} fill="#22d3ee" opacity={0.6} />
                ))}
                {reg.fit_line.length === 2 && (
                  <line
                    x1={scatterRender.sx(reg.fit_line[0][0])}
                    y1={scatterRender.sy(reg.fit_line[0][1])}
                    x2={scatterRender.sx(reg.fit_line[1][0])}
                    y2={scatterRender.sy(reg.fit_line[1][1])}
                    stroke="#f59e0b"
                    strokeWidth={1.5}
                  />
                )}
              </svg>
              <div className="flex-1 grid grid-cols-2 content-start">
                <StatCell label="α Alpha" value={fmt(reg.alpha, 5)} />
                <StatCell label="β Beta" value={fmt(reg.beta, 3)} tone={reg.beta >= 0 ? 'bull' : 'bear'} />
                <StatCell label="R²" value={fmt(reg.r_squared, 3)} />
                <StatCell label="Obs" value={String(reg.n)} tone="muted" />
              </div>
            </div>
            <LinePane points={reg.residuals} color="#a78bfa" title="Residuals" height={110} />
          </div>
        )}

        {!stats && !loading && !error && (
          <div className="p-6 text-xs font-mono text-text-muted">No data — pick a symbol with stored bars.</div>
        )}
      </div>
    </div>
  );
};

export const AnalyticsPanel = React.memo(AnalyticsPanelInner);
AnalyticsPanel.displayName = 'AnalyticsPanel';
