/**
 * Tauri Event Bridge Hooks
 *
 * These hooks bridge Tauri's real-time event system with React state.
 * When running in the browser (no Tauri), they fall back to polling.
 */
import { useEffect, useRef, useCallback } from 'react';
import { useMarketStore } from '../stores/marketStore';
import { useOrderStore } from '../stores/orderStore';
import { useRiskStore } from '../stores/riskStore';
import { useHealthStore } from '../stores/healthStore';
import type { QuoteDto, OrderDto, PositionDto, RiskStatusDto, SystemHealthDto } from '../lib/types';

const IS_TAURI = typeof window !== 'undefined' && '__TAURI__' in window;

type UnlistenFn = () => void;

/**
 * Subscribe to real-time quote updates from Tauri events.
 * Falls back to no-op in browser mode (useDataSync handles polling).
 */
export function useQuoteStream() {
  const updateQuote = useMarketStore((s) => s.updateQuote);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!IS_TAURI) return;

    let mounted = true;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<QuoteDto>('quote-update', (event) => {
          if (mounted && event.payload?.symbol) {
            updateQuote(event.payload);
          }
        });
        unlistenRef.current = unlisten;
      } catch {
        // Tauri event API not available
      }
    })();

    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, [updateQuote]);
}

/**
 * Subscribe to real-time order update events from Tauri.
 */
export function useOrderStream() {
  const setOrders = useOrderStore((s) => s.setOrders);
  const ordersRef = useRef<OrderDto[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!IS_TAURI) return;

    let mounted = true;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<OrderDto>('order-update', (event) => {
          if (!mounted || !event.payload?.id) return;
          const idx = ordersRef.current.findIndex((o) => o.id === event.payload.id);
          if (idx >= 0) {
            ordersRef.current[idx] = event.payload;
          } else {
            ordersRef.current.push(event.payload);
          }
          setOrders([...ordersRef.current]);
        });
        unlistenRef.current = unlisten;
      } catch {
        // Tauri event API not available
      }
    })();

    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, [setOrders]);
}

/**
 * Subscribe to position update events from Tauri.
 */
export function usePositionStream() {
  const setPositions = useOrderStore((s) => s.setPositions);
  const positionsRef = useRef<PositionDto[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!IS_TAURI) return;

    let mounted = true;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<PositionDto>('position-update', (event) => {
          if (!mounted || !event.payload?.symbol) return;
          const idx = positionsRef.current.findIndex((p) => p.symbol === event.payload.symbol);
          if (idx >= 0) {
            positionsRef.current[idx] = event.payload;
          } else {
            positionsRef.current.push(event.payload);
          }
          setPositions([...positionsRef.current]);
        });
        unlistenRef.current = unlisten;
      } catch {
        // Tauri event API not available
      }
    })();

    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, [setPositions]);
}

/**
 * Subscribe to adapter health events from Tauri.
 */
export function useHealthStream() {
  const setHealth = useHealthStore((s) => s.setHealth);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!IS_TAURI) return;

    let mounted = true;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<SystemHealthDto>('adapter-health', (event) => {
          if (mounted && event.payload) {
            setHealth(event.payload);
          }
        });
        unlistenRef.current = unlisten;
      } catch {
        // Tauri event API not available
      }
    })();

    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, [setHealth]);
}

/**
 * Master hook that sets up all Tauri event streams.
 * Call once from the app root to initialize all real-time subscriptions.
 */
export function useTauriEventBridge() {
  useQuoteStream();
  useOrderStream();
  usePositionStream();
  useHealthStream();
}
