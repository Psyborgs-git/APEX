import React, { useEffect, useRef, useCallback } from 'react';
import { formatPrice, formatVolume } from '../../lib/format';

interface OrderBookLevel {
  price: number;
  quantity: number;
}

interface OrderBookHeatmapProps {
  bids: OrderBookLevel[];
  asks: OrderBookLevel[];
  spread?: number;
  maxDepth?: number;
}

const COLORS = {
  bidFill: 'rgba(0, 200, 83, 0.35)',
  bidStroke: 'rgba(0, 200, 83, 0.7)',
  askFill: 'rgba(255, 23, 68, 0.35)',
  askStroke: 'rgba(255, 23, 68, 0.7)',
  background: '#0a0a0f',
  gridLine: '#1a1a25',
  text: '#a0a0b8',
  textMuted: '#5c5c7a',
  midLine: '#448aff',
} as const;

const TARGET_FPS = 30;
const FRAME_INTERVAL = 1000 / TARGET_FPS;

function drawOrderBook(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  bids: OrderBookLevel[],
  asks: OrderBookLevel[],
  dpr: number,
): void {
  ctx.save();
  ctx.scale(dpr, dpr);

  const logicalW = width / dpr;
  const logicalH = height / dpr;
  const midX = logicalW / 2;
  const padding = { top: 24, bottom: 20, left: 0, right: 0 };
  const chartH = logicalH - padding.top - padding.bottom;

  // Clear
  ctx.fillStyle = COLORS.background;
  ctx.fillRect(0, 0, logicalW, logicalH);

  if (bids.length === 0 && asks.length === 0) {
    ctx.font = '11px "JetBrains Mono", monospace';
    ctx.fillStyle = COLORS.textMuted;
    ctx.textAlign = 'center';
    ctx.fillText('No order book data', midX, logicalH / 2);
    ctx.restore();
    return;
  }

  // Compute cumulative depths
  const bidCum: number[] = [];
  let cum = 0;
  for (const level of bids) {
    cum += level.quantity;
    bidCum.push(cum);
  }

  const askCum: number[] = [];
  cum = 0;
  for (const level of asks) {
    cum += level.quantity;
    askCum.push(cum);
  }

  const maxCum = Math.max(bidCum[bidCum.length - 1] ?? 0, askCum[askCum.length - 1] ?? 0, 1);

  // Draw center line
  ctx.strokeStyle = COLORS.midLine;
  ctx.lineWidth = 1;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  ctx.moveTo(midX, padding.top);
  ctx.lineTo(midX, logicalH - padding.bottom);
  ctx.stroke();
  ctx.setLineDash([]);

  // Header
  ctx.font = '10px "JetBrains Mono", monospace';
  ctx.fillStyle = COLORS.text;
  ctx.textAlign = 'left';
  ctx.fillText('BIDS', 8, 14);
  ctx.textAlign = 'right';
  ctx.fillText('ASKS', logicalW - 8, 14);

  // Spread
  if (bids.length > 0 && asks.length > 0) {
    const spreadVal = asks[0].price - bids[0].price;
    ctx.textAlign = 'center';
    ctx.fillStyle = COLORS.textMuted;
    ctx.fillText(`Spread: ${formatPrice(spreadVal)}`, midX, 14);
  }

  // Draw bid side (left, from center going left)
  const bidBarWidth = midX - 4;
  drawSide(ctx, bids, bidCum, maxCum, {
    x: 0,
    y: padding.top,
    width: bidBarWidth,
    height: chartH,
    fillStyle: COLORS.bidFill,
    strokeStyle: COLORS.bidStroke,
    direction: 'left',
  });

  // Draw ask side (right, from center going right)
  const askBarWidth = midX - 4;
  drawSide(ctx, asks, askCum, maxCum, {
    x: midX + 4,
    y: padding.top,
    width: askBarWidth,
    height: chartH,
    fillStyle: COLORS.askFill,
    strokeStyle: COLORS.askStroke,
    direction: 'right',
  });

  // Price labels
  ctx.font = '9px "JetBrains Mono", monospace';
  ctx.fillStyle = COLORS.textMuted;

  const maxLevels = Math.min(bids.length, Math.floor(chartH / 14));
  const bidStep = Math.max(1, Math.floor(bids.length / maxLevels));
  for (let i = 0; i < bids.length && i < maxLevels * bidStep; i += bidStep) {
    const rowY = padding.top + (i / Math.max(bids.length - 1, 1)) * chartH;
    ctx.textAlign = 'right';
    ctx.fillText(formatPrice(bids[i].price), midX - 8, rowY + 3);
  }

  const askMaxLevels = Math.min(asks.length, Math.floor(chartH / 14));
  const askStep = Math.max(1, Math.floor(asks.length / askMaxLevels));
  for (let i = 0; i < asks.length && i < askMaxLevels * askStep; i += askStep) {
    const rowY = padding.top + (i / Math.max(asks.length - 1, 1)) * chartH;
    ctx.textAlign = 'left';
    ctx.fillText(formatPrice(asks[i].price), midX + 8, rowY + 3);
  }

  // Bottom summary
  ctx.font = '9px "JetBrains Mono", monospace';
  ctx.textAlign = 'left';
  ctx.fillStyle = COLORS.bidStroke;
  ctx.fillText(`Σ ${formatVolume(bidCum[bidCum.length - 1] ?? 0)}`, 8, logicalH - 6);
  ctx.textAlign = 'right';
  ctx.fillStyle = COLORS.askStroke;
  ctx.fillText(`Σ ${formatVolume(askCum[askCum.length - 1] ?? 0)}`, logicalW - 8, logicalH - 6);

  ctx.restore();
}

