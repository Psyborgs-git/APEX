import React, { useEffect, useRef, useCallback, useState } from 'react';
import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type CandlestickData,
  type HistogramData,
  type Time,
  ColorType,
  CrosshairMode,
} from 'lightweight-charts';
import { useMarketStore } from '../../stores/marketStore';
import { getHistoricalData } from '../../lib/tauri';
import type { OHLCVDto } from '../../lib/types';
import { formatPrice, formatVolume } from '../../lib/format';

interface CandleChartProps {
  symbol: string;
  ohlcvData?: OHLCVDto[];
  height?: number;
}

type ChartTimeframe = '5m' | '15m' | '1h' | '1d';

const TIMEFRAMES: { id: ChartTimeframe; label: string; bucketSecs: number }[] = [
  { id: '5m', label: '5m', bucketSecs: 300 },
  { id: '15m', label: '15m', bucketSecs: 900 },
  { id: '1h', label: '1H', bucketSecs: 3600 },
  { id: '1d', label: '1D', bucketSecs: 86400 },
];

function toChartTime(iso: string): Time {
  return (new Date(iso).getTime() / 1000) as Time;
}

const CHART_COLORS = {
  background: '#0a0a0f',
  text: '#a0a0b8',
  grid: '#1a1a25',
  border: '#2a2a3a',
  bull: '#00c853',
  bear: '#ff1744',
  volumeUp: 'rgba(0, 200, 83, 0.3)',
  volumeDown: 'rgba(255, 23, 68, 0.3)',
  crosshair: '#5c5c7a',
} as const;

