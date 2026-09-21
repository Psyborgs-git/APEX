import React, { useCallback, useEffect, useState } from 'react';
import { getNews, searchNews } from '../../lib/tauri';
import { useNewsStore } from '../../stores/newsStore';
import { useMarketStore } from '../../stores/marketStore';

const POLL_MS = 30_000;

function sentimentBadge(sentiment: number | null) {
  if (sentiment === null || sentiment === undefined) {
    return null;
  }
  const tone = sentiment > 0.15 ? 'text-bull border-bull/30' : sentiment < -0.15 ? 'text-bear border-bear/30' : 'text-text-muted border-[var(--border-color)]';
  return (
    <span className={`rounded border px-1.5 py-0.5 text-[10px] font-mono ${tone}`}>
      {sentiment > 0 ? '+' : ''}{sentiment.toFixed(2)}
    </span>
  );
}

export const NewsPanel: React.FC = () => {
  const items = useNewsStore((s) => s.items);
  const setItems = useNewsStore((s) => s.setItems);
  const firedAlerts = useNewsStore((s) => s.firedAlerts);
  const clearFiredAlerts = useNewsStore((s) => s.clearFiredAlerts);
  const watchlist = useMarketStore((s) => s.watchlist);

  const [query, setQuery] = useState('');
  const [symbolFilter, setSymbolFilter] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const symbol = symbolFilter.trim() || undefined;
      const next = query.trim()
        ? await searchNews(query.trim(), symbol, 100)
        : await getNews(100, symbol);
      setItems(next);
      setError(null);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Unable to load news');
    } finally {
      setIsLoading(false);
    }
  }, [query, setItems, symbolFilter]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const mergeItems = items;

  return (
    <div className="flex flex-col h-full" data-testid="news-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center gap-2">
        <span className="text-sm font-medium text-text-secondary">News</span>
        <select
          value={symbolFilter}
          onChange={(e) => setSymbolFilter(e.target.value)}
          className="px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
          data-testid="news-symbol-filter"
        >
          <option value="">All symbols</option>
          {watchlist.map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
        </select>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search headlines…"
          className="flex-1 min-w-0 px-2 py-0.5 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
          data-testid="news-search-input"
        />
        <span className="text-xs text-text-muted font-mono">{mergeItems.length}</span>
      </div>

      {firedAlerts.length > 0 && (
        <div className="px-3 py-1.5 border-b border-[var(--border-color)] bg-surface-2 flex items-center gap-2" data-testid="news-fired-alerts">
          <span className="text-[10px] uppercase tracking-wide text-warning">Fired alerts</span>
          <div className="flex-1 overflow-hidden text-xs text-text-secondary truncate">
            {firedAlerts.slice(0, 3).map((a) => a.message).join(' · ')}
            {firedAlerts.length > 3 ? ` +${firedAlerts.length - 3} more` : ''}
          </div>
          <button
            onClick={clearFiredAlerts}
            className="text-[10px] text-text-muted hover:text-text-primary"
            data-testid="clear-fired-alerts"
          >
            Clear
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto">
        {error ? (
          <p className="px-3 py-2 text-xs text-bear">{error}</p>
        ) : isLoading && mergeItems.length === 0 ? (
          <p className="px-3 py-2 text-xs text-text-muted">Loading news…</p>
        ) : mergeItems.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-muted text-sm px-4 text-center">
            No news yet — feeds poll on the configured interval
          </div>
        ) : (
          mergeItems.map((item) => (
            <article key={item.id} className="px-3 py-2 border-b border-[var(--border-color)] hover:bg-surface-2" data-testid="news-item">
              <div className="flex items-start justify-between gap-2">
                <a
                  href={/^https?:\/\//i.test(item.url ?? '') ? item.url : undefined}
                  target="_blank"
                  rel="noreferrer"
                  className="text-sm text-text-primary leading-snug hover:text-accent"
                >
                  {item.headline}
                </a>
                {sentimentBadge(item.sentiment)}
              </div>
              <div className="mt-1 flex items-center gap-2 text-[10px] text-text-muted font-mono">
                <span>{item.source}</span>
                <span>{new Date(item.published).toLocaleString()}</span>
                {item.symbols.length > 0 && (
                  <span className="text-accent">{item.symbols.join(', ')}</span>
                )}
              </div>
            </article>
          ))
        )}
      </div>
    </div>
  );
};
