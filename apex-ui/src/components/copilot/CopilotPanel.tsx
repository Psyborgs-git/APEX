import React, { useCallback, useEffect, useRef, useState } from 'react';
import { copilotChat } from '../../lib/tauri';
import type { CopilotMessageDto } from '../../lib/types';

interface ChatEntry extends CopilotMessageDto {
  id: number;
  model?: string;
  error?: boolean;
}

const SUGGESTIONS = [
  'Summarize my open positions and session P&L',
  'Which watchlist symbols are moving most today?',
  'Explain what the correlation graph edges mean',
];

export const CopilotPanel: React.FC = () => {
  const [entries, setEntries] = useState<ChatEntry[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const nextId = useRef(1);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [entries, busy]);

  const send = useCallback(async (text?: string) => {
    const message = (text ?? input).trim();
    if (!message || busy) return;

    const userEntry: ChatEntry = { id: nextId.current++, role: 'user', content: message };
    const history: CopilotMessageDto[] = [...entries, userEntry]
      .filter((e) => !e.error)
      .map((e) => ({ role: e.role, content: e.content }));

    setEntries((current) => [...current, userEntry]);
    setInput('');
    setBusy(true);
    try {
      const reply = await copilotChat(message, history);
      setEntries((current) => [
        ...current,
        { id: nextId.current++, role: 'assistant', content: reply.reply, model: reply.model },
      ]);
    } catch (err) {
      setEntries((current) => [
        ...current,
        {
          id: nextId.current++,
          role: 'assistant',
          content: typeof err === 'string' ? err : err instanceof Error ? err.message : 'Copilot request failed',
          error: true,
        },
      ]);
    } finally {
      setBusy(false);
    }
  }, [busy, entries, input]);

  return (
    <div className="flex flex-col h-full" data-testid="copilot-panel">
      <div className="px-3 py-2 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Copilot</span>
        <span className="text-[10px] text-text-muted font-mono">OpenRouter · live terminal context</span>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-auto px-3 py-2 space-y-3">
        {entries.length === 0 ? (
          <div className="space-y-2" data-testid="copilot-empty">
            <p className="text-xs text-text-muted">
              Ask about positions, quotes, scans, or anything market-related. The copilot sees
              your live watchlist, positions, and session P&amp;L.
            </p>
            {SUGGESTIONS.map((s) => (
              <button
                key={s}
                onClick={() => void send(s)}
                className="block w-full text-left px-2 py-1.5 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded text-text-secondary"
                data-testid="copilot-suggestion"
              >
                {s}
              </button>
            ))}
          </div>
        ) : (
          entries.map((entry) => (
            <div
              key={entry.id}
              className={entry.role === 'user' ? 'flex justify-end' : 'flex justify-start'}
              data-testid={entry.role === 'user' ? 'copilot-user-msg' : 'copilot-assistant-msg'}
            >
              <div
                className={`max-w-[85%] rounded px-3 py-2 text-sm whitespace-pre-wrap ${
                  entry.role === 'user'
                    ? 'bg-primary-500/20 text-text-primary'
                    : entry.error
                      ? 'bg-bear/10 text-bear border border-bear/30'
                      : 'bg-surface-2 text-text-primary'
                }`}
              >
                {entry.content}
                {entry.model && entry.model !== 'mock' && (
                  <div className="mt-1 text-[10px] text-text-muted font-mono">{entry.model}</div>
                )}
              </div>
            </div>
          ))
        )}
        {busy && (
          <div className="flex justify-start">
            <div className="bg-surface-2 rounded px-3 py-2 text-sm text-text-muted">Thinking…</div>
          </div>
        )}
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
        className="px-3 py-2 border-t border-[var(--border-color)] flex items-center gap-2"
      >
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Ask the copilot…"
          className="flex-1 min-w-0 px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded focus:border-accent focus:outline-none"
          data-testid="copilot-input"
        />
        <button
          type="submit"
          disabled={busy || !input.trim()}
          className="px-3 py-2 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded disabled:opacity-50"
          data-testid="copilot-send"
        >
          Send
        </button>
      </form>
    </div>
  );
};
