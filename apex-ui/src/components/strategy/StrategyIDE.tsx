import React, { useState, useCallback, useRef, useEffect } from 'react';
import Editor, { type OnMount } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import {
  createStrategyFile,
  listStrategyFiles,
  runStrategyBacktest,
  runStrategyFile,
  saveStrategyFile,
} from '../../lib/tauri';
import type { StrategyBacktestRequestDto, StrategyBacktestResultDto, StrategyExecutionResultDto } from '../../lib/types';

type EditorInstance = editor.IStandaloneCodeEditor;

interface StrategyFile {
  name: string;
  path: string;
  content: string;
}

type RunState = 'idle' | 'running' | 'stopped' | 'error';

interface StrategyMetrics {
  totalSignals: number;
  avgLatencyMs: number;
  errorCount: number;
  uptime: string;
}

interface StrategyIDEProps {
  defaultSymbol?: string;
  initialFiles?: StrategyFile[];
  onRun?: (file: StrategyFile) => void;
  onStop?: () => void;
  onRestart?: () => void;
  onSave?: (file: StrategyFile) => void;
  metrics?: StrategyMetrics;
}

const formatDateInput = (date: Date) => {
  const adjusted = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return adjusted.toISOString().slice(0, 10);
};

const formatNumber = (value: number) => new Intl.NumberFormat('en-IN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
}).format(value);

const formatPercent = (value: number) => `${value.toFixed(2)}%`;

const createDefaultBacktestConfig = (symbol?: string): Omit<StrategyBacktestRequestDto, 'path'> => {
  const to = new Date();
  const from = new Date();
  from.setMonth(from.getMonth() - 6);

  return {
    symbol: symbol ?? 'RELIANCE.NS',
    timeframe: '1d',
    from: formatDateInput(from),
    to: formatDateInput(to),
    initial_capital: 100_000,
    commission_bps: 3,
    slippage_bps: 2,
    quantity: 10,
  };
};

const DEFAULT_TEMPLATE = `"""
APEX Strategy Template
Subclass Strategy and override on_bar / on_tick.
"""
from apex_sdk import Strategy, Bar, Signal, Timeframe


class MyStrategy(Strategy):
    def on_init(self, params: dict) -> None:
        self.subscribe(["RELIANCE.NS"], Timeframe.M5)
        self.log("Strategy initialized")

    def on_bar(self, symbol: str, bar: Bar) -> None:
        sma = self.indicator("sma", symbol, 20)
        if bar.close > sma:
            self.emit(Signal(
                symbol=symbol,
                direction="long",
                strength=0.8,
                metadata={"reason": "price_above_sma"},
            ))

    def on_stop(self) -> None:
        self.log("Strategy stopped")
`;

const DEFAULT_FILES: StrategyFile[] = [
  {
    name: 'my_strategy.py',
    path: 'strategies/my_strategy.py',
    content: DEFAULT_TEMPLATE,
  },
];

const STATE_COLORS: Record<RunState, string> = {
  idle: 'bg-surface-3',
  running: 'bg-bull',
  stopped: 'bg-muted',
  error: 'bg-bear',
};

const STATE_LABELS: Record<RunState, string> = {
  idle: 'Idle',
  running: 'Running',
  stopped: 'Stopped',
  error: 'Error',
};

