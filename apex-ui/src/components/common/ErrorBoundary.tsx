import React from 'react';

interface Props {
  children: React.ReactNode;
  label?: string;
}

interface State {
  error: string | null;
}

// One crashed panel must never take the terminal down with it.
export class ErrorBoundary extends React.Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(err: unknown): State {
    return { error: err instanceof Error ? err.message : String(err) };
  }

  componentDidCatch(err: unknown, info: React.ErrorInfo) {
    console.error(`[apex] panel crash (${this.props.label ?? 'panel'})`, err, info.componentStack);
  }

  private reset = () => this.setState({ error: null });

  render() {
    if (this.state.error !== null) {
      return (
        <div className="flex flex-col items-center justify-center h-full gap-2 px-4 text-center">
          <span className="text-xs font-mono text-bear uppercase tracking-wider">
            {this.props.label ?? 'Panel'} error
          </span>
          <span className="text-xs text-text-muted font-mono max-w-md break-words">
            {this.state.error}
          </span>
          <button
            type="button"
            onClick={this.reset}
            className="px-2 py-1 text-xs font-mono border border-[var(--border-color)] rounded text-text-secondary hover:text-text-primary hover:bg-surface-2"
          >
            RETRY
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
