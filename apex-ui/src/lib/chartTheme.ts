import { useEffect, useState } from 'react';

/** Chart palette resolved from the active CSS theme tokens. */
export interface ChartTheme {
  background: string;
  surface: string;
  text: string;
  textMuted: string;
  grid: string;
  border: string;
  bull: string;
  bear: string;
  accent: string;
  warning: string;
}

const FALLBACK: ChartTheme = {
  background: '#0a0a0f',
  surface: '#12121a',
  text: '#a0a0b8',
  textMuted: '#5c5c7a',
  grid: '#1a1a25',
  border: '#2a2a3a',
  bull: '#00c853',
  bear: '#ff1744',
  accent: '#448aff',
  warning: '#ffab00',
};

function cssVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** Read the current theme's CSS variables into concrete colors for canvas/lwc. */
export function chartTheme(): ChartTheme {
  if (typeof document === 'undefined') return FALLBACK;
  return {
    background: cssVar('--surface-0', FALLBACK.background),
    surface: cssVar('--surface-1', FALLBACK.surface),
    text: cssVar('--text-secondary', FALLBACK.text),
    textMuted: cssVar('--text-muted', FALLBACK.textMuted),
    grid: cssVar('--surface-2', FALLBACK.grid),
    border: cssVar('--border-color', FALLBACK.border),
    bull: cssVar('--color-bull', FALLBACK.bull),
    bear: cssVar('--color-bear', FALLBACK.bear),
    accent: cssVar('--color-accent', FALLBACK.accent),
    warning: cssVar('--color-warning', FALLBACK.warning),
  };
}

/** `#rrggbb` + alpha → `#rrggbbaa`. Non-hex input returns as-is. */
export function withAlpha(color: string, alphaPct: number): string {
  const a = Math.round(Math.max(0, Math.min(100, alphaPct)) * 2.55)
    .toString(16)
    .padStart(2, '0');
  return /^#[0-9a-f]{6}$/i.test(color) ? `${color}${a}` : color;
}

/**
 * Bumping tick that changes whenever `data-theme`/`data-density` flips on
 * <html>. Add it to a chart-creation effect's deps to repaint on theme change.
 */
export function useThemeTick(): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const obs = new MutationObserver(() => setTick((t) => t + 1));
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'data-density'],
    });
    return () => obs.disconnect();
  }, []);
  return tick;
}
