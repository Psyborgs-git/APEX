import React, { useCallback, useEffect, useState } from 'react';
import { Watchlist } from '../trading/Watchlist';
import { DataBrowser } from '../trading/DataBrowser';
import { OrderEntry } from '../trading/OrderEntry';
import { PositionsPanel } from '../trading/PositionsPanel';
import { CandleChart } from '../charts/CandleChart';
import { StrategyIDE } from '../strategy/StrategyIDE';
import { MLWorkbench } from '../ml/MLWorkbench';
import { HealthMonitor } from '../monitor/HealthMonitor';
import { NotebookEditor } from './NotebookEditor';
import { addAlert, clearBrokerSession, getAlertRules, getAppSettings, listBrokerConnections, removeAlert, saveAppSettings, setBrokerSession, subscribeSymbols } from '../../lib/tauri';
import type { AlertRuleDto, AppSettingsDto, AppSettingsUpdateDto, BrokerConnectionDto } from '../../lib/types';
import { useMarketStore } from '../../stores/marketStore';
import { useBrokerStore } from '../../stores/brokerStore';
import { useWorkspaceStore } from '../../stores/workspaceStore';
import { VALID_TABS, type CenterTab } from './workspaceTabs';

type AlertCondition = 'price_above' | 'price_below';

const SETTINGS_LABELS: Record<string, string> = {
  paper: 'Paper Trading',
  yahoo_finance: 'Yahoo Finance',
  zerodha: 'Zerodha Kite',
  angel_one: 'Angel One',
  groww: 'Groww',
  robinhood: 'Robinhood',
  sqlite: 'SQLite',
  timescale: 'Timescale / Postgres',
};

function buildAlertRulePayload(symbol: string, condition: AlertCondition, value: string) {
  const threshold = Number(value);

  switch (condition) {
    case 'price_below':
      return {
        PriceBelow: {
          symbol,
          threshold,
        },
      };
    case 'price_above':
    default:
      return {
        PriceAbove: {
          symbol,
          threshold,
        },
      };
  }
}

function formatAlertRule(ruleJson: string) {
  try {
    const parsed = JSON.parse(ruleJson) as Record<string, any>;

    if (parsed.PriceAbove) {
      return `${parsed.PriceAbove.symbol} price above ${parsed.PriceAbove.threshold}`;
    }
    if (parsed.PriceBelow) {
      return `${parsed.PriceBelow.symbol} price below ${parsed.PriceBelow.threshold}`;
    }
    if (parsed.VwapCross) {
      return `${parsed.VwapCross.symbol} crossed VWAP`;
    }
    if (parsed.DailyPnl) {
      return `Daily P&L below ${parsed.DailyPnl.threshold}`;
    }
  } catch {
    // Fall back to raw JSON if the payload is malformed.
  }

  return ruleJson;
}

function toSettingsDraft(settings: AppSettingsDto): AppSettingsUpdateDto {
  return {
    general: {
      data_dir: settings.general.data_dir,
    },
    market_data: {
      adapter: settings.market_data.adapter,
    },
    execution: {
      adapter: settings.execution.adapter,
    },
    risk: {
      max_daily_loss: settings.risk.max_daily_loss,
      max_order_value: settings.risk.max_order_value,
    },
    storage: {
      backend: settings.storage.backend,
      sqlite_path: settings.storage.sqlite_path,
      postgres_url: settings.storage.postgres_url,
      wal_mode: settings.storage.wal_mode,
      pool_size: settings.storage.pool_size,
    },
  };
}