interface SideConfig {
  x: number;
  y: number;
  width: number;
  height: number;
  fillStyle: string;
  strokeStyle: string;
  direction: 'left' | 'right';
}

function drawSide(
  ctx: CanvasRenderingContext2D,
  levels: OrderBookLevel[],
  cumulative: number[],
  maxCum: number,
  config: SideConfig,
): void {
  if (levels.length === 0) return;

  const { x, y, width, height, fillStyle, strokeStyle, direction } = config;

  ctx.fillStyle = fillStyle;
  ctx.strokeStyle = strokeStyle;
  ctx.lineWidth = 1;
  ctx.beginPath();

  const levelCount = levels.length;
  const rowH = height / levelCount;

  if (direction === 'left') {
    ctx.moveTo(x + width, y);
    for (let i = 0; i < levelCount; i++) {
      const barW = (cumulative[i] / maxCum) * width;
      const rowTop = y + i * rowH;
      ctx.lineTo(x + width - barW, rowTop);
      ctx.lineTo(x + width - barW, rowTop + rowH);
    }
    ctx.lineTo(x + width, y + height);
  } else {
    ctx.moveTo(x, y);
    for (let i = 0; i < levelCount; i++) {
      const barW = (cumulative[i] / maxCum) * width;
      const rowTop = y + i * rowH;
      ctx.lineTo(x + barW, rowTop);
      ctx.lineTo(x + barW, rowTop + rowH);
    }
    ctx.lineTo(x, y + height);
  }

  ctx.closePath();
  ctx.fill();
  ctx.stroke();
}

const OrderBookHeatmapInner: React.FC<OrderBookHeatmapProps> = ({
  bids,
  asks,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const animFrameRef = useRef<number>(0);
  const lastDrawRef = useRef<number>(0);
  const bidsRef = useRef(bids);
  const asksRef = useRef(asks);

  bidsRef.current = bids;
  asksRef.current = asks;

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    drawOrderBook(ctx, canvas.width, canvas.height, bidsRef.current, asksRef.current, window.devicePixelRatio || 1);
  }, []);

  const animate = useCallback(() => {
    const now = performance.now();
    if (now - lastDrawRef.current >= FRAME_INTERVAL) {
      draw();
      lastDrawRef.current = now;
    }
    animFrameRef.current = requestAnimationFrame(animate);
  }, [draw]);

  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const handleResize = () => {
      const dpr = window.devicePixelRatio || 1;
      const rect = container.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      draw();
    };

    handleResize();
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(container);

    animFrameRef.current = requestAnimationFrame(animate);

    return () => {
      resizeObserver.disconnect();
      cancelAnimationFrame(animFrameRef.current);
    };
  }, [draw, animate]);

  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Order Book</span>
        <span className="text-xs text-text-muted font-mono">
          {bids.length + asks.length} levels
        </span>
      </div>
      <div ref={containerRef} className="flex-1 min-h-0">
        <canvas ref={canvasRef} className="block w-full h-full" />
      </div>
    </div>
  );
};

export const OrderBookHeatmap = React.memo(OrderBookHeatmapInner);
OrderBookHeatmap.displayName = 'OrderBookHeatmap';
