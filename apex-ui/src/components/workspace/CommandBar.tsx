import React, { useState, useCallback, useRef, useEffect } from 'react';

interface ParsedCommand {
  type: 'SYMBOL_DEFAULT' | 'SYMBOL_PANEL' | 'SYSTEM_PANEL' | 'ORDER' | 'UNKNOWN';
  symbol?: string;
  panel?: string;
  side?: string;
  quantity?: number;
  price?: number;
  orderType?: string;
}

function parseCommand(input: string): ParsedCommand {
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

export const CommandBar: React.FC = () => {
  const [input, setInput] = useState('');
  const [isActive, setIsActive] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    const cmd = parseCommand(input);
    console.log('Command:', cmd);
    setInput('');
    setIsActive(false);
  }, [input]);

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
      <form onSubmit={handleSubmit} className="flex-1 max-w-xl">
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onFocus={() => setIsActive(true)}
          onBlur={() => setIsActive(false)}
          placeholder="Type symbol, command, or order (Space to activate)"
          className="w-full bg-surface-2 text-text-primary font-mono text-sm px-3 py-1.5 rounded-md border border-[var(--border-color)] focus:border-accent focus:outline-none placeholder:text-text-muted"
        />
      </form>
      <div className="flex items-center gap-2 text-xs text-text-muted font-mono">
        <span>Paper Trading</span>
        <span className="w-2 h-2 rounded-full bg-bull"></span>
      </div>
    </div>
  );
};
