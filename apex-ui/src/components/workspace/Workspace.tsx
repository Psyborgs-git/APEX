import React, { useState } from 'react';
import { Watchlist } from '../trading/Watchlist';
import { OrderEntry } from '../trading/OrderEntry';
import { PositionsPanel } from '../trading/PositionsPanel';
import { CandleChart } from '../charts/CandleChart';
import { StrategyIDE } from '../strategy/StrategyIDE';
import { MLWorkbench } from '../ml/MLWorkbench';
import { HealthMonitor } from '../monitor/HealthMonitor';
import { useMarketStore } from '../../stores/marketStore';
import { useWorkspaceStore } from '../../stores/workspaceStore';

type CenterTab = 'chart' | 'strategy' | 'ml' | 'health';

export const Workspace: React.FC = () => {
  const watchlist = useMarketStore((s) => s.watchlist);
  const [selectedSymbol, setSelectedSymbol] = useState(watchlist[0] ?? 'RELIANCE.NS');
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [showLoadDialog, setShowLoadDialog] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [layoutName, setLayoutName] = useState('');
  const [saveConfirmation, setSaveConfirmation] = useState(false);
  const [loadConfirmation, setLoadConfirmation] = useState(false);
  const [centerTab, setCenterTab] = useState<CenterTab>('chart');

  const saveLayout = useWorkspaceStore((s) => s.saveLayout);
  const loadLayout = useWorkspaceStore((s) => s.loadLayout);
  const layouts = useWorkspaceStore((s) => s.layouts);

  const handleSaveLayout = () => {
    if (layoutName.trim()) {
      saveLayout(layoutName.trim(), { selectedSymbol, timestamp: Date.now() });
      setLayoutName('');
      setShowSaveDialog(false);
      setSaveConfirmation(true);
      setTimeout(() => setSaveConfirmation(false), 3000);
    }
  };

  const handleLoadLayout = (name: string) => {
    const layout = loadLayout(name);
    if (layout?.config?.selectedSymbol) {
      setSelectedSymbol(layout.config.selectedSymbol as string);
    }
    setShowLoadDialog(false);
    setLoadConfirmation(true);
    setTimeout(() => setLoadConfirmation(false), 3000);
  };

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-[var(--border-color)] bg-surface-1">
        <div className="flex items-center gap-2">
          <button
            onClick={() => setShowSaveDialog(true)}
            className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
            data-testid="workspace-save-layout"
          >
            Save Layout
          </button>
          <button
            onClick={() => setShowLoadDialog(true)}
            className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
            data-testid="workspace-load-layout"
          >
            Load Layout
          </button>
          <button
            onClick={() => setShowSettings(true)}
            className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
            data-testid="open-settings"
          >
            Settings
          </button>
        </div>
        {saveConfirmation && (
          <span className="text-xs text-green-500" data-testid="layout-save-confirmation">
            Layout saved successfully
          </span>
        )}
        {loadConfirmation && (
          <span className="text-xs text-green-500" data-testid="layout-load-confirmation">
            Layout loaded successfully
          </span>
        )}
      </div>

      {/* Save Layout Dialog */}
      {showSaveDialog && (
        <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-surface-1 border border-[var(--border-color)] rounded-lg p-4 w-96">
            <h3 className="text-sm font-medium mb-3">Save Workspace Layout</h3>
            <input
              type="text"
              value={layoutName}
              onChange={(e) => setLayoutName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSaveLayout()}
              placeholder="Layout name"
              className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded mb-3"
              data-testid="layout-name-input"
              autoFocus
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowSaveDialog(false)}
                className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
              >
                Cancel
              </button>
              <button
                onClick={handleSaveLayout}
                className="px-3 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded"
                data-testid="confirm-save-layout"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Load Layout Dialog */}
      {showLoadDialog && (
        <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-surface-1 border border-[var(--border-color)] rounded-lg p-4 w-96">
            <h3 className="text-sm font-medium mb-3">Load Workspace Layout</h3>
            <div className="space-y-2 mb-3 max-h-64 overflow-auto">
              {layouts.length === 0 ? (
                <p className="text-sm text-text-muted">No saved layouts</p>
              ) : (
                layouts.map((layout) => (
                  <button
                    key={layout.name}
                    onClick={() => handleLoadLayout(layout.name)}
                    className="w-full px-3 py-2 text-sm text-left bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
                    data-testid={`layout-item-${layout.name}`}
                  >
                    {layout.name}
                  </button>
                ))
              )}
            </div>
            <div className="flex justify-end">
              <button
                onClick={() => setShowLoadDialog(false)}
                className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Settings Panel */}
      {showSettings && (
        <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-surface-1 border border-[var(--border-color)] rounded-lg p-4 w-[600px]" data-testid="settings-panel">
            <h3 className="text-sm font-medium mb-4">Settings</h3>
            <div className="space-y-4">
              <div data-testid="trading-settings-section">
                <h4 className="text-xs font-medium text-text-secondary mb-2">Trading Settings</h4>
                <div className="space-y-2">
                  <label className="flex items-center justify-between text-sm">
                    <span>Default Order Size</span>
                    <input type="number" className="w-24 px-2 py-1 text-sm bg-surface-0 border border-[var(--border-color)] rounded" defaultValue={1} />
                  </label>
                  <label className="flex items-center justify-between text-sm">
                    <span>Enable Confirmations</span>
                    <input type="checkbox" defaultChecked />
                  </label>
                </div>
              </div>
            </div>
            <div className="flex justify-end mt-4">
              <button
                onClick={() => setShowSettings(false)}
                className="px-3 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Main Workspace Grid */}
      <div className="flex-1 grid grid-cols-12 gap-1 p-1 bg-surface-0" data-testid="workspace-panel">
        {/* Left: Watchlist */}
        <div className="col-span-3 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
          <Watchlist onSelectSymbol={setSelectedSymbol} selectedSymbol={selectedSymbol} />
        </div>

        {/* Center: Tab-switchable Chart/OrderEntry or StrategyIDE */}
        <div className="col-span-6 flex flex-col gap-1">
          {/* Center tab bar */}
          <div className="flex gap-1 bg-surface-1 rounded-lg border border-[var(--border-color)] px-2 py-1">
            <button
              onClick={() => setCenterTab('chart')}
              data-testid="tab-chart"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'chart' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              Chart
            </button>
            <button
              onClick={() => setCenterTab('strategy')}
              data-testid="tab-strategy"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'strategy' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              Strategy IDE
            </button>
            <button
              onClick={() => setCenterTab('ml')}
              data-testid="tab-ml"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'ml' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              ML Workbench
            </button>
            <button
              onClick={() => setCenterTab('health')}
              data-testid="tab-health"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'health' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              Health
            </button>
          </div>

          {centerTab === 'chart' ? (
            <>
              <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
                <CandleChart symbol={selectedSymbol} />
              </div>
              <div className="h-48 bg-surface-1 rounded-lg border border-[var(--border-color)]">
                <OrderEntry defaultSymbol={selectedSymbol} />
              </div>
            </>
          ) : centerTab === 'strategy' ? (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <StrategyIDE />
            </div>
          ) : centerTab === 'ml' ? (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <MLWorkbench />
            </div>
          ) : (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <HealthMonitor />
            </div>
          )}
        </div>

        {/* Right: Positions & Alerts */}
        <div className="col-span-3 flex flex-col gap-1">
          <div className="flex-1 bg-surface-1 rounded-lg overflow-hidden border border-[var(--border-color)]">
            <PositionsPanel />
          </div>
          <div className="h-40 bg-surface-1 rounded-lg border border-[var(--border-color)] p-3">
            <AlertConsole />
          </div>
        </div>
      </div>
    </div>
  );
};

const AlertConsole: React.FC = () => {
  const [alerts, setAlerts] = useState<Array<{ symbol: string; condition: string; value: string }>>([]);
  const [showCreateAlert, setShowCreateAlert] = useState(false);
  const [alertSymbol, setAlertSymbol] = useState('');
  const [alertCondition, setAlertCondition] = useState('price_above');
  const [alertValue, setAlertValue] = useState('');

  const handleCreateAlert = () => {
    if (alertSymbol && alertValue) {
      setAlerts([...alerts, { symbol: alertSymbol, condition: alertCondition, value: alertValue }]);
      setAlertSymbol('');
      setAlertValue('');
      setShowCreateAlert(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-text-secondary">Alerts</span>
        <button
          onClick={() => setShowCreateAlert(!showCreateAlert)}
          className="text-xs px-2 py-0.5 bg-primary-500 hover:bg-primary-600 text-white rounded"
          data-testid="create-alert-button"
        >
          +
        </button>
      </div>

      {showCreateAlert && (
        <div className="mb-2 p-2 border border-[var(--border-color)] rounded space-y-1">
          <input
            type="text"
            value={alertSymbol}
            onChange={(e) => setAlertSymbol(e.target.value)}
            placeholder="Symbol"
            className="w-full px-2 py-1 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid="alert-symbol-input"
          />
          <select
            value={alertCondition}
            onChange={(e) => setAlertCondition(e.target.value)}
            className="w-full px-2 py-1 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid="alert-condition-select"
          >
            <option value="price_above">Price Above</option>
            <option value="price_below">Price Below</option>
          </select>
          <input
            type="number"
            value={alertValue}
            onChange={(e) => setAlertValue(e.target.value)}
            placeholder="Value"
            className="w-full px-2 py-1 text-xs bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid="alert-value-input"
          />
          <button
            onClick={handleCreateAlert}
            className="w-full px-2 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded"
            data-testid="alert-save-button"
          >
            Save
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto text-xs font-mono text-text-muted">
        {alerts.length === 0 ? (
          <p>No alerts triggered.</p>
        ) : (
          alerts.map((alert, idx) => (
            <div key={idx} className="mb-1" data-testid="alert-item">
              {alert.symbol} {alert.condition} {alert.value}
            </div>
          ))
        )}
      </div>
    </div>
  );
};
