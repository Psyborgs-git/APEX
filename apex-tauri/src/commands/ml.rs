use super::python_runtime;
use crate::dto::{MLModelDto, MLTrainingRequestDto, MLTrainingResultDto};
use crate::validation;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;
use tokio::process::Command;

/// Filesystem-backed ML model registry rooted at `<repo>/models`.
pub struct ModelRegistry {
    pub models_dir: PathBuf,
}

impl ModelRegistry {
    pub fn new(runtime_paths: &python_runtime::RuntimePaths) -> Self {
        let models_dir = runtime_paths.models_dir().to_path_buf();
        let _ = fs::create_dir_all(&models_dir);
        Self {
            models_dir,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModelMetadataFile {
    model_id: Option<String>,
    algorithm: String,
    feature_names: Vec<String>,
    target_column: String,
    data_path: Option<String>,
    metrics: HashMap<String, f64>,
    model_file: String,
    created_utc: String,
}

#[derive(Debug, Deserialize)]
struct TrainerCliResult {
    model_id: String,
    algorithm: String,
    metrics: HashMap<String, f64>,
    feature_names: Vec<String>,
    created_at: String,
    model_path: String,
    metadata_path: String,
    data_path: String,
    target_column: String,
}

fn sample_dataset_path(runtime_paths: &python_runtime::RuntimePaths) -> PathBuf {
    runtime_paths.data_dir().join("sample.csv")
}

fn ensure_training_dataset(
    relative_path: &str,
    runtime_paths: &python_runtime::RuntimePaths,
) -> Result<(), String> {
    let full_path = runtime_paths.resolve_user_relative_path(relative_path);
    if full_path.exists() {
        return Ok(());
    }

    if full_path != sample_dataset_path(runtime_paths) {
        return Err(format!(
            "Training data file not found: {}",
            full_path.display()
        ));
    }

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create sample data directory {:?}: {e}", parent))?;
    }

    let mut csv = String::from(
        "sma_20,sma_50,ema_12,ema_26,rsi_14,macd_signal,bb_upper,bb_lower,atr_14,volume_lag_1,signal\n",
    );
    for index in 0..240 {
        let i = index as f64;
        let base = 100.0 + i * 0.35;
        let wave = (i / 7.0).sin() * 3.0;
        let sma_20 = base + wave;
        let sma_50 = base - 1.8 + wave * 0.35;
        let ema_12 = sma_20 + 0.6 + (i / 5.0).cos() * 0.25;
        let ema_26 = sma_50 + 0.2;
        let rsi_14 = 48.0 + (i / 6.0).sin() * 18.0;
        let macd_signal = ema_12 - ema_26;
        let bb_upper = sma_20 + 2.4;
        let bb_lower = sma_20 - 2.4;
        let atr_14 = 1.1 + (i / 9.0).cos().abs();
        let volume_lag_1 = 450_000.0 + i * 3_500.0 + ((index % 9) as f64 * 1_250.0);
        let signal = if ema_12 > ema_26 && rsi_14 > 50.0 { 1 } else { 0 };

        csv.push_str(&format!(
            "{sma_20:.4},{sma_50:.4},{ema_12:.4},{ema_26:.4},{rsi_14:.4},{macd_signal:.4},{bb_upper:.4},{bb_lower:.4},{atr_14:.4},{volume_lag_1:.2},{signal}\n"
        ));
    }

    fs::write(&full_path, csv)
        .map_err(|e| format!("Failed to create sample dataset {:?}: {e}", full_path))?;
    Ok(())
}

fn load_registered_models(models_dir: &Path) -> Result<Vec<MLModelDto>, String> {
    if !models_dir.exists() {
        return Ok(Vec::new());
    }

    let mut models = Vec::new();
    let entries = fs::read_dir(models_dir)
        .map_err(|e| format!("Failed to read models directory {:?}: {e}", models_dir))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to inspect model entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read model metadata {:?}: {e}", path))?;
        let metadata: ModelMetadataFile = match serde_json::from_str(&raw) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!(?error, path = ?path, "Skipping malformed model metadata file");
                continue;
            }
        };

        let model_id = metadata.model_id.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown_model")
                .to_string()
        });

        let model_path = models_dir.join(&metadata.model_file);
        let status = if model_path.exists() { "completed" } else { "missing_artifact" };

        models.push(MLModelDto {
            model_id,
            algorithm: metadata.algorithm,
            status: status.to_string(),
            metrics: metadata.metrics,
            feature_names: metadata.feature_names,
            created_at: metadata.created_utc,
            data_path: metadata.data_path.unwrap_or_else(|| "unknown".into()),
            target_column: metadata.target_column,
        });
    }

    models.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(models)
}