function formatSettingsLabel(value: string) {
  return SETTINGS_LABELS[value] ?? value.replace(/_/g, ' ').replace(/\b\w/g, (char) => char.toUpperCase());
}

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
  const [brokerTokens, setBrokerTokens] = useState<Record<string, string>>({});
  const [brokerActionError, setBrokerActionError] = useState<string | null>(null);
  const [brokerBusyId, setBrokerBusyId] = useState<string | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettingsDto | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<AppSettingsUpdateDto | null>(null);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsSuccess, setSettingsSuccess] = useState<string | null>(null);

  const saveLayout = useWorkspaceStore((s) => s.saveLayout);
  const loadLayout = useWorkspaceStore((s) => s.loadLayout);
  const layouts = useWorkspaceStore((s) => s.layouts);
  const brokerConnections = useBrokerStore((s) => s.connections);
  const setBrokerConnections = useBrokerStore((s) => s.setConnections);
  const activeBrokerId = useBrokerStore((s) => s.activeBrokerId);
  const setActiveBrokerId = useBrokerStore((s) => s.setActiveBrokerId);

  // React to CommandBar dispatches
  const commandSymbol = useWorkspaceStore((s) => s.commandSymbol);
  const commandTab = useWorkspaceStore((s) => s.commandTab);
  const setCommandSymbol = useWorkspaceStore((s) => s.setCommandSymbol);
  const setCommandTab = useWorkspaceStore((s) => s.setCommandTab);

  useEffect(() => {
    if (commandSymbol) {
      setSelectedSymbol(commandSymbol);
      setCommandSymbol(null);
    }
  }, [commandSymbol, setCommandSymbol]);

  useEffect(() => {
    if (commandTab && (VALID_TABS as readonly string[]).includes(commandTab)) {
      setCenterTab(commandTab as CenterTab);
      setCommandTab(null);
    }
  }, [commandTab, setCommandTab]);

  const refreshBrokerConnections = useCallback(async () => {
    try {
      const nextConnections = await listBrokerConnections();
      setBrokerConnections(nextConnections);
      setBrokerActionError(null);
    } catch (err) {
      setBrokerActionError(err instanceof Error ? err.message : 'Unable to load broker connections');
    }
  }, [setBrokerConnections]);

  const refreshAppSettings = useCallback(async () => {
    try {
      const nextSettings = await getAppSettings();
      setAppSettings(nextSettings);
      setSettingsDraft(toSettingsDraft(nextSettings));
      setSettingsError(null);
    } catch (err) {
      setSettingsError(err instanceof Error ? err.message : 'Unable to load application settings');
    }
  }, []);

  useEffect(() => {
    if (showSettings) {
      void refreshBrokerConnections();
      void refreshAppSettings();
      setSettingsSuccess(null);
    }
  }, [refreshAppSettings, refreshBrokerConnections, showSettings]);

  const handleBrokerTokenChange = useCallback((brokerId: string, token: string) => {
    setBrokerTokens((current) => ({
      ...current,
      [brokerId]: token,
    }));
  }, []);

  const handleConnectBroker = useCallback(async (brokerId: string) => {
    const token = brokerTokens[brokerId]?.trim();

    if (!token) {
      setBrokerActionError('Enter a live session token before connecting a broker.');
      return;
    }

    setBrokerBusyId(brokerId);
    try {
      await setBrokerSession(brokerId, token);
      await refreshBrokerConnections();
      await subscribeSymbols(watchlist);
      setBrokerTokens((current) => ({ ...current, [brokerId]: '' }));
      setBrokerActionError(null);
    } catch (err) {
      setBrokerActionError(err instanceof Error ? err.message : 'Unable to connect broker');
    } finally {
      setBrokerBusyId(null);
    }
  }, [brokerTokens, refreshBrokerConnections, watchlist]);

  const handleDisconnectBroker = useCallback(async (brokerId: string) => {
    setBrokerBusyId(brokerId);
    try {
      await clearBrokerSession(brokerId);
      await refreshBrokerConnections();
      setBrokerActionError(null);
    } catch (err) {
      setBrokerActionError(err instanceof Error ? err.message : 'Unable to disconnect broker');
    } finally {
      setBrokerBusyId(null);
    }
  }, [refreshBrokerConnections]);

  const handleSaveSettings = useCallback(async () => {
    if (!settingsDraft) {
      return;
    }

    setSettingsBusy(true);
    try {
      const nextSettings = await saveAppSettings(settingsDraft);
      setAppSettings(nextSettings);
      setSettingsDraft(toSettingsDraft(nextSettings));
      setSettingsError(null);
      setSettingsSuccess('Settings saved. Restart the desktop app to apply storage and adapter changes.');
    } catch (err) {
      setSettingsError(err instanceof Error ? err.message : 'Unable to save application settings');
      setSettingsSuccess(null);
    } finally {
      setSettingsBusy(false);
    }
  }, [settingsDraft]);

  const executionBrokers = brokerConnections.filter((broker) => broker.mode === 'paper' || broker.execution_available);

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
          <div className="bg-surface-1 border border-[var(--border-color)] rounded-lg p-4 w-[820px] max-h-[85vh] overflow-y-auto" data-testid="settings-panel">
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

              <div data-testid="broker-settings-section">
                <div className="mb-2 flex items-center justify-between">
                  <h4 className="text-xs font-medium text-text-secondary">Broker Connections</h4>
                  <button
                    type="button"
                    onClick={() => void refreshBrokerConnections()}
                    className="px-2 py-1 text-[11px] bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
                    data-testid="refresh-broker-connections"
                  >
                    Refresh
                  </button>
                </div>

                <label className="flex items-center justify-between gap-4 text-sm">
                  <span>Active Execution Broker</span>
                  <select
                    value={activeBrokerId}
                    onChange={(e) => setActiveBrokerId(e.target.value)}
                    className="min-w-[220px] px-2 py-1 text-sm bg-surface-0 border border-[var(--border-color)] rounded"
                    data-testid="active-broker-select"
                  >
                    {(executionBrokers.length > 0 ? executionBrokers : [{
                      broker_id: 'paper',
                      display_name: 'Paper Trading',
                      mode: 'paper',
                    } as Pick<BrokerConnectionDto, 'broker_id' | 'display_name' | 'mode'>]).map((broker) => (
                      <option key={broker.broker_id} value={broker.broker_id}>
                        {broker.mode === 'paper' ? 'Paper (Simulated)' : broker.display_name}
                      </option>
                    ))}
                  </select>
                </label>

                {brokerActionError && (
                  <p className="mt-2 text-xs text-bear" data-testid="broker-action-error">
                    {brokerActionError}
                  </p>
                )}

                <div className="mt-3 space-y-3">
                  {brokerConnections.map((broker) => {
                    const statusTone = broker.status === 'connected' || broker.status === 'ready'
                      ? 'border-bull/30 text-bull'
                      : broker.status === 'auth_required' || broker.status === 'not_configured'
                        ? 'border-warning/30 text-warning'
                        : 'border-bear/30 text-bear';

                    return (
                      <div
                        key={broker.broker_id}
                        className="rounded border border-[var(--border-color)] bg-surface-0 p-3"
                        data-testid={`broker-card-${broker.broker_id}`}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div>
                            <div className="flex items-center gap-2">
                              <span className="text-sm font-medium text-text-primary">{broker.display_name}</span>
                              <span className="rounded bg-surface-2 px-2 py-0.5 text-[10px] uppercase tracking-wide text-text-muted">
                                {broker.mode}
                              </span>
                              {broker.execution_available && (
                                <span className="rounded bg-surface-2 px-2 py-0.5 text-[10px] text-text-muted">Execution</span>
                              )}
                              {broker.market_data_available && (
                                <span className="rounded bg-surface-2 px-2 py-0.5 text-[10px] text-text-muted">Market Data</span>
                              )}
                            </div>
                            <p className="mt-2 text-xs text-text-muted">{broker.message}</p>
                          </div>

                          <span className={`rounded border px-2 py-1 text-[10px] uppercase tracking-wide ${statusTone}`}>
                            {broker.status.replace('_', ' ')}
                          </span>
                        </div>

                        {broker.mode === 'live' && broker.configured ? (
                          <div className="mt-3 flex items-center gap-2">
                            <input
                              type="password"
                              value={brokerTokens[broker.broker_id] ?? ''}
                              onChange={(e) => handleBrokerTokenChange(broker.broker_id, e.target.value)}
                              placeholder={broker.token_field_label || 'Session Token'}
                              className="min-w-0 flex-1 px-3 py-2 text-xs bg-surface-1 border border-[var(--border-color)] rounded"
                              data-testid={`broker-token-${broker.broker_id}`}
                            />
                            <button
                              type="button"
                              onClick={() => void handleConnectBroker(broker.broker_id)}
                              disabled={brokerBusyId === broker.broker_id || !(brokerTokens[broker.broker_id] ?? '').trim()}
                              className="px-3 py-2 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded disabled:opacity-50"
                              data-testid={`broker-connect-${broker.broker_id}`}
                            >
                              {brokerBusyId === broker.broker_id ? 'Working…' : 'Connect'}
                            </button>
                            <button
                              type="button"
                              onClick={() => void handleDisconnectBroker(broker.broker_id)}
                              disabled={brokerBusyId === broker.broker_id || !broker.authenticated}
                              className="px-3 py-2 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded disabled:opacity-50"
                              data-testid={`broker-disconnect-${broker.broker_id}`}
                            >
                              Disconnect
                            </button>
                          </div>
                        ) : broker.mode === 'live' ? (
                          <p className="mt-3 text-xs text-text-muted">
                            Fill the required `.env` values and restart the desktop app to enable this broker.
                          </p>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              </div>

              <div data-testid="runtime-settings-section">
                <div className="mb-2 flex items-start justify-between gap-4">
                  <div>
                    <h4 className="text-xs font-medium text-text-secondary">Runtime & Storage</h4>
                    <p className="mt-1 text-xs text-text-muted">
                      These values are saved to <code>{appSettings?.config_path ?? 'config/apex.toml'}</code> and apply after restart.
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => void refreshAppSettings()}
                    className="px-2 py-1 text-[11px] bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded"
                    data-testid="refresh-app-settings"
                  >
                    Refresh
                  </button>
                </div>

                <div className="rounded border border-[var(--border-color)] bg-surface-0 p-3 space-y-2">
                  <div className="flex flex-wrap items-center gap-3 text-xs text-text-muted">
                    <span>
                      Runtime storage: <span className="text-text-primary font-medium">{appSettings ? formatSettingsLabel(appSettings.runtime_storage_backend) : '--'}</span>
                    </span>
                    <span className="truncate">
                      Target: <span className="text-text-primary font-mono">{appSettings?.runtime_storage_target ?? '--'}</span>
                    </span>
                  </div>

                  {settingsError && (
                    <p className="text-xs text-bear" data-testid="settings-error">
                      {settingsError}
                    </p>
                  )}

                  {settingsSuccess && (
                    <p className="text-xs text-bull" data-testid="settings-success">
                      {settingsSuccess}
                    </p>
                  )}

                  {settingsDraft ? (
                    <div className="space-y-3 pt-1">
                      <div className="grid grid-cols-2 gap-3">
                        <label className="flex flex-col gap-1 text-sm">
                          <span>Data Directory</span>
                          <input
                            type="text"
                            value={settingsDraft.general.data_dir}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                general: { ...current.general, data_dir: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-data-dir"
                          />
                        </label>

                        <label className="flex flex-col gap-1 text-sm">
                          <span>Storage Backend</span>
                          <select
                            value={settingsDraft.storage.backend}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                storage: { ...current.storage, backend: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-storage-backend"
                          >
                            {(appSettings?.storage.available_backends ?? ['sqlite', 'timescale']).map((backend) => (
                              <option key={backend} value={backend}>
                                {formatSettingsLabel(backend)}
                              </option>
                            ))}
                          </select>
                        </label>
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <label className="flex flex-col gap-1 text-sm">
                          <span>Preferred Market Data Adapter</span>
                          <select
                            value={settingsDraft.market_data.adapter}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                market_data: { adapter: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-market-data-adapter"
                          >
                            {(appSettings?.market_data.available_adapters ?? []).map((adapter) => (
                              <option key={adapter} value={adapter}>
                                {formatSettingsLabel(adapter)}
                              </option>
                            ))}
                          </select>
                        </label>

                        <label className="flex flex-col gap-1 text-sm">
                          <span>Preferred Execution Adapter</span>
                          <select
                            value={settingsDraft.execution.adapter}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                execution: { adapter: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-execution-adapter"
                          >
                            {(appSettings?.execution.available_adapters ?? []).map((adapter) => (
                              <option key={adapter} value={adapter}>
                                {formatSettingsLabel(adapter)}
                              </option>
                            ))}
                          </select>
                        </label>
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <label className="flex flex-col gap-1 text-sm">
                          <span>SQLite Path</span>
                          <input
                            type="text"
                            value={settingsDraft.storage.sqlite_path}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                storage: { ...current.storage, sqlite_path: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-sqlite-path"
                          />
                        </label>

                        <label className="flex flex-col gap-1 text-sm">
                          <span>Postgres / Timescale URL</span>
                          <input
                            type="password"
                            value={settingsDraft.storage.postgres_url}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                storage: { ...current.storage, postgres_url: e.target.value },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            placeholder="postgresql://postgres:postgres@localhost:5432/apex_market"
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-postgres-url"
                          />
                        </label>
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <label className="flex flex-col gap-1 text-sm">
                          <span>Max Daily Loss</span>
                          <input
                            type="number"
                            min={1}
                            value={settingsDraft.risk.max_daily_loss}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                risk: { ...current.risk, max_daily_loss: Number(e.target.value) || 0 },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-max-daily-loss"
                          />
                        </label>

                        <label className="flex flex-col gap-1 text-sm">
                          <span>Max Order Value</span>
                          <input
                            type="number"
                            min={1}
                            value={settingsDraft.risk.max_order_value}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                risk: { ...current.risk, max_order_value: Number(e.target.value) || 0 },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-max-order-value"
                          />
                        </label>
                      </div>

                      <div className="grid grid-cols-2 gap-3 items-end">
                        <label className="flex flex-col gap-1 text-sm">
                          <span>Storage Pool Size</span>
                          <input
                            type="number"
                            min={1}
                            value={settingsDraft.storage.pool_size}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                storage: { ...current.storage, pool_size: Math.max(1, Number(e.target.value) || 1) },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            className="px-3 py-2 text-sm bg-surface-1 border border-[var(--border-color)] rounded"
                            data-testid="settings-storage-pool-size"
                          />
                        </label>

                        <label className="flex items-center gap-2 text-sm pt-6">
                          <input
                            type="checkbox"
                            checked={settingsDraft.storage.wal_mode}
                            onChange={(e) => {
                              setSettingsDraft((current) => current ? {
                                ...current,
                                storage: { ...current.storage, wal_mode: e.target.checked },
                              } : current);
                              setSettingsSuccess(null);
                            }}
                            data-testid="settings-storage-wal-mode"
                          />
                          <span>Enable SQLite WAL mode</span>
                        </label>
                      </div>

                      <div className="rounded border border-[var(--border-color)] bg-surface-1 px-3 py-2 text-xs text-text-muted">
                        <p>
                          <strong className="text-text-primary">Heads-up:</strong> storage and adapter changes are saved immediately, but the desktop runtime keeps using the current backend until restart.
                        </p>
                        {settingsDraft.storage.backend === 'timescale' && (
                          <p className="mt-1">
                            The Postgres option expects a TimescaleDB-enabled database URL.
                          </p>
                        )}
                      </div>
                    </div>
                  ) : (
                    <p className="text-sm text-text-muted">Loading application settings…</p>
                  )}
                </div>
              </div>
            </div>
            <div className="flex items-center justify-between gap-3 mt-4">
              <span className="text-xs text-text-muted">
                Runtime continues using <strong className="text-text-primary">{appSettings ? formatSettingsLabel(appSettings.runtime_storage_backend) : 'current settings'}</strong> until restart.
              </span>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => void handleSaveSettings()}
                  disabled={settingsBusy || !settingsDraft}
                  className="px-3 py-1 text-xs bg-surface-2 hover:bg-surface-3 border border-[var(--border-color)] rounded disabled:opacity-50"
                  data-testid="save-app-settings"
                >
                  {settingsBusy ? 'Saving…' : 'Save & Apply on Restart'}
                </button>
                <button
                  onClick={() => setShowSettings(false)}
                  className="px-3 py-1 text-xs bg-primary-500 hover:bg-primary-600 text-white rounded"
                >
                  Close
                </button>
              </div>
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
              onClick={() => setCenterTab('data')}
              data-testid="tab-data"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'data' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              Stored Data
            </button>
            <button
              onClick={() => setCenterTab('notebook')}
              data-testid="tab-notebook"
              className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
                centerTab === 'notebook' ? 'bg-accent text-white' : 'bg-surface-2 text-text-muted hover:text-text-primary'
              }`}
            >
              Notebook
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
              <StrategyIDE defaultSymbol={selectedSymbol} />
            </div>
          ) : centerTab === 'ml' ? (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <MLWorkbench />
            </div>
          ) : centerTab === 'data' ? (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <DataBrowser defaultSymbol={selectedSymbol} />
            </div>
          ) : centerTab === 'notebook' ? (
            <div className="flex-1 bg-surface-1 rounded-lg border border-[var(--border-color)] overflow-hidden">
              <NotebookEditor />
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
  const [rules, setRules] = useState<AlertRuleDto[]>([]);
  const [showCreateAlert, setShowCreateAlert] = useState(false);
  const [alertSymbol, setAlertSymbol] = useState('');
  const [alertCondition, setAlertCondition] = useState<AlertCondition>('price_above');
  const [alertValue, setAlertValue] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshRules = useCallback(async () => {
    setIsLoading(true);
    try {
      const nextRules = await getAlertRules();
      setRules(Array.isArray(nextRules) ? nextRules : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unable to load alerts');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshRules();
  }, [refreshRules]);

  const handleCreateAlert = useCallback(async () => {
    const symbol = alertSymbol.trim().toUpperCase();

    if (symbol && alertValue) {
      const ruleId = `alert_${Date.now()}`;
      const payload = buildAlertRulePayload(symbol, alertCondition, alertValue);

      try {
        await addAlert(ruleId, JSON.stringify(payload));
        await refreshRules();
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unable to save alert');
        return;
      }

      setAlertSymbol('');
      setAlertValue('');
      setShowCreateAlert(false);
    }
  }, [alertCondition, alertSymbol, alertValue, refreshRules]);

  const handleRemoveAlert = useCallback(async (ruleId: string) => {
    try {
      const removed = await removeAlert(ruleId);
      if (removed) {
        setRules((current) => current.filter((rule) => rule.id !== ruleId));
      }
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unable to remove alert');
    }
  }, []);

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
            onChange={(e) => setAlertCondition(e.target.value as AlertCondition)}
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
        {error ? (
          <p className="text-bear">{error}</p>
        ) : isLoading ? (
          <p>Loading alerts…</p>
        ) : rules.length === 0 ? (
          <p>No active alerts.</p>
        ) : (
          rules.map((rule) => (
            <div key={rule.id} className="mb-2 flex items-start justify-between gap-2" data-testid="alert-item">
              <div className="min-w-0">
                <div className="truncate">{formatAlertRule(rule.rule)}</div>
                <div className="text-[10px] text-text-muted/70">{rule.id}</div>
              </div>
              <button
                type="button"
                onClick={() => void handleRemoveAlert(rule.id)}
                className="text-[10px] text-bear hover:brightness-125"
                data-testid={`alert-remove-${rule.id}`}
              >
                Remove
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
