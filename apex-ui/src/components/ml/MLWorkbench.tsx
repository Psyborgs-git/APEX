import React, { useState, useEffect, useCallback } from 'react';
import { useMLStore } from '../../stores/mlStore';
import { listMLModels, trainMLModel, deleteMLModel } from '../../lib/tauri';
import { formatPct } from '../../lib/format';
import type { MLTrainingRequestDto } from '../../lib/types';

const ALGORITHMS = [
  { value: 'random_forest', label: 'Random Forest' },
  { value: 'gradient_boosting', label: 'Gradient Boosting' },
  { value: 'logistic_regression', label: 'Logistic Regression' },
  { value: 'xgboost', label: 'XGBoost' },
];

const DEFAULT_FEATURES = [
  'sma_20', 'sma_50', 'ema_12', 'ema_26', 'rsi_14',
  'macd_signal', 'bb_upper', 'bb_lower', 'atr_14', 'volume_lag_1',
];

export const MLWorkbench: React.FC = React.memo(() => {
  const models = useMLStore((s) => s.models);
  const isTraining = useMLStore((s) => s.isTraining);
  const trainingError = useMLStore((s) => s.trainingError);
  const setModels = useMLStore((s) => s.setModels);
  const addModel = useMLStore((s) => s.addModel);
  const removeModel = useMLStore((s) => s.removeModel);
  const setTraining = useMLStore((s) => s.setTraining);
  const setTrainingError = useMLStore((s) => s.setTrainingError);

  // Training form state
  const [algorithm, setAlgorithm] = useState('random_forest');
  const [dataPath, setDataPath] = useState('data/sample.csv');
  const [targetColumn, setTargetColumn] = useState('signal');
  const [selectedFeatures, setSelectedFeatures] = useState<string[]>(
    DEFAULT_FEATURES.slice(0, 5),
  );
  const [nSplits, setNSplits] = useState(5);
  const [lagPeriods, setLagPeriods] = useState('1,5,10');
  const [activeTab, setActiveTab] = useState<'train' | 'registry'>('train');

  // Load models on mount
  useEffect(() => {
    listMLModels()
      .then((m) => { if (Array.isArray(m)) setModels(m); })
      .catch(() => { /* backend may not be available */ });
  }, [setModels]);

  const toggleFeature = useCallback((feature: string) => {
    setSelectedFeatures((prev) =>
      prev.includes(feature)
        ? prev.filter((f) => f !== feature)
        : [...prev, feature],
    );
  }, []);

  const handleTrain = useCallback(async () => {
    setTraining(true);
    setTrainingError(null);
    try {
      const request: MLTrainingRequestDto = {
        algorithm,
        data_path: dataPath,
        target_column: targetColumn,
        feature_columns: selectedFeatures,
        hyperparams: {},
        n_splits: nSplits,
        lag_periods: lagPeriods.split(',').map(Number).filter((n) => !isNaN(n)),
      };
      const result = await trainMLModel(request);
      addModel({
        model_id: result.model_id,
        algorithm,
        status: result.status as 'idle' | 'training' | 'completed' | 'failed',
        metrics: result.metrics,
        feature_names: result.feature_names,
        created_at: new Date().toISOString(),
        data_path: dataPath,
        target_column: targetColumn,
      });
    } catch (err) {
      setTrainingError(err instanceof Error ? err.message : 'Training failed');
    } finally {
      setTraining(false);
    }
  }, [algorithm, dataPath, targetColumn, selectedFeatures, nSplits, lagPeriods, addModel, setTraining, setTrainingError]);

  const handleDelete = useCallback(
    async (modelId: string) => {
      try {
        await deleteMLModel(modelId);
        removeModel(modelId);
      } catch {
        /* ignore */
      }
    },
    [removeModel],
  );

  return (
    <div className="h-full flex flex-col" data-testid="ml-workbench">
      {/* Tab bar */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-[var(--border-color)] bg-surface-1">
        <button
          onClick={() => setActiveTab('train')}
          data-testid="ml-tab-train"
          className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
            activeTab === 'train'
              ? 'bg-accent text-white'
              : 'bg-surface-2 text-text-muted hover:text-text-primary'
          }`}
        >
          Train Model
        </button>
        <button
          onClick={() => setActiveTab('registry')}
          data-testid="ml-tab-registry"
          className={`px-3 py-1 text-xs font-mono rounded transition-colors ${
            activeTab === 'registry'
              ? 'bg-accent text-white'
              : 'bg-surface-2 text-text-muted hover:text-text-primary'
          }`}
        >
          Model Registry ({models.length})
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-3">
        {activeTab === 'train' ? (
          <TrainingPanel
            algorithm={algorithm}
            setAlgorithm={setAlgorithm}
            dataPath={dataPath}
            setDataPath={setDataPath}
            targetColumn={targetColumn}
            setTargetColumn={setTargetColumn}
            selectedFeatures={selectedFeatures}
            toggleFeature={toggleFeature}
            nSplits={nSplits}
            setNSplits={setNSplits}
            lagPeriods={lagPeriods}
            setLagPeriods={setLagPeriods}
            isTraining={isTraining}
            trainingError={trainingError}
            onTrain={handleTrain}
          />
        ) : (
          <ModelRegistry models={models} onDelete={handleDelete} />
        )}
      </div>
    </div>
  );
});

MLWorkbench.displayName = 'MLWorkbench';

/* ---------- Training panel ---------- */

interface TrainingPanelProps {
  algorithm: string;
  setAlgorithm: (v: string) => void;
  dataPath: string;
  setDataPath: (v: string) => void;
  targetColumn: string;
  setTargetColumn: (v: string) => void;
  selectedFeatures: string[];
  toggleFeature: (f: string) => void;
  nSplits: number;
  setNSplits: (v: number) => void;
  lagPeriods: string;
  setLagPeriods: (v: string) => void;
  isTraining: boolean;
  trainingError: string | null;
  onTrain: () => void;
}

const TrainingPanel: React.FC<TrainingPanelProps> = ({
  algorithm, setAlgorithm,
  dataPath, setDataPath,
  targetColumn, setTargetColumn,
  selectedFeatures, toggleFeature,
  nSplits, setNSplits,
  lagPeriods, setLagPeriods,
  isTraining, trainingError,
  onTrain,
}) => (
  <div className="space-y-4 max-w-2xl">
    <h3 className="text-sm font-medium text-text-primary">Model Configuration</h3>

    {/* Algorithm */}
    <div>
      <label className="block text-xs text-text-secondary mb-1">Algorithm</label>
      <select
        value={algorithm}
        onChange={(e) => setAlgorithm(e.target.value)}
        className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded"
        data-testid="ml-algorithm-select"
      >
        {ALGORITHMS.map((a) => (
          <option key={a.value} value={a.value}>
            {a.label}
          </option>
        ))}
      </select>
    </div>

    {/* Dataset */}
    <div className="grid grid-cols-2 gap-3">
      <div>
        <label className="block text-xs text-text-secondary mb-1">Data Path</label>
        <input
          type="text"
          value={dataPath}
          onChange={(e) => setDataPath(e.target.value)}
          className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded font-mono"
          data-testid="ml-data-path"
        />
      </div>
      <div>
        <label className="block text-xs text-text-secondary mb-1">Target Column</label>
        <input
          type="text"
          value={targetColumn}
          onChange={(e) => setTargetColumn(e.target.value)}
          className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded font-mono"
          data-testid="ml-target-column"
        />
      </div>
    </div>

    {/* Feature Selection */}
    <div>
      <label className="block text-xs text-text-secondary mb-1">
        Features ({selectedFeatures.length} selected)
      </label>
      <div
        className="flex flex-wrap gap-1 p-2 bg-surface-0 border border-[var(--border-color)] rounded max-h-24 overflow-auto"
        data-testid="ml-feature-list"
      >
        {DEFAULT_FEATURES.map((f) => (
          <button
            key={f}
            onClick={() => toggleFeature(f)}
            className={`px-2 py-0.5 text-xs rounded transition-colors ${
              selectedFeatures.includes(f)
                ? 'bg-accent text-white'
                : 'bg-surface-2 text-text-muted hover:text-text-primary'
            }`}
            data-testid={`ml-feature-${f}`}
          >
            {f}
          </button>
        ))}
      </div>
    </div>

    {/* CV & Lag */}
    <div className="grid grid-cols-2 gap-3">
      <div>
        <label className="block text-xs text-text-secondary mb-1">CV Splits</label>
        <input
          type="number"
          min={2}
          max={10}
          value={nSplits}
          onChange={(e) => setNSplits(Number(e.target.value))}
          className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded"
          data-testid="ml-cv-splits"
        />
      </div>
      <div>
        <label className="block text-xs text-text-secondary mb-1">Lag Periods (comma-separated)</label>
        <input
          type="text"
          value={lagPeriods}
          onChange={(e) => setLagPeriods(e.target.value)}
          className="w-full px-3 py-2 text-sm bg-surface-0 border border-[var(--border-color)] rounded font-mono"
          data-testid="ml-lag-periods"
        />
      </div>
    </div>

    {/* Train button */}
    <button
      onClick={onTrain}
      disabled={isTraining || selectedFeatures.length === 0}
      className="w-full px-4 py-2 text-sm font-medium bg-accent hover:bg-blue-600 text-white rounded disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      data-testid="ml-train-button"
    >
      {isTraining ? 'Training…' : 'Start Training'}
    </button>

    {/* Status */}
    {isTraining && (
      <div className="flex items-center gap-2 text-xs text-text-secondary" data-testid="ml-training-status">
        <span className="animate-pulse">●</span> Training in progress…
      </div>
    )}
    {trainingError && (
      <div className="text-xs text-bear" data-testid="ml-training-error">
        Error: {trainingError}
      </div>
    )}
  </div>
);

/* ---------- Model registry ---------- */

interface ModelRegistryProps {
  models: import('../../lib/types').MLModelDto[];
  onDelete: (id: string) => void;
}

const ModelRegistry: React.FC<ModelRegistryProps> = ({ models, onDelete }) => (
  <div className="space-y-3" data-testid="ml-model-registry">
    <h3 className="text-sm font-medium text-text-primary">
      Trained Models ({models.length})
    </h3>

    {models.length === 0 ? (
      <p className="text-xs text-text-muted" data-testid="ml-no-models">
        No models trained yet. Use the Train tab to build your first model.
      </p>
    ) : (
      <div className="space-y-2">
        {models.map((model) => (
          <div
            key={model.model_id}
            className="p-3 bg-surface-0 border border-[var(--border-color)] rounded"
            data-testid={`ml-model-${model.model_id}`}
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <span className="text-sm font-mono text-text-primary">{model.model_id}</span>
                <span
                  className={`px-1.5 py-0.5 text-xs rounded ${
                    model.status === 'completed'
                      ? 'bg-green-900/30 text-bull'
                      : model.status === 'failed'
                        ? 'bg-red-900/30 text-bear'
                        : 'bg-blue-900/30 text-accent'
                  }`}
                  data-testid={`ml-model-status-${model.model_id}`}
                >
                  {model.status}
                </span>
              </div>
              <button
                onClick={() => onDelete(model.model_id)}
                className="text-xs text-text-muted hover:text-bear transition-colors"
                data-testid={`ml-delete-${model.model_id}`}
              >
                Delete
              </button>
            </div>

            <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
              <div>
                <span className="text-text-muted">Algorithm: </span>
                <span className="text-text-secondary">{model.algorithm}</span>
              </div>
              <div>
                <span className="text-text-muted">Data: </span>
                <span className="text-text-secondary font-mono">{model.data_path}</span>
              </div>
              {Object.entries(model.metrics).map(([key, value]) => (
                <div key={key}>
                  <span className="text-text-muted">{key}: </span>
                  <span className="text-text-primary font-mono">
                    {formatPct(value)}
                  </span>
                </div>
              ))}
            </div>

            {model.feature_names.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1">
                {model.feature_names.map((f) => (
                  <span
                    key={f}
                    className="px-1.5 py-0.5 text-xs bg-surface-2 text-text-muted rounded"
                  >
                    {f}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    )}
  </div>
);
