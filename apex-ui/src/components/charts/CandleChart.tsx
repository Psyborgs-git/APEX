import React, { useEffect, useRef, useCallback } from 'react';
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
import type { OHLCVDto } from '../../lib/types';
import { formatPrice, formatVolume } from '../../lib/format';

interface CandleChartProps {
  symbol: string;
  ohlcvData?: OHLCVDto[];
  height?: number;
}

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
  const getQuote = useMarketStore((s) => s.getQuote);

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

    // Load initial data
    if (ohlcvData && ohlcvData.length > 0) {
      const candles: CandlestickData<Time>[] = ohlcvData.map((bar) => ({
        time: toChartTime(bar.time),
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
      }));

      const volumes: HistogramData<Time>[] = ohlcvData.map((bar) => ({
        time: toChartTime(bar.time),
        value: bar.volume,
        color: bar.close >= bar.open ? CHART_COLORS.volumeUp : CHART_COLORS.volumeDown,
      }));

      candleSeries.setData(candles);
      volumeSeries.setData(volumes);
      chart.timeScale().fitContent();
    }

    return chart;
  }, [ohlcvData, height]);

  // Initialize chart
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
  }, [initChart]);

  // Listen for real-time quote updates
  useEffect(() => {
    if (!symbol) return;

    const intervalId = setInterval(() => {
      const quote = getQuote(symbol);
      if (!quote || !candleSeriesRef.current || !volumeSeriesRef.current) return;

      const now = (Math.floor(Date.now() / 1000)) as Time;
      candleSeriesRef.current.update({
        time: now,
        open: quote.open,
        high: quote.high,
        low: quote.low,
        close: quote.last,
      });

      volumeSeriesRef.current.update({
        time: now,
        value: quote.volume,
        color: quote.last >= quote.open ? CHART_COLORS.volumeUp : CHART_COLORS.volumeDown,
      });
    }, 1000);

    return () => clearInterval(intervalId);
  }, [symbol, getQuote]);

  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-sm font-mono font-medium text-text-primary">{symbol}</span>
          {(() => {
            const q = getQuote(symbol);
            if (!q) return null;
            return (
              <>
                <span className="text-sm font-mono text-text-primary">{formatPrice(q.last)}</span>
                <span className={`text-xs font-mono ${q.change_pct >= 0 ? 'text-bull' : 'text-bear'}`}>
                  {q.change_pct >= 0 ? '+' : ''}{q.change_pct.toFixed(2)}%
                </span>
                <span className="text-xs font-mono text-text-muted">Vol {formatVolume(q.volume)}</span>
              </>
            );
          })()}
        </div>
        <span className="text-xs text-text-muted font-mono">1D</span>
      </div>
      <div ref={containerRef} className="flex-1 min-h-0" />
    </div>
  );
};

export const CandleChart = React.memo(CandleChartInner);
CandleChart.displayName = 'CandleChart';
