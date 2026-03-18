use crate::dto::{MLModelDto, MLTrainingRequestDto, MLTrainingResultDto};
use crate::validation;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;

/// In-memory model registry for ML models.
/// In production this would persist to SQLite; for now we keep it in process
/// memory so the IPC contract is fully wired end-to-end.
pub struct ModelRegistry {
    pub models: Mutex<Vec<MLModelDto>>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self {
            models: Mutex::new(Vec::new()),
        }
    }
}

/// List all trained ML models.
#[tauri::command]
pub async fn list_ml_models(
    state: State<'_, ModelRegistry>,
) -> Result<Vec<MLModelDto>, String> {
    let models = state.models.lock().map_err(|e| e.to_string())?;
    Ok(models.clone())
}

/// Train a new ML model (stub implementation — records the request and
/// returns mock metrics; the real pipeline would call the Python sidecar).
#[tauri::command]
pub async fn train_ml_model(
    request: MLTrainingRequestDto,
    state: State<'_, ModelRegistry>,
) -> Result<MLTrainingResultDto, String> {
    // Validate inputs
    validation::validate_algorithm(&request.algorithm)?;
    validation::validate_path(&request.data_path)?;
    validation::validate_string_length(&request.target_column, "target_column")?;
    for col in &request.feature_columns {
        validation::validate_string_length(col, "feature_column")?;
    }
    if request.n_splits == 0 || request.n_splits > 20 {
        return Err("n_splits must be between 1 and 20".into());
    }

    let model_id = format!("model_{}", chrono::Utc::now().timestamp_millis());

    // In a real implementation this would send a request to the Python sidecar
    // via the IPC channel to run apex-python/ml/trainer.py
    let mut metrics = HashMap::new();
    metrics.insert("accuracy".into(), 0.72);
    metrics.insert("f1".into(), 0.68);
    metrics.insert("precision".into(), 0.74);
    metrics.insert("recall".into(), 0.65);

    let model = MLModelDto {
        model_id: model_id.clone(),
        algorithm: request.algorithm,
        status: "completed".into(),
        metrics: metrics.clone(),
        feature_names: request.feature_columns.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        data_path: request.data_path,
        target_column: request.target_column,
    };

    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    models.push(model);

    Ok(MLTrainingResultDto {
        model_id,
        metrics,
        feature_names: request.feature_columns,
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

    let mut models = state.models.lock().map_err(|e| e.to_string())?;
    let before = models.len();
    models.retain(|m| m.model_id != model_id);
    Ok(models.len() < before)
}
