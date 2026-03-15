"""ML model trainer with time-series cross-validation.

Supports scikit-learn estimators and XGBoost.  Models are persisted with
:mod:`joblib` alongside a companion ``metadata.json`` so that the Rust core
can discover registered models without importing Python.
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import joblib
import polars as pl
from sklearn.base import BaseEstimator, clone
from sklearn.ensemble import GradientBoostingClassifier, RandomForestClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, f1_score, precision_score, recall_score
from sklearn.model_selection import TimeSeriesSplit

logger = logging.getLogger(__name__)


# ------------------------------------------------------------------
# Config / result types
# ------------------------------------------------------------------

@dataclass
class TrainingConfig:
    """Everything needed to kick off a training run."""

    algorithm: str  # "random_forest" | "gradient_boosting" | "xgboost" | "logistic_regression"
    data_path: str  # path to Parquet / CSV
    target_column: str
    feature_columns: list[str] = field(default_factory=list)
    hyperparams: dict[str, Any] = field(default_factory=dict)
    n_splits: int = 5
    output_dir: str = "models"
    lag_periods: list[int] = field(default_factory=lambda: [1, 5, 10])


@dataclass
class TrainedModel:
    """Result of a successful training run."""

    model: BaseEstimator
    metrics: dict[str, float]
    feature_names: list[str]
    metadata_path: Path
    model_path: Path


# ------------------------------------------------------------------
# Trainer
# ------------------------------------------------------------------


class ModelTrainer:
    """Train, evaluate, and register ML models for APEX strategies."""

    _ALGORITHMS: dict[str, type[BaseEstimator]] = {
        "random_forest": RandomForestClassifier,
        "gradient_boosting": GradientBoostingClassifier,
        "logistic_regression": LogisticRegression,
    }

    def train(self, config: TrainingConfig) -> TrainedModel:
        """End-to-end train → evaluate → register pipeline."""
        logger.info("Starting training run: algorithm=%s", config.algorithm)

        df = self._load_data(config)
        x, y, feature_names = self._build_features(df, config)
        model = self._build_model(config.algorithm, config.hyperparams)

        cv = TimeSeriesSplit(n_splits=config.n_splits)
        metrics = self._evaluate(model, x, y, cv)

        # Final fit on full dataset
        model.fit(x, y)

        return self._register(model, feature_names, metrics, config)

    # ------------------------------------------------------------------
    # Pipeline stages
    # ------------------------------------------------------------------

    def _load_data(self, config: TrainingConfig) -> pl.DataFrame:
        """Load data from Parquet or CSV into a Polars DataFrame."""
        path = Path(config.data_path)
        if path.suffix == ".parquet":
            return pl.read_parquet(path)
        if path.suffix == ".csv":
            return pl.read_csv(path)
        raise ValueError(f"Unsupported file format: {path.suffix}")

    def _build_features(
        self,
        df: pl.DataFrame,
        config: TrainingConfig,
    ) -> tuple[Any, Any, list[str]]:
        """Apply lag features and return (X, y, feature_names)."""
        feature_cols = config.feature_columns or [
            c for c in df.columns if c != config.target_column
        ]

        # Add lag features
        for col in list(feature_cols):
            for lag in config.lag_periods:
                lag_name = f"{col}_lag{lag}"
                df = df.with_columns(pl.col(col).shift(lag).alias(lag_name))
                feature_cols.append(lag_name)

        df = df.drop_nulls()

        x = df.select(feature_cols).to_numpy()
        y = df.select(config.target_column).to_numpy().ravel()
        return x, y, feature_cols

    def _build_model(
        self,
        algorithm: str,
        hyperparams: dict[str, Any],
    ) -> BaseEstimator:
        """Instantiate the requested estimator."""
        if algorithm == "xgboost":
            import xgboost as xgb  # noqa: PLC0415

            return xgb.XGBClassifier(**hyperparams)

        cls = self._ALGORITHMS.get(algorithm)
        if cls is None:
            raise ValueError(
                f"Unknown algorithm '{algorithm}'. "
                f"Supported: {sorted([*self._ALGORITHMS, 'xgboost'])}"
            )
        return cls(**hyperparams)

    def _evaluate(
        self,
        model: BaseEstimator,
        x: Any,
        y: Any,
        cv: TimeSeriesSplit,
    ) -> dict[str, float]:
        """Run TimeSeriesSplit CV and return averaged metrics."""
        accuracy_scores: list[float] = []
        f1_scores: list[float] = []
        precision_scores: list[float] = []
        recall_scores: list[float] = []

        for fold, (train_idx, test_idx) in enumerate(cv.split(x)):
            fold_model = clone(model)
            fold_model.fit(x[train_idx], y[train_idx])
            preds = fold_model.predict(x[test_idx])

            acc = accuracy_score(y[test_idx], preds)
            f1 = f1_score(y[test_idx], preds, average="weighted", zero_division=0)
            prec = precision_score(y[test_idx], preds, average="weighted", zero_division=0)
            rec = recall_score(y[test_idx], preds, average="weighted", zero_division=0)

            logger.info(
                "Fold %d — accuracy=%.4f  f1=%.4f  precision=%.4f  recall=%.4f",
                fold, acc, f1, prec, rec,
            )
            accuracy_scores.append(acc)
            f1_scores.append(f1)
            precision_scores.append(prec)
            recall_scores.append(rec)

        return {
            "accuracy": sum(accuracy_scores) / len(accuracy_scores),
            "f1": sum(f1_scores) / len(f1_scores),
            "precision": sum(precision_scores) / len(precision_scores),
            "recall": sum(recall_scores) / len(recall_scores),
        }

    def _register(
        self,
        model: BaseEstimator,
        feature_names: list[str],
        metrics: dict[str, float],
        config: TrainingConfig,
    ) -> TrainedModel:
        """Persist model with joblib + companion metadata.json."""
        out_dir = Path(config.output_dir)
        out_dir.mkdir(parents=True, exist_ok=True)

        timestamp = datetime.now(tz=timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        model_filename = f"{config.algorithm}_{timestamp}.joblib"
        model_path = out_dir / model_filename

        joblib.dump(model, model_path)
        logger.info("Model saved to %s", model_path)

        metadata = {
            "algorithm": config.algorithm,
            "hyperparams": config.hyperparams,
            "feature_names": feature_names,
            "target_column": config.target_column,
            "n_splits": config.n_splits,
            "metrics": metrics,
            "model_file": model_filename,
            "created_utc": timestamp,
        }
        metadata_path = model_path.with_suffix(".json")
        metadata_path.write_text(json.dumps(metadata, indent=2))
        logger.info("Metadata saved to %s", metadata_path)

        return TrainedModel(
            model=model,
            metrics=metrics,
            feature_names=feature_names,
            metadata_path=metadata_path,
            model_path=model_path,
        )