const StrategyIDEInner: React.FC<StrategyIDEProps> = ({
  defaultSymbol,
  initialFiles,
  onRun,
  onStop,
  onRestart,
  onSave,
  metrics,
}) => {
  const [files, setFiles] = useState<StrategyFile[]>(initialFiles ?? DEFAULT_FILES);
  const [activeFileIndex, setActiveFileIndex] = useState(0);
  const [runState, setRunState] = useState<RunState>('idle');
  const [output, setOutput] = useState<string[]>([]);
  const [saveConfirm, setSaveConfirm] = useState(false);
  const [isLoadingFiles, setIsLoadingFiles] = useState(!initialFiles);
  const [strategyError, setStrategyError] = useState<string | null>(null);
  const [isBacktesting, setIsBacktesting] = useState(false);
  const [backtestConfig, setBacktestConfig] = useState<Omit<StrategyBacktestRequestDto, 'path'>>(
    () => createDefaultBacktestConfig(defaultSymbol),
  );
  const [backtestResult, setBacktestResult] = useState<StrategyBacktestResultDto | null>(null);
  const editorRef = useRef<EditorInstance | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  const activeFile = files[activeFileIndex];

  const replaceFileInState = useCallback((file: StrategyFile) => {
    setFiles((prev) => {
      const index = prev.findIndex((existing) => existing.path === file.path);
      if (index === -1) {
        return [...prev, file];
      }

      const next = [...prev];
      next[index] = file;
      return next;
    });
  }, []);

  const currentEditorFile = useCallback(() => {
    if (!activeFile) return null;

    const content = editorRef.current?.getValue() ?? activeFile.content;
    const updated = { ...activeFile, content };
    replaceFileInState(updated);
    return updated;
  }, [activeFile, replaceFileInState]);

  const persistFile = useCallback(async (file: StrategyFile, showConfirmation = false) => {
    const saved = await saveStrategyFile(file.path, file.content);
    replaceFileInState(saved);
    onSave?.(saved);
    if (showConfirmation) {
      setSaveConfirm(true);
      setTimeout(() => setSaveConfirm(false), 2000);
    }
    return saved;
  }, [onSave, replaceFileInState]);

  useEffect(() => {
    if (!defaultSymbol) {
      return;
    }

    setBacktestConfig((prev) => ({ ...prev, symbol: defaultSymbol }));
  }, [defaultSymbol]);

  useEffect(() => {
    if (initialFiles) {
      setFiles(initialFiles);
      setActiveFileIndex(0);
      setIsLoadingFiles(false);
      return;
    }

    let cancelled = false;

    (async () => {
      setIsLoadingFiles(true);
      try {
        const loadedFiles = await listStrategyFiles();
        if (cancelled) return;

        if (loadedFiles.length > 0) {
          setFiles(loadedFiles);
          setActiveFileIndex((current) => Math.min(current, loadedFiles.length - 1));
        } else {
          setFiles(DEFAULT_FILES);
          setActiveFileIndex(0);
        }
        setStrategyError(null);
      } catch (err) {
        if (cancelled) return;
        setFiles(DEFAULT_FILES);
        setActiveFileIndex(0);
        setStrategyError(err instanceof Error ? err.message : 'Failed to load strategy files');
      } finally {
        if (!cancelled) {
          setIsLoadingFiles(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [initialFiles]);

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor;

    // Ctrl+S save
    editor.addCommand(
      // Monaco KeyMod.CtrlCmd | Monaco KeyCode.KeyS
      2048 | 49, // KeyMod.CtrlCmd | KeyCode.KeyS
      () => {
        const updated = currentEditorFile();
        if (updated) {
          void persistFile(updated, true).catch((err) => {
            setStrategyError(err instanceof Error ? err.message : 'Failed to save strategy');
          });
        }
      },
    );
  }, [currentEditorFile, persistFile]);

  const handleEditorChange = useCallback(
    (value: string | undefined) => {
      if (value === undefined) return;
      setFiles((prev) => {
        const next = [...prev];
        const current = next[activeFileIndex];
        if (current) {
          next[activeFileIndex] = { ...current, content: value };
        }
        return next;
      });
    },
    [activeFileIndex],
  );

  const appendOutput = useCallback((line: string) => {
    setOutput((prev) => [...prev.slice(-500), line]);
  }, []);

  const appendResult = useCallback((result: StrategyExecutionResultDto) => {
    result.output.forEach((line) => appendOutput(line));
    if (result.error) {
      appendOutput(`Error: ${result.error}`);
    }
  }, [appendOutput]);

  const handleRun = useCallback(async () => {
    const fileToRun = currentEditorFile();
    if (!fileToRun) return;

    setRunState('running');
    setStrategyError(null);
    appendOutput(`[${new Date().toLocaleTimeString()}] ▶ Running ${fileToRun.name}...`);

    try {
      const savedFile = await persistFile(fileToRun);
      const result = await runStrategyFile(savedFile.path);
      appendResult(result);
      onRun?.(savedFile);
      setRunState(result.success ? 'stopped' : 'error');
      if (!result.success) {
        setStrategyError(result.error ?? 'Strategy execution failed');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Strategy execution failed';
      setStrategyError(message);
      appendOutput(`Error: ${message}`);
      setRunState('error');
    }
  }, [appendOutput, appendResult, currentEditorFile, onRun, persistFile]);

  const handleStop = useCallback(async () => {
    setRunState('stopped');
    appendOutput(`[${new Date().toLocaleTimeString()}] ■ Strategy stopped`);
    onStop?.();
  }, [onStop, appendOutput]);

  const handleRestart = useCallback(async () => {
    appendOutput(`[${new Date().toLocaleTimeString()}] ↻ Restarting strategy...`);
    onRestart?.();
    await handleRun();
  }, [appendOutput, handleRun, onRestart]);

  const handleBacktest = useCallback(async () => {
    const fileToBacktest = currentEditorFile();
    if (!fileToBacktest) return;

    setIsBacktesting(true);
    setStrategyError(null);
    appendOutput(
      `[${new Date().toLocaleTimeString()}] ⧗ Backtesting ${fileToBacktest.name} on ${backtestConfig.symbol}...`,
    );

    try {
      const savedFile = await persistFile(fileToBacktest);
      const result = await runStrategyBacktest({
        path: savedFile.path,
        ...backtestConfig,
      });
      setBacktestResult(result);
      appendOutput(
        `[${new Date().toLocaleTimeString()}] ✓ Backtest complete: ${formatPercent(result.metrics.total_return_pct)} total return over ${result.bars_analyzed} bars`,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Strategy backtest failed';
      setStrategyError(message);
      appendOutput(`Backtest error: ${message}`);
    } finally {
      setIsBacktesting(false);
    }
  }, [appendOutput, backtestConfig, currentEditorFile, persistFile]);

  const [newFileName, setNewFileName] = useState('');
  const [showNewFileInput, setShowNewFileInput] = useState(false);

  const handleNewFile = useCallback(async () => {
    if (showNewFileInput) {
      // Create the file with the given name
      const name = newFileName || `strategy_${files.length + 1}.py`;
      const finalName = name.endsWith('.py') ? name : `${name}.py`;
      try {
        const newFile = await createStrategyFile(`strategies/${finalName}`, DEFAULT_TEMPLATE);
        setFiles((prev) => [...prev, newFile]);
        setActiveFileIndex(files.length);
        setNewFileName('');
        setShowNewFileInput(false);
        setStrategyError(null);
      } catch (err) {
        setStrategyError(err instanceof Error ? err.message : 'Failed to create strategy file');
      }
    } else {
      setShowNewFileInput(true);
    }
  }, [files.length, showNewFileInput, newFileName]);

  // Auto-scroll output
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [output]);

  return (
    <div className="flex flex-col h-full bg-surface-0" data-testid="strategy-ide">
      {/* Toolbar */}
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between bg-surface-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-text-secondary">Strategy IDE</span>
          <div className="flex items-center gap-1 ml-2">
            <span className={`w-2 h-2 rounded-full ${STATE_COLORS[runState]}`} data-testid="pipeline-status" data-status={runState} />
            <span className="text-xs text-text-muted font-mono">{STATE_LABELS[runState]}</span>
          </div>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => void handleRun()}
            disabled={runState === 'running'}
            data-testid="execute-strategy"
            className="px-3 py-1 text-xs font-mono rounded bg-bull/20 text-bull hover:bg-bull/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ▶ Run
          </button>
          <button
            type="button"
            onClick={() => void handleStop()}
            disabled={runState !== 'running'}
            className="px-3 py-1 text-xs font-mono rounded bg-bear/20 text-bear hover:bg-bear/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ■ Stop
          </button>
          <button
            type="button"
            onClick={() => void handleRestart()}
            disabled={runState === 'running' && isLoadingFiles}
            className="px-3 py-1 text-xs font-mono rounded bg-accent/20 text-accent hover:bg-accent/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ↻ Restart
          </button>
          <button
            type="button"
            onClick={() => {
              const updated = currentEditorFile();
              if (!updated) return;

              void persistFile(updated, true).catch((err) => {
                setStrategyError(err instanceof Error ? err.message : 'Failed to save strategy');
              });
            }}
            data-testid="save-strategy"
            className="px-3 py-1 text-xs font-mono rounded bg-surface-2 text-text-secondary hover:text-text-primary transition-colors"
          >
            💾 Save
          </button>
        </div>
      </div>

      {isLoadingFiles && (
        <div className="px-3 py-1 text-xs text-text-muted bg-surface-1/80 border-b border-[var(--border-color)]">
          Loading strategy files…
        </div>
      )}

      {strategyError && (
        <div className="px-3 py-1 text-xs text-bear bg-bear/10 border-b border-[var(--border-color)]" data-testid="strategy-error-banner">
          {strategyError}
        </div>
      )}

      {saveConfirm && (
        <div className="px-3 py-1 text-xs text-bull bg-bull/10" data-testid="save-confirmation">File saved</div>
      )}

      <div
        className="border-b border-[var(--border-color)] bg-surface-1/70 px-3 py-3 space-y-3"
        data-testid="strategy-backtest-panel"
      >
        <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
          <div>
            <div className="text-xs font-medium uppercase tracking-wider text-text-muted">Strategy Backtest</div>
            <div className="text-xs text-text-secondary">
              Run the active strategy file against cached or live historical market data.
            </div>
          </div>
          <button
            type="button"
            onClick={() => void handleBacktest()}
            disabled={isBacktesting || isLoadingFiles || !activeFile}
            data-testid="run-backtest"
            className="px-3 py-1 text-xs font-mono rounded bg-accent/20 text-accent hover:bg-accent/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isBacktesting ? 'Running…' : 'Run Backtest'}
          </button>
        </div>

        <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Symbol</span>
            <input
              type="text"
              value={backtestConfig.symbol}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, symbol: e.target.value.toUpperCase() }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-symbol-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Timeframe</span>
            <select
              value={backtestConfig.timeframe}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, timeframe: e.target.value }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-timeframe-select"
            >
              <option value="1d">1D</option>
              <option value="4h">4H</option>
              <option value="1h">1H</option>
              <option value="15m">15M</option>
              <option value="5m">5M</option>
              <option value="1m">1M</option>
            </select>
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">From</span>
            <input
              type="date"
              value={backtestConfig.from}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, from: e.target.value }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-from-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">To</span>
            <input
              type="date"
              value={backtestConfig.to}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, to: e.target.value }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-to-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Initial Capital</span>
            <input
              type="number"
              min={1}
              step="1000"
              value={backtestConfig.initial_capital ?? 100_000}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, initial_capital: Number(e.target.value) }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-capital-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Trade Size</span>
            <input
              type="number"
              min={1}
              step="1"
              value={backtestConfig.quantity ?? 10}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, quantity: Number(e.target.value) }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-quantity-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Commission (bps)</span>
            <input
              type="number"
              min={0}
              step="0.1"
              value={backtestConfig.commission_bps ?? 3}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, commission_bps: Number(e.target.value) }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-commission-input"
            />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="mb-1 block">Slippage (bps)</span>
            <input
              type="number"
              min={0}
              step="0.1"
              value={backtestConfig.slippage_bps ?? 2}
              onChange={(e) => setBacktestConfig((prev) => ({ ...prev, slippage_bps: Number(e.target.value) }))}
              className="w-full rounded border border-[var(--border-color)] bg-surface-0 px-2 py-1 font-mono text-xs text-text-primary"
              data-testid="backtest-slippage-input"
            />
          </label>
        </div>

        {backtestResult && (
          <div className="space-y-3 rounded border border-[var(--border-color)] bg-surface-0 p-3" data-testid="backtest-results">
            <div className="flex flex-col gap-1 lg:flex-row lg:items-center lg:justify-between">
              <div>
                <div className="text-sm font-medium text-text-primary">{backtestResult.strategy_name}</div>
                <div className="text-xs text-text-muted">
                  {backtestResult.inferred_strategy} · {backtestResult.symbol} · {backtestResult.timeframe.toUpperCase()} · {backtestResult.bars_analyzed} bars
                </div>
              </div>
              <div className="text-xs text-text-muted" data-testid="backtest-trade-count">
                {backtestResult.metrics.total_trades} closed trades
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2 xl:grid-cols-5">
              <MetricCard label="Total Return" value={formatPercent(backtestResult.metrics.total_return_pct)} testId="backtest-total-return" />
              <MetricCard label="Final Equity" value={formatNumber(backtestResult.metrics.final_equity)} testId="backtest-final-equity" />
              <MetricCard label="Win Rate" value={formatPercent(backtestResult.metrics.win_rate)} />
              <MetricCard label="Sharpe" value={backtestResult.metrics.sharpe_ratio.toFixed(2)} />
              <MetricCard label="Max DD" value={formatPercent(backtestResult.metrics.max_drawdown_pct)} />
            </div>

            <div className="grid gap-3 xl:grid-cols-2">
              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wider text-text-muted">Backtest Notes</div>
                <ul className="space-y-1 text-xs text-text-secondary" data-testid="backtest-notes">
                  {backtestResult.notes.map((note) => (
                    <li key={note} className="rounded bg-surface-1 px-2 py-1">{note}</li>
                  ))}
                </ul>
              </div>
              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wider text-text-muted">Recent Trades</div>
                <div className="max-h-32 overflow-auto rounded border border-[var(--border-color)]">
                  {backtestResult.trades.length === 0 ? (
                    <div className="px-2 py-2 text-xs text-text-muted">No closed trades were generated for this run.</div>
                  ) : (
                    backtestResult.trades.slice(-5).reverse().map((trade, index) => (
                      <div key={`${trade.entry_time}-${trade.exit_time ?? 'open'}-${index}`} className="grid grid-cols-4 gap-2 border-b border-[var(--border-color)] px-2 py-1 text-xs last:border-b-0">
                        <span className="font-mono text-text-primary">{trade.symbol}</span>
                        <span className="text-text-secondary">{trade.side}</span>
                        <span className={trade.pnl >= 0 ? 'text-bull' : 'text-bear'}>{formatNumber(trade.pnl)}</span>
                        <span className="text-text-muted">{new Date(trade.entry_time).toLocaleDateString()}</span>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      <div className="flex flex-1 min-h-0">
        {/* File browser sidebar */}
        <div className="w-48 border-r border-[var(--border-color)] bg-surface-1 flex flex-col">
          <div className="px-2 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
            <span className="text-xs text-text-muted uppercase tracking-wider">Files</span>
            <button
              type="button"
              onClick={() => setShowNewFileInput(true)}
              data-testid="strategy-new-file"
              className="text-xs text-accent hover:text-text-primary transition-colors"
              title="New file"
            >
              +
            </button>
          </div>
          {showNewFileInput && (
            <div className="px-2 py-1 flex gap-1 border-b border-[var(--border-color)]">
              <input
                type="text"
                value={newFileName}
                onChange={(e) => setNewFileName(e.target.value)}
                placeholder="filename.py"
                data-testid="file-name-input"
                className="flex-1 bg-surface-2 text-text-primary font-mono text-xs px-1 py-0.5 rounded border border-[var(--border-color)] focus:border-accent focus:outline-none"
                autoFocus
                onKeyDown={(e) => { if (e.key === 'Enter') void handleNewFile(); }}
              />
              <button
                type="button"
                onClick={() => void handleNewFile()}
                data-testid="confirm-create-file"
                className="text-xs text-bull"
              >
                ✓
              </button>
            </div>
          )}
          <div className="flex-1 overflow-auto py-1" data-testid="strategy-file-list">
            {files.map((file, idx) => (
              <button
                key={file.path}
                type="button"
                onClick={() => setActiveFileIndex(idx)}
                data-testid={`file-${file.name}`}
                className={`w-full text-left px-2 py-1 text-xs font-mono truncate transition-colors strategy-file-item ${
                  idx === activeFileIndex
                    ? 'bg-surface-2 text-text-primary'
                    : 'text-text-secondary hover:bg-surface-2/50'
                }`}
              >
                🐍 {file.name}
              </button>
            ))}
          </div>

          {/* Live metrics */}
          {metrics && (
            <div className="border-t border-[var(--border-color)] px-2 py-2 space-y-1">
              <span className="text-xs text-text-muted uppercase tracking-wider">Metrics</span>
              <div className="text-xs font-mono text-text-secondary space-y-0.5">
                <div className="flex justify-between">
                  <span>Signals</span>
                  <span className="text-text-primary">{metrics.totalSignals}</span>
                </div>
                <div className="flex justify-between">
                  <span>Latency</span>
                  <span className="text-text-primary">{metrics.avgLatencyMs.toFixed(1)}ms</span>
                </div>
                <div className="flex justify-between">
                  <span>Errors</span>
                  <span className={metrics.errorCount > 0 ? 'text-bear' : 'text-text-primary'}>
                    {metrics.errorCount}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Uptime</span>
                  <span className="text-text-primary">{metrics.uptime}</span>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Editor + Output */}
        <div className="flex-1 flex flex-col min-w-0">
          {/* File tabs */}
          <div className="flex border-b border-[var(--border-color)] bg-surface-1">
            {files.map((file, idx) => (
              <button
                key={file.path}
                type="button"
                onClick={() => setActiveFileIndex(idx)}
                className={`px-3 py-1 text-xs font-mono border-r border-[var(--border-color)] transition-colors ${
                  idx === activeFileIndex
                    ? 'bg-surface-0 text-text-primary border-b-2 border-b-accent'
                    : 'bg-surface-1 text-text-muted hover:text-text-secondary'
                }`}
              >
                {file.name}
              </button>
            ))}
          </div>

          {/* Monaco Editor */}
          <div className="flex-1 min-h-0" data-testid="strategy-editor">
            <Editor
              height="100%"
              language="python"
              theme="vs-dark"
              value={activeFile?.content ?? ''}
              onChange={handleEditorChange}
              onMount={handleEditorMount}
              options={{
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                fontSize: 13,
                lineHeight: 20,
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                padding: { top: 8 },
                renderLineHighlight: 'line',
                cursorBlinking: 'smooth',
                smoothScrolling: true,
                bracketPairColorization: { enabled: true },
                automaticLayout: true,
                tabSize: 4,
              }}
            />
          </div>

          {/* Output panel */}
          <div className="h-32 border-t border-[var(--border-color)] bg-surface-1 flex flex-col" data-testid="strategy-output">
            <div className="px-3 py-1 border-b border-[var(--border-color)] flex items-center justify-between">
              <span className="text-xs text-text-muted uppercase tracking-wider">Output</span>
              <button
                type="button"
                onClick={() => setOutput([])}
                className="text-xs text-text-muted hover:text-text-secondary transition-colors"
              >
                Clear
              </button>
            </div>
            <div ref={outputRef} className="flex-1 overflow-auto px-3 py-1 font-mono text-xs">
              {output.length === 0 ? (
                <span className="text-text-muted">Strategy output will appear here...</span>
              ) : (
                output.map((line, i) => (
                  <div
                    key={`${i}-${line.slice(0, 20)}`}
                    className={`py-0.5 ${
                      line.includes('Error') || line.includes('error')
                        ? 'text-bear strategy-error'
                        : line.includes('▶')
                          ? 'text-bull'
                          : 'text-text-secondary'
                    }`}
                    data-testid={line.includes('Error') || line.includes('error') ? 'strategy-error' : undefined}
                  >
                    {line}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export const StrategyIDE = React.memo(StrategyIDEInner);
StrategyIDE.displayName = 'StrategyIDE';

export type { StrategyFile, StrategyMetrics, RunState };

interface MetricCardProps {
  label: string;
  value: string;
  testId?: string;
}

const MetricCard: React.FC<MetricCardProps> = ({ label, value, testId }) => (
  <div className="rounded border border-[var(--border-color)] bg-surface-1 px-3 py-2">
    <div className="text-[11px] uppercase tracking-wider text-text-muted">{label}</div>
    <div className="mt-1 text-sm font-mono text-text-primary" data-testid={testId}>{value}</div>
  </div>
);
