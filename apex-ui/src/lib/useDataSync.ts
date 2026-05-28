import { useEffect, useRef } from 'react';
import { useMarketStore } from '../stores/marketStore';
import { useOrderStore } from '../stores/orderStore';
import { useRiskStore } from '../stores/riskStore';
import { useBrokerStore } from '../stores/brokerStore';
import { getQuote, subscribeSymbols, getPositions, getOpenOrders, getRiskStatus, getAccountBalance, listBrokerConnections } from './tauri';

const POLL_INTERVAL_MS = 3000;

/**
 * Hook that initializes data subscriptions and polling on mount.
 * Subscribes to market data for the watchlist, then polls for
 * quotes, positions, orders, and risk status.
 */
export function useDataSync() {
  const watchlist = useMarketStore((s) => s.watchlist);
  const updateQuote = useMarketStore((s) => s.updateQuote);
  const setPositions = useOrderStore((s) => s.setPositions);
  const setOrders = useOrderStore((s) => s.setOrders);
  const setAccountBalance = useOrderStore((s) => s.setAccountBalance);
  const clearAccountBalance = useOrderStore((s) => s.clearAccountBalance);
  const setRiskStatus = useRiskStore((s) => s.setStatus);
  const setBrokerConnections = useBrokerStore((s) => s.setConnections);
  const activeBrokerId = useBrokerStore((s) => s.activeBrokerId);
  const subscribedRef = useRef(false);

  // Subscribe to market data on first mount
  useEffect(() => {
    if (subscribedRef.current) return;
    subscribedRef.current = true;
    subscribeSymbols(watchlist).catch((err) =>
      console.warn('[DataSync] subscribe failed:', err)
    );
  }, [watchlist]);

  // Poll quotes
  useEffect(() => {
    const poll = async () => {
      for (const symbol of watchlist) {
        try {
          const quote = await getQuote(symbol);
          if (quote && quote.symbol) {
            updateQuote(quote);
          }
        } catch {
          // Quote not yet available — skip
        }
      }
    };

    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [watchlist, updateQuote]);

  // Poll broker readiness and account balance for the active execution broker.
  useEffect(() => {
    const poll = async () => {
      try {
        const connections = await listBrokerConnections();
        if (Array.isArray(connections)) {
          setBrokerConnections(connections);
        }
      } catch {
        // Broker commands not yet available — skip.
      }
    };

    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [setBrokerConnections]);

  // Poll positions, orders, risk
  useEffect(() => {
    const poll = async () => {
      try {
        const [positions, orders, risk, balance] = await Promise.all([
          getPositions(),
          getOpenOrders(),
          getRiskStatus(),
          getAccountBalance(activeBrokerId).catch(() => null),
        ]);
        if (Array.isArray(positions)) setPositions(positions);
        if (Array.isArray(orders)) setOrders(orders);
        if (risk && typeof risk.session_pnl === 'number') setRiskStatus(risk);
        if (balance) {
          setAccountBalance(balance);
        } else {
          clearAccountBalance();
        }
      } catch {
        // Not connected to backend yet — skip
      }
    };

    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [activeBrokerId, clearAccountBalance, setAccountBalance, setPositions, setOrders, setRiskStatus]);
}
