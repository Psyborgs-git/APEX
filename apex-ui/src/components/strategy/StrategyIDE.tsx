import React, { useState, useCallback, useRef, useEffect } from 'react';
import Editor, { type OnMount } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';

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
  initialFiles?: StrategyFile[];
  onRun?: (file: StrategyFile) => void;
  onStop?: () => void;
  onRestart?: () => void;
  onSave?: (file: StrategyFile) => void;
  metrics?: StrategyMetrics;
}

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
  const editorRef = useRef<EditorInstance | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  const activeFile = files[activeFileIndex];

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor;

    // Ctrl+S save
    editor.addCommand(
      // Monaco KeyMod.CtrlCmd | Monaco KeyCode.KeyS
      2048 | 49, // KeyMod.CtrlCmd | KeyCode.KeyS
      () => {
        const content = editor.getValue();
        if (activeFile) {
          const updated = { ...activeFile, content };
          onSave?.(updated);
        }
      },
    );
  }, [activeFile, onSave]);

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

  const handleRun = useCallback(() => {
    if (!activeFile) return;
    setRunState('running');
    appendOutput(`[${new Date().toLocaleTimeString()}] ▶ Running ${activeFile.name}...`);
    onRun?.(activeFile);
  }, [activeFile, onRun, appendOutput]);

  const handleStop = useCallback(() => {
    setRunState('stopped');
    appendOutput(`[${new Date().toLocaleTimeString()}] ■ Strategy stopped`);
    onStop?.();
  }, [onStop, appendOutput]);

  const handleRestart = useCallback(() => {
    setRunState('running');
    appendOutput(`[${new Date().toLocaleTimeString()}] ↻ Restarting strategy...`);
    onRestart?.();
  }, [onRestart, appendOutput]);

  const [newFileName, setNewFileName] = useState('');
  const [showNewFileInput, setShowNewFileInput] = useState(false);

  const handleNewFile = useCallback(() => {
    if (showNewFileInput) {
      // Create the file with the given name
      const name = newFileName || `strategy_${files.length + 1}.py`;
      const finalName = name.endsWith('.py') ? name : `${name}.py`;
      const newFile: StrategyFile = {
        name: finalName,
        path: `strategies/${finalName}`,
        content: DEFAULT_TEMPLATE,
      };
      setFiles((prev) => [...prev, newFile]);
      setActiveFileIndex(files.length);
      setNewFileName('');
      setShowNewFileInput(false);
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
            onClick={handleRun}
            disabled={runState === 'running'}
            data-testid="execute-strategy"
            className="px-3 py-1 text-xs font-mono rounded bg-bull/20 text-bull hover:bg-bull/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ▶ Run
          </button>
          <button
            type="button"
            onClick={handleStop}
            disabled={runState !== 'running'}
            className="px-3 py-1 text-xs font-mono rounded bg-bear/20 text-bear hover:bg-bear/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ■ Stop
          </button>
          <button
            type="button"
            onClick={handleRestart}
            disabled={runState !== 'running'}
            className="px-3 py-1 text-xs font-mono rounded bg-accent/20 text-accent hover:bg-accent/30 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            ↻ Restart
          </button>
          <button
            type="button"
            onClick={() => {
              if (activeFile) {
                const content = editorRef.current?.getValue() ?? activeFile.content;
                onSave?.({ ...activeFile, content });
                setSaveConfirm(true);
                setTimeout(() => setSaveConfirm(false), 2000);
              }
            }}
            data-testid="save-strategy"
            className="px-3 py-1 text-xs font-mono rounded bg-surface-2 text-text-secondary hover:text-text-primary transition-colors"
          >
            💾 Save
          </button>
        </div>
      </div>

      {saveConfirm && (
        <div className="px-3 py-1 text-xs text-bull bg-bull/10" data-testid="save-confirmation">File saved</div>
      )}

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
                onKeyDown={(e) => { if (e.key === 'Enter') handleNewFile(); }}
              />
              <button
                type="button"
                onClick={handleNewFile}
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
