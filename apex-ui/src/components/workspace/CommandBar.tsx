import React, { useState, useCallback, useRef, useEffect } from 'react';
import { placeOrder } from '../../lib/tauri';
import type { NewOrderRequestDto } from '../../lib/types';

export interface ParsedCommand {
  type: 'SYMBOL_DEFAULT' | 'SYMBOL_PANEL' | 'SYSTEM_PANEL' | 'ORDER' | 'UNKNOWN';
  symbol?: string;
  panel?: string;
  side?: string;
  quantity?: number;
  price?: number;
  orderType?: string;
}

export function parseCommand(input: string): ParsedCommand {
  const trimmed = input.trim().toUpperCase();

  // ":ORDERS" → system panel
  if (trimmed.startsWith(':')) {
    return { type: 'SYSTEM_PANEL', panel: trimmed.slice(1) };
  }

  // "BUY RELIANCE 10" or "SELL HDFCBANK 5 LIMIT 1600"
  const orderMatch = trimmed.match(/^(BUY|SELL)\s+(\S+)\s+(\d+(?:\.\d+)?)(?:\s+(LIMIT|STOP)\s+(\d+(?:\.\d+)?))?$/);
  if (orderMatch) {
    return {
      type: 'ORDER',
      side: orderMatch[1],
      symbol: orderMatch[2],
      quantity: parseFloat(orderMatch[3]),
      orderType: orderMatch[4] || 'MARKET',
      price: orderMatch[5] ? parseFloat(orderMatch[5]) : undefined,
    };
  }

  // "RELIANCE:CHART" → symbol + panel
  if (trimmed.includes(':')) {
    const [symbol, panel] = trimmed.split(':');
    return { type: 'SYMBOL_PANEL', symbol, panel };
  }

  // "RELIANCE" → symbol default (open chart)
  if (trimmed.length > 0) {
    return { type: 'SYMBOL_DEFAULT', symbol: trimmed };
  }

  return { type: 'UNKNOWN' };
}

interface CommandBarProps {
  onSelectSymbol?: (symbol: string) => void;
  onSwitchTab?: (tab: string) => void;
}

export const CommandBar: React.FC<CommandBarProps> = ({ onSelectSymbol, onSwitchTab }) => {
  const [input, setInput] = useState('');
  const [isActive, setIsActive] = useState(false);
  const [feedback, setFeedback] = useState<{ message: string; type: 'success' | 'error' } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const showFeedback = useCallback((message: string, type: 'success' | 'error') => {
    setFeedback({ message, type });
    setTimeout(() => setFeedback(null), 3000);
  }, []);

  const executeCommand = useCallback(async (cmd: ParsedCommand) => {
    switch (cmd.type) {
      case 'ORDER': {
        if (!cmd.symbol || !cmd.side || !cmd.quantity) {
          showFeedback('Invalid order: missing symbol, side, or quantity', 'error');
          return;
        }
        const request: NewOrderRequestDto = {
          symbol: cmd.symbol,
          side: cmd.side === 'BUY' ? 'Buy' : 'Sell',
          order_type: cmd.orderType === 'LIMIT' ? 'Limit' : cmd.orderType === 'STOP' ? 'Stop' : 'Market',
          quantity: cmd.quantity,
          price: cmd.price ?? null,
          stop_price: null,
          broker_id: 'paper',
          tag: 'command-bar',
        };
        try {
          const orderId = await placeOrder(request);
          showFeedback(`Order placed: ${cmd.side} ${cmd.quantity} ${cmd.symbol} → ${orderId}`, 'success');
        } catch (err) {
          showFeedback(`Order failed: ${err instanceof Error ? err.message : 'Unknown error'}`, 'error');
        }
        break;
      }
      case 'SYMBOL_DEFAULT': {
        if (cmd.symbol && onSelectSymbol) {
          onSelectSymbol(cmd.symbol);
          onSwitchTab?.('chart');
          showFeedback(`Viewing ${cmd.symbol}`, 'success');
        }
        break;
      }
      case 'SYMBOL_PANEL': {
        if (cmd.symbol && onSelectSymbol) {
          onSelectSymbol(cmd.symbol);
        }
        if (cmd.panel) {
          const panelMap: Record<string, string> = {
            CHART: 'chart',
            STRATEGY: 'strategy',
            ML: 'ml',
            HEALTH: 'health',
          };
          const tab = panelMap[cmd.panel];
          if (tab) onSwitchTab?.(tab);
        }
        break;
      }
      case 'SYSTEM_PANEL': {
        if (cmd.panel) {
          const panelMap: Record<string, string> = {
            CHART: 'chart',
            STRATEGY: 'strategy',
            ML: 'ml',
            HEALTH: 'health',
            ORDERS: 'chart',
            POSITIONS: 'chart',
          };
          const tab = panelMap[cmd.panel];
          if (tab) {
            onSwitchTab?.(tab);
            showFeedback(`Switched to ${cmd.panel}`, 'success');
          }
        }
        break;
      }
      default:
        break;
    }
  }, [onSelectSymbol, onSwitchTab, showFeedback]);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;
    const cmd = parseCommand(input);
    executeCommand(cmd);
    setInput('');
    setIsActive(false);
  }, [input, executeCommand]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === ' ' && !isActive && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault();
      setIsActive(true);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
    if (e.key === 'Escape') {
      setIsActive(false);
      setInput('');
    }
  }, [isActive]);

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div className="h-10 bg-surface-1 border-b border-[var(--border-color)] flex items-center px-4 gap-3">
      <span className="text-accent font-mono font-semibold text-sm">APEX</span>
      <form onSubmit={handleSubmit} className="flex-1 max-w-xl" data-testid="command-bar-form">
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onFocus={() => setIsActive(true)}
          onBlur={() => setTimeout(() => setIsActive(false), 200)}
          placeholder="Type symbol, command, or order (Space to activate)"
          className="w-full bg-surface-2 text-text-primary font-mono text-sm px-3 py-1.5 rounded-md border border-[var(--border-color)] focus:border-accent focus:outline-none placeholder:text-text-muted"
          data-testid="command-bar-input"
        />
      </form>
      {feedback && (
        <span
          className={`text-xs font-mono ${feedback.type === 'success' ? 'text-bull' : 'text-bear'}`}
          data-testid="command-bar-feedback"
        >
          {feedback.message}
        </span>
      )}
      <div className="flex items-center gap-2 text-xs text-text-muted font-mono">
        <span>Paper Trading</span>
        <span className="w-2 h-2 rounded-full bg-bull"></span>
      </div>
    </div>
  );
};
