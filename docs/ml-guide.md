# ML Workbench Guide

The APEX ML workbench lets you train, evaluate, and deploy machine learning
models for use in trading strategies — all from within the terminal.

## Overview

```
┌─────────────┐     ┌──────────────┐     ┌───────────────┐
│  Historical  │────▶│   Trainer    │────▶│  Registered   │
│  Data (CSV/  │     │  (scikit /   │     │  Model (.job  │
│   Parquet)   │     │   xgboost)   │     │  + metadata)  │
└─────────────┘     └──────────────┘     └──────┬────────┘
                                                │
                                         ┌──────▼────────┐
                                         │  Strategy     │
                                         │  (inference)  │
                                         └───────────────┘
```

The pipeline:

1. **Prepare data** — historical OHLCV or custom features in CSV/Parquet
2. **Configure training** — algorithm, features, hyperparameters
3. **Train & evaluate** — time-series cross-validation
4. **Register model** — joblib artefact + metadata JSON
5. **Use in strategy** — load model and call `predict()` in `on_bar`

## Supported Algorithms

| Algorithm              | Key                      | Library        |
| ---------------------- | ------------------------ | -------------- |
| Random Forest          | `random_forest`          | scikit-learn   |
| Gradient Boosting      | `gradient_boosting`      | scikit-learn   |
| Logistic Regression    | `logistic_regression`    | scikit-learn   |
| XGBoost                | `xgboost`                | xgboost        |

## Quick Start

### 1. Prepare Your Data

Place your data file in the `data/` directory. The file should be a CSV or
Parquet file with:

- A **target column** (e.g., `signal` with values `1` for buy, `0` for hold,
  `-1` for sell)
- **Feature columns** (e.g., `close`, `volume`, `rsi_14`, `sma_20`)

Example CSV:

```csv
timestamp,close,volume,rsi_14,sma_20,signal
2024-01-02,2500.50,1234567,55.2,2480.30,1
2024-01-03,2510.75,1345678,58.1,2482.50,1
2024-01-04,2495.00,1456789,42.3,2484.20,-1
...
```

### 2. Configure Training

```python
from ml.trainer import ModelTrainer, TrainingConfig

config = TrainingConfig(
    algorithm="random_forest",
    data_path="data/reliance_features.parquet",
    target_column="signal",
    feature_columns=["close", "volume", "rsi_14", "sma_20", "macd"],
    hyperparams={
        "n_estimators": 200,
        "max_depth": 10,
        "min_samples_split": 5,
        "random_state": 42,
    },
    n_splits=5,
    output_dir="models",
    lag_periods=[1, 5, 10],
)
```

#### `TrainingConfig` Fields

| Field              | Type          | Default          | Description                          |
| ------------------ | ------------- | ---------------- | ------------------------------------ |
| `algorithm`        | `str`         | —                | Algorithm key (see table above)      |
| `data_path`        | `str`         | —                | Path to CSV or Parquet file          |
| `target_column`    | `str`         | —                | Name of the target column            |
| `feature_columns`  | `list[str]`   | all non-target   | Feature column names                 |
| `hyperparams`      | `dict`        | `{}`             | Algorithm-specific hyperparameters   |
| `n_splits`         | `int`         | `5`              | Time-series CV splits                |
| `output_dir`       | `str`         | `"models"`       | Output directory for artefacts       |
| `lag_periods`      | `list[int]`   | `[1, 5, 10]`    | Lag periods for auto-generated features |

### 3. Train the Model

```python
trainer = ModelTrainer()
result = trainer.train(config)

print(f"Model saved to: {result.model_path}")
print(f"Metrics: {result.metrics}")
# Output:
# Model saved to: models/random_forest_20240115T103045Z.joblib
# Metrics: {'accuracy': 0.72, 'f1': 0.68, 'precision': 0.71, 'recall': 0.66}
```

### 4. Review the Result

The trainer produces two files:

**`random_forest_20240115T103045Z.joblib`** — the serialized model

**`random_forest_20240115T103045Z.json`** — metadata:

```json
{
  "algorithm": "random_forest",
  "hyperparams": {
    "n_estimators": 200,
    "max_depth": 10
  },
  "feature_names": [
    "close", "volume", "rsi_14", "sma_20", "macd",
    "close_lag1", "close_lag5", "close_lag10",
    "volume_lag1", "volume_lag5", "volume_lag10",
    ...
  ],
  "target_column": "signal",
  "n_splits": 5,
  "metrics": {
    "accuracy": 0.72,
    "f1": 0.68,
    "precision": 0.71,
    "recall": 0.66
  },
  "model_file": "random_forest_20240115T103045Z.joblib",
  "created_utc": "20240115T103045Z"
}
```

