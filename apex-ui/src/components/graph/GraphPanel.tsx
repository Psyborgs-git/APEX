import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { computeCorrelations, getGraph } from '../../lib/tauri';
import type { GraphDto, GraphNodeDto } from '../../lib/types';
import { useMarketStore } from '../../stores/marketStore';
import { VectorGraph, type GraphEdge, type GraphNode } from '../charts/VectorGraph';

const NODE_TYPE_MAP: Record<string, GraphNode['type']> = {
  Instrument: 'symbol',
  Sector: 'indicator',
  MacroVariable: 'model',
  NewsEvent: 'signal',
  CustomVariable: 'strategy',
};

function toViewModel(dto: GraphDto): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = dto.nodes.map((n: GraphNodeDto) => ({
    id: n.id,
    label: n.label,
    type: NODE_TYPE_MAP[n.node_type] ?? 'symbol',
  }));
  const edges: GraphEdge[] = dto.edges.map((e) => {
    const et = e.data.edge_type as { CorrelatedWith?: { coefficient?: number; window?: string } } | null;
    const corr = et && typeof et === 'object' && 'CorrelatedWith' in et ? et.CorrelatedWith : undefined;
    const w = Number.isFinite(e.data.weight) ? e.data.weight : 0.1;
    const coeff = corr?.coefficient;
    return {
      source: e.source,
      target: e.target,
      weight: Math.max(0.1, Math.min(1, w)),
      label: coeff !== undefined && Number.isFinite(coeff) ? `ρ ${coeff.toFixed(2)}` : undefined,
    };
  });
  return { nodes, edges };
}

export const GraphPanel: React.FC = () => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const [graph, setGraph] = useState<GraphDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [windowDays, setWindowDays] = useState(90);

  const refresh = useCallback(async () => {
    try {
      setGraph(await getGraph());
      setError(null);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Unable to load graph');
    }
  }, []);

  const compute = useCallback(async () => {
    setBusy(true);
    try {
      setGraph(await computeCorrelations(watchlist.slice(0, 40), windowDays));
      setError(null);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Correlation compute failed');
    } finally {
      setBusy(false);
    }
  }, [watchlist, windowDays]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const viewModel = useMemo(
    () => (graph ? toViewModel(graph) : { nodes: [], edges: [] }),
    [graph],
  );

  return (
    <div className="flex flex-col h-full" data-testid="graph-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center gap-2">
        <span className="text-sm font-medium text-text-secondary">Correlation Graph</span>
        <select
          value={windowDays}
          onChange={(e) => setWindowDays(Number(e.target.value))}
          className="px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
          data-testid="graph-window-select"
        >
          <option value={30}>30d</option>
          <option value={90}>90d</option>
          <option value={180}>180d</option>
          <option value={365}>1y</option>
        </select>
        <button
          onClick={() => void compute()}
          disabled={busy}
          className="px-2 py-0.5 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded disabled:opacity-50"
          data-testid="graph-compute"
        >
          {busy ? 'Computing…' : 'Compute from watchlist'}
        </button>
        <span className="ml-auto text-xs text-text-muted font-mono">
          {viewModel.nodes.length} nodes · {viewModel.edges.length} edges
        </span>
      </div>
      <div className="flex-1 min-h-0">
        {error ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm px-4 text-center">
            {error}
          </div>
        ) : viewModel.nodes.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm px-4 text-center">
            Empty graph — press “Compute from watchlist” to build the correlation network
          </div>
        ) : (
          <VectorGraph nodes={viewModel.nodes} edges={viewModel.edges} />
        )}
      </div>
    </div>
  );
};
