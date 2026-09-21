"""Single-row inference for a trained APEX model.

Loads a ``.joblib`` artifact persisted by :mod:`ml.trainer` and scores one
feature row supplied as JSON. Prints a single JSON line:

    {"signal": <int>, "probability": <float>, "model_id": <str>}

Used by the Rust automation engine — the model's joblib file lives next to a
companion ``<model_id>.json`` metadata file describing feature ordering.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import joblib


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Predict with an APEX model")
    parser.add_argument("--model-path", required=True)
    parser.add_argument(
        "--features-json",
        required=True,
        help="JSON object mapping feature name -> value, or a JSON array in metadata feature order",
    )
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    model_path = Path(args.model_path)
    metadata_path = model_path.with_suffix(".json")

    feature_order: list[str] = []
    if metadata_path.exists():
        meta = json.loads(metadata_path.read_text())
        feature_order = list(meta.get("feature_names") or [])

    raw = json.loads(args.features_json)
    if isinstance(raw, dict):
        if not feature_order:
            feature_order = sorted(raw)
        try:
            row = [[float(raw[name]) for name in feature_order]]
        except KeyError as exc:
            missing = ", ".join(sorted(set(feature_order) - set(raw)))
            print(json.dumps({"error": f"missing features: {missing} (offending: {exc})"}))
            return 1
    else:
        row = [[float(v) for v in raw]]

    model = joblib.load(model_path)
    pred = model.predict(row)[0]

    probability = None
    if hasattr(model, "predict_proba"):
        try:
            proba = model.predict_proba(row)[0]
            probability = float(max(proba))
        except Exception:
            probability = None

    print(
        json.dumps(
            {
                "signal": int(pred),
                "probability": probability,
                "model_id": model_path.stem,
                "features_used": feature_order,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