### 5. Use in a Strategy

```python
import math
import joblib
from apex_sdk import Strategy, Bar, Signal, Timeframe


class MLStrategy(Strategy):
    def on_init(self, params: dict) -> None:
        model_path = params.get("model_path", "models/random_forest_latest.joblib")
        self.model = joblib.load(model_path)
        self.subscribe(["RELIANCE.NS"], Timeframe.M5)
        self.log(f"ML model loaded from {model_path}")

    def on_bar(self, symbol: str, bar: Bar) -> None:
        # Build feature vector matching training features
        features = self._build_features(symbol, bar)
        if features is None:
            return

        prediction = self.model.predict([features])[0]
        probability = self.model.predict_proba([features]).max()

        if prediction == 1:
            self.emit(Signal(
                symbol=symbol,
                direction="long",
                strength=float(probability),
                metadata={"model": "random_forest", "prediction": int(prediction)},
            ))
        elif prediction == -1:
            self.emit(Signal(
                symbol=symbol,
                direction="short",
                strength=float(probability),
                metadata={"model": "random_forest", "prediction": int(prediction)},
            ))

    def _build_features(self, symbol: str, bar: Bar) -> list[float] | None:
        """Build feature vector from indicators and bar data."""
        rsi = self.indicator("rsi", symbol, 14)
        sma = self.indicator("sma", symbol, 20)
        macd = self.indicator("macd", symbol)

        # Skip if indicators aren't warm yet
        if math.isnan(rsi):
            return None

        return [bar.close, bar.volume, rsi, sma, macd]
```

## Evaluation Metrics

The trainer reports four metrics, all computed via **time-series
cross-validation** (no look-ahead bias):

| Metric      | Description                                  |
| ----------- | -------------------------------------------- |
| `accuracy`  | Fraction of correct predictions              |
| `f1`        | Weighted F1 score                            |
| `precision` | Weighted precision (false positive control)  |
| `recall`    | Weighted recall (false negative control)     |

## Auto-Generated Lag Features

For each feature column, the trainer automatically creates lag features for
the configured `lag_periods`. For example, with `lag_periods = [1, 5, 10]`
and a feature `close`:

- `close_lag1` — close value 1 bar ago
- `close_lag5` — close value 5 bars ago
- `close_lag10` — close value 10 bars ago

Rows with null values (from the lag) are dropped before training.

## Algorithm-Specific Hyperparameters

### Random Forest

```python
hyperparams = {
    "n_estimators": 200,     # number of trees
    "max_depth": 10,         # max tree depth (None = unlimited)
    "min_samples_split": 5,  # min samples to split a node
    "random_state": 42,      # reproducibility seed
}
```

### Gradient Boosting

```python
hyperparams = {
    "n_estimators": 100,
    "learning_rate": 0.1,
    "max_depth": 5,
    "subsample": 0.8,
    "random_state": 42,
}
```

### XGBoost

```python
hyperparams = {
    "n_estimators": 200,
    "learning_rate": 0.05,
    "max_depth": 6,
    "subsample": 0.8,
    "colsample_bytree": 0.8,
    "eval_metric": "mlogloss",
    "random_state": 42,
}
```

### Logistic Regression

```python
hyperparams = {
    "C": 1.0,               # inverse regularization strength
    "max_iter": 1000,
    "solver": "lbfgs",
    "random_state": 42,
}
```

## Configuration

ML settings in `config/apex.toml`:

```toml
[ml]
models_dir = "models"
default_cv_splits = 5
default_lag_periods = [1, 5, 10]
```

## Best Practices

1. **Always use time-series CV** — never shuffle financial data. The built-in
   `TimeSeriesSplit` ensures no look-ahead bias.

2. **Feature engineering matters more than algorithms** — start with simple
   features (SMA, RSI, volume ratios) before trying complex models.

3. **Watch for overfitting** — if training accuracy is much higher than CV
   accuracy, reduce model complexity.

4. **Retrain periodically** — market regimes change. Schedule retraining
   weekly or monthly.

5. **Start with `random_forest`** — it's robust, interpretable, and rarely
   needs extensive tuning.

6. **Keep feature lists in metadata** — the auto-generated metadata JSON
   records exactly which features were used, making reproduction easy.

7. **Version your models** — the timestamped filenames provide natural
   versioning. Keep old models until the new one proves itself in paper
   trading.