const CandleChartInner: React.FC<CandleChartProps> = ({ symbol, ohlcvData, height }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<'Candlestick'> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<'Histogram'> | null>(null);
  const lastBarTimeRef = useRef<number>(0);
  const getQuote = useMarketStore((s) => s.getQuote);
  const quote = useMarketStore((s) => s.quotes.get(symbol));
  const [timeframe, setTimeframe] = useState<ChartTimeframe>('1d');
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const bucketSecs = TIMEFRAMES.find((t) => t.id === timeframe)?.bucketSecs ?? 86400;

  const initChart = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    // Dispose previous chart
    if (chartRef.current) {
      chartRef.current.remove();
      chartRef.current = null;
    }

    const chart = createChart(container, {
      layout: {
        background: { type: ColorType.Solid, color: CHART_COLORS.background },
        textColor: CHART_COLORS.text,
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 11,
      },
      grid: {
        vertLines: { color: CHART_COLORS.grid },
        horzLines: { color: CHART_COLORS.grid },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: CHART_COLORS.crosshair, labelBackgroundColor: CHART_COLORS.border },
        horzLine: { color: CHART_COLORS.crosshair, labelBackgroundColor: CHART_COLORS.border },
      },
      rightPriceScale: {
        borderColor: CHART_COLORS.border,
      },
      localization: {
        // System locale can be 'C' (no ICU data in some webviews) — pin a real
        // locale so price/time formatting never throws.
        locale: 'en-US',
      },
      timeScale: {
        borderColor: CHART_COLORS.border,
        timeVisible: true,
        secondsVisible: false,
      },
      width: container.clientWidth,
      height: height ?? container.clientHeight,
    });

    const candleSeries = chart.addCandlestickSeries({
      upColor: CHART_COLORS.bull,
      downColor: CHART_COLORS.bear,
      borderDownColor: CHART_COLORS.bear,
      borderUpColor: CHART_COLORS.bull,
      wickDownColor: CHART_COLORS.bear,
      wickUpColor: CHART_COLORS.bull,
    });

    const volumeSeries = chart.addHistogramSeries({
      priceFormat: { type: 'volume' },
      priceScaleId: 'volume',
    });

    chart.priceScale('volume').applyOptions({
      scaleMargins: { top: 0.8, bottom: 0 },
    });

    chartRef.current = chart;
    candleSeriesRef.current = candleSeries;
    volumeSeriesRef.current = volumeSeries;

    return chart;
  }, [height]);

  const setBars = useCallback((bars: OHLCVDto[]) => {
    const candleSeries = candleSeriesRef.current;
    const volumeSeries = volumeSeriesRef.current;
    const chart = chartRef.current;
    if (!candleSeries || !volumeSeries || !chart || bars.length === 0) return;

    // Collapse duplicate bar timestamps — a charting series requires strictly
    // ascending unique times.
    const seen = new Set<number>();
    const uniqueBars = bars.filter((bar) => {
      const t = new Date(bar.time).getTime();
      if (seen.has(t)) return false;
      seen.add(t);
      return true;
    });

    const candles: CandlestickData<Time>[] = uniqueBars.map((bar) => ({
      time: toChartTime(bar.time),
      open: bar.open,
      high: bar.high,
      low: bar.low,
      close: bar.close,
    }));

    const volumes: HistogramData<Time>[] = uniqueBars.map((bar) => ({
      time: toChartTime(bar.time),
      value: bar.volume,
      color: bar.close >= bar.open ? CHART_COLORS.volumeUp : CHART_COLORS.volumeDown,
    }));

    candleSeries.setData(candles);
    volumeSeries.setData(volumes);
    lastBarTimeRef.current = Number(candles[candles.length - 1].time);
    chart.timeScale().fitContent();
  }, []);

  // Initialize chart once
  useEffect(() => {
    const chart = initChart();
    if (!chart) return;

    const handleResize = () => {
      const container = containerRef.current;
      if (container && chartRef.current) {
        chartRef.current.applyOptions({
          width: container.clientWidth,
          height: height ?? container.clientHeight,
        });
      }
    };

    const resizeObserver = new ResizeObserver(handleResize);
    if (containerRef.current) {
      resizeObserver.observe(containerRef.current);
    }

    return () => {
      resizeObserver.disconnect();
      if (chartRef.current) {
        chartRef.current.remove();
        chartRef.current = null;
      }
    };
  }, [height, initChart]);

  // Load OHLCV bars when symbol or timeframe changes
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setLoadError(null);

    const load = async () => {
      try {
        const bars = ohlcvData && ohlcvData.length > 0
          ? ohlcvData
          : await getHistoricalData(symbol, timeframe, 500);
        if (cancelled) return;
        setBars(bars);
      } catch (err) {
        if (!cancelled) {
          setLoadError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to load chart data');
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, [symbol, timeframe, ohlcvData, setBars]);

  // Listen for real-time quote updates — bucket ticks into the active timeframe
  useEffect(() => {
    if (!symbol) return;

    const intervalId = setInterval(() => {
      const q = getQuote(symbol);
      if (!q || !candleSeriesRef.current || !volumeSeriesRef.current) return;
      if (![q.open, q.high, q.low, q.last, q.volume].every((v) => Number.isFinite(v))) return;

      // Ticks must never update at a time earlier than the last stored bar
      // (e.g. today's daily bar is timestamped at market open, after the
      // midnight bucket boundary) — lwc rejects that as 'oldest data'.
      const bucketStart = Math.max(
        Math.floor(Date.now() / 1000 / bucketSecs) * bucketSecs,
        lastBarTimeRef.current,
      ) as Time;
      candleSeriesRef.current.update({
        time: bucketStart,
        open: q.open,
        high: q.high,
        low: q.low,
        close: q.last,
      });

      volumeSeriesRef.current.update({
        time: bucketStart,
        value: q.volume,
        color: q.last >= q.open ? CHART_COLORS.volumeUp : CHART_COLORS.volumeDown,
      });
    }, 1000);

    return () => clearInterval(intervalId);
  }, [symbol, getQuote, bucketSecs]);

  return (
    <div className="flex flex-col h-full" data-testid="candle-chart">
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-sm font-mono font-medium text-text-primary">{symbol}</span>
          {quote && (
            <>
              <span className="text-sm font-mono text-text-primary">{formatPrice(quote.last)}</span>
              <span className={`text-xs font-mono ${quote.change_pct >= 0 ? 'text-bull' : 'text-bear'}`}>
                {quote.change_pct >= 0 ? '+' : ''}{quote.change_pct.toFixed(2)}%
              </span>
              <span className="text-xs font-mono text-text-muted">Vol {formatVolume(quote.volume)}</span>
            </>
          )}
        </div>
        <div className="flex items-center gap-1" data-testid="chart-timeframes">
          {TIMEFRAMES.map((tf) => (
            <button
              key={tf.id}
              onClick={() => setTimeframe(tf.id)}
              className={`px-2 py-0.5 text-[10px] font-mono uppercase tracking-wider rounded transition-colors ${
                timeframe === tf.id
                  ? 'bg-accent/15 text-accent'
                  : 'text-text-muted hover:text-text-primary'
              }`}
              data-testid={`chart-tf-${tf.id}`}
            >
              {tf.label}
            </button>
          ))}
        </div>
      </div>
      <div className="relative flex-1 min-h-0">
        <div ref={containerRef} className="absolute inset-0" />
        {loading && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <span className="text-xs font-mono text-text-muted animate-pulse">Loading bars…</span>
          </div>
        )}
        {loadError && !loading && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <span className="text-xs font-mono text-bear" data-testid="chart-load-error">{loadError}</span>
          </div>
        )}
      </div>
    </div>
  );
};

export const CandleChart = React.memo(CandleChartInner);
CandleChart.displayName = 'CandleChart';