/// List all trained ML models.
#[tauri::command]
pub async fn list_ml_models(
    state: State<'_, ModelRegistry>,
) -> Result<Vec<MLModelDto>, String> {
    load_registered_models(&state.models_dir)
}

/// Train a new ML model by invoking the Python trainer.
#[tauri::command]
pub async fn train_ml_model(
    request: MLTrainingRequestDto,
    state: State<'_, ModelRegistry>,
    runtime_paths: State<'_, python_runtime::RuntimePaths>,
) -> Result<MLTrainingResultDto, String> {
    let runtime_paths = runtime_paths.inner();

    // Validate inputs
    validation::validate_algorithm(&request.algorithm)?;
    validation::validate_path(&request.data_path)?;
    validation::validate_string_length(&request.target_column, "target_column")?;
    for col in &request.feature_columns {
        validation::validate_string_length(col, "feature_column")?;
    }
    validation::validate_path(&request.data_path)?;
    if request.n_splits == 0 || request.n_splits > 20 {
        return Err("n_splits must be between 1 and 20".into());
    }
    if request.feature_columns.is_empty() {
        return Err("feature_columns must not be empty".into());
    }
    if request.lag_periods.is_empty() {
        return Err("lag_periods must not be empty".into());
    }

    ensure_training_dataset(&request.data_path, runtime_paths)?;

    let python = python_runtime::resolve_python_executable(
        runtime_paths,
        "APEX_ML_PYTHON_PATH",
        &["APEX_STRATEGY_PYTHON_PATH"],
    )?;

    let mut payload = serde_json::to_value(&request)
        .map_err(|e| format!("Failed to serialize training request: {e}"))?;
    payload["output_dir"] = serde_json::json!(state.models_dir.to_string_lossy().to_string());

    let output = Command::new(&python)
        .current_dir(runtime_paths.work_root())
        .env("PYTHONPATH", python_runtime::build_python_path(runtime_paths)?)
        .env("PYTHONIOENCODING", "utf-8")
        .arg("-m")
        .arg("ml.trainer")
        .arg("--config-json")
        .arg(
            serde_json::to_string(&payload)
                .map_err(|e| format!("Failed to encode training payload: {e}"))?,
        )
        .output()
        .await
        .map_err(|e| format!("Failed to start Python trainer with {}: {e}", python.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .or_else(|| stdout.lines().rev().find(|line| !line.trim().is_empty()))
            .unwrap_or("ML training failed without an error message");
        return Err(python_runtime::enrich_python_error(
            message,
            &["APEX_ML_PYTHON_PATH", "APEX_STRATEGY_PYTHON_PATH"],
        ));
    }

    let json_line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "Python trainer did not return a JSON result".to_string())?;
    let result: TrainerCliResult = serde_json::from_str(json_line)
        .map_err(|e| format!("Failed to parse trainer output: {e}"))?;

    tracing::info!(
        model_id = %result.model_id,
        algorithm = %result.algorithm,
        model_path = %result.model_path,
        metadata_path = %result.metadata_path,
        created_at = %result.created_at,
        data_path = %result.data_path,
        target_column = %result.target_column,
        "ML training completed"
    );

    Ok(MLTrainingResultDto {
        model_id: result.model_id,
        metrics: result.metrics,
        feature_names: result.feature_names,
        status: "completed".into(),
    })
}

/// Delete a trained ML model by ID.
#[tauri::command]
pub async fn delete_ml_model(
    model_id: String,
    state: State<'_, ModelRegistry>,
) -> Result<bool, String> {
    validation::validate_string_length(&model_id, "model_id")?;

    let metadata_path = state.models_dir.join(format!("{model_id}.json"));
    let model_path = state.models_dir.join(format!("{model_id}.joblib"));
    let mut removed_any = false;

    let resolved_model_path = if metadata_path.exists() {
        let raw = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read model metadata {:?}: {e}", metadata_path))?;
        let metadata: ModelMetadataFile = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse model metadata {:?}: {e}", metadata_path))?;
        state.models_dir.join(metadata.model_file)
    } else {
        model_path
    };

    if metadata_path.exists() {
        fs::remove_file(&metadata_path)
            .map_err(|e| format!("Failed to delete metadata {:?}: {e}", metadata_path))?;
        removed_any = true;
    }

    if resolved_model_path.exists() {
        fs::remove_file(&resolved_model_path)
            .map_err(|e| format!("Failed to delete model artifact {:?}: {e}", resolved_model_path))?;
        removed_any = true;
    }

    Ok(removed_any)
}
