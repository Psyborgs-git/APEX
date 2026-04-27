use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::models::{DriftMeasurement, FeatureImportance, RunStatus};

// ---------------------------------------------------------------------------
// Experiment / Run structures
// ---------------------------------------------------------------------------

/// A single ML experiment run record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRun {
    pub run_id:              String,
    pub experiment_id:       String,
    pub params:              HashMap<String, f64>,
    pub metrics:             HashMap<String, f64>,
    pub string_params:       HashMap<String, String>,
    pub feature_importances: Vec<FeatureImportance>,
    pub status:              RunStatus,
    pub started_at:          DateTime<Utc>,
    pub ended_at:            Option<DateTime<Utc>>,
    pub notes:               String,
}

/// An ML experiment groups related runs together
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlExperiment {
    pub experiment_id: String,
    pub name:          String,
    pub description:   String,
    pub tags:          HashMap<String, String>,
    pub created_at:    DateTime<Utc>,
    pub runs:          Vec<ExperimentRun>,
}

// ---------------------------------------------------------------------------
// Model registry / metadata
// ---------------------------------------------------------------------------

/// Deployment status of a model version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelDeploymentStatus {
    Staging,
    Production,
    Archived,
    RolledBack,
}

/// Metadata for a deployed/registered ML model version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    pub model_id:           String,
    pub version:            u32,
    pub experiment_run_id:  Option<String>,
    pub framework:          String,
    pub artifact_path:      String,
    pub deployment_status:  ModelDeploymentStatus,
    pub metrics:            HashMap<String, f64>,
    pub feature_importances: Vec<FeatureImportance>,
    pub registered_at:      DateTime<Utc>,
    pub promoted_at:        Option<DateTime<Utc>>,
    pub notes:              String,
}

// ---------------------------------------------------------------------------
// Drift monitor
// ---------------------------------------------------------------------------

/// Configuration for feature drift detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftMonitorConfig {
    /// PSI threshold above which a feature is considered drifted
    pub psi_threshold:      f64,
    /// KS statistic threshold
    pub ks_threshold:       f64,
    /// Fraction of features that may drift before issuing a model alert
    pub drift_feature_fraction_alert: f64,
}

impl Default for DriftMonitorConfig {
    fn default() -> Self {
        Self {
            psi_threshold:                0.20,
            ks_threshold:                 0.10,
            drift_feature_fraction_alert: 0.30,
        }
    }
}

/// Leakage risk level (P4 requirement: leakage checks)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LeakageRisk {
    None,
    Low,
    High,
}

/// Result of a leakage check on a feature set
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakageCheckResult {
    pub feature_name: String,
    pub risk:         LeakageRisk,
    pub reason:       String,
}

// ---------------------------------------------------------------------------
// Tracker
// ---------------------------------------------------------------------------

/// Central ML experiment tracker and model registry.
///
/// Manages experiments, runs, model versions, and drift measurements.
/// Thread-safe via internal Arc<Mutex<…>> state.
pub struct MlTracker {
    experiments: Arc<Mutex<HashMap<String, MlExperiment>>>,
    models:      Arc<Mutex<HashMap<String, Vec<ModelVersion>>>>,
    drift_log:   Arc<Mutex<Vec<DriftMeasurement>>>,
    config:      DriftMonitorConfig,
}

impl MlTracker {
    /// Create a new tracker with the default drift monitor configuration
    pub fn new() -> Self {
        Self::with_config(DriftMonitorConfig::default())
    }

    /// Create a new tracker with a custom drift configuration
    pub fn with_config(config: DriftMonitorConfig) -> Self {
        Self {
            experiments: Arc::new(Mutex::new(HashMap::new())),
            models:      Arc::new(Mutex::new(HashMap::new())),
            drift_log:   Arc::new(Mutex::new(Vec::new())),
            config,
        }
    }

    // ----- Experiment management -------------------------------------------

    /// Create a new experiment and return its ID
    pub fn create_experiment(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        tags: HashMap<String, String>,
    ) -> String {
        let experiment_id = Uuid::new_v4().to_string();
        let experiment = MlExperiment {
            experiment_id: experiment_id.clone(),
            name: name.into(),
            description: description.into(),
            tags,
            created_at: Utc::now(),
            runs: Vec::new(),
        };
        self.experiments
            .lock()
            .unwrap()
            .insert(experiment_id.clone(), experiment);
        info!(experiment_id = %experiment_id, "ML experiment created");
        experiment_id
    }

    /// Start a new run within an experiment and return the run ID
    pub fn start_run(
        &self,
        experiment_id: &str,
        params: HashMap<String, f64>,
        string_params: HashMap<String, String>,
    ) -> Result<String> {
        let mut exps = self.experiments.lock().unwrap();
        let experiment = exps
            .get_mut(experiment_id)
            .ok_or_else(|| anyhow!("Experiment not found: {}", experiment_id))?;

        let run_id = Uuid::new_v4().to_string();
        experiment.runs.push(ExperimentRun {
            run_id: run_id.clone(),
            experiment_id: experiment_id.to_string(),
            params,
            string_params,
            metrics: HashMap::new(),
            feature_importances: Vec::new(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            ended_at: None,
            notes: String::new(),
        });
        info!(experiment_id = %experiment_id, run_id = %run_id, "ML run started");
        Ok(run_id)
    }

    /// Log a metric for a running experiment run
    pub fn log_metric(
        &self,
        experiment_id: &str,
        run_id: &str,
        name: impl Into<String>,
        value: f64,
    ) -> Result<()> {
        let mut exps = self.experiments.lock().unwrap();
        let run = find_run_mut(&mut exps, experiment_id, run_id)?;
        run.metrics.insert(name.into(), value);
        Ok(())
    }

    /// Record feature importances for a run
    pub fn log_feature_importances(
        &self,
        experiment_id: &str,
        run_id: &str,
        importances: Vec<FeatureImportance>,
    ) -> Result<()> {
        let mut exps = self.experiments.lock().unwrap();
        let run = find_run_mut(&mut exps, experiment_id, run_id)?;
        run.feature_importances = importances;
        Ok(())
    }

    /// Finish a run, marking it as completed or failed
    pub fn finish_run(
        &self,
        experiment_id: &str,
        run_id: &str,
        status: RunStatus,
        notes: impl Into<String>,
    ) -> Result<()> {
        let mut exps = self.experiments.lock().unwrap();
        let run = find_run_mut(&mut exps, experiment_id, run_id)?;
        run.status = status;
        run.ended_at = Some(Utc::now());
        run.notes = notes.into();
        Ok(())
    }

    /// Retrieve a full experiment record
    pub fn get_experiment(&self, experiment_id: &str) -> Option<MlExperiment> {
        self.experiments
            .lock()
            .unwrap()
            .get(experiment_id)
            .cloned()
    }

    /// List all experiments (id, name)
    pub fn list_experiments(&self) -> Vec<(String, String)> {
        self.experiments
            .lock()
            .unwrap()
            .values()
            .map(|e| (e.experiment_id.clone(), e.name.clone()))
            .collect()
    }

    /// Compare runs within an experiment by a metric, returning them sorted descending
    pub fn compare_runs(&self, experiment_id: &str, metric: &str) -> Vec<(String, f64)> {
        let exps = self.experiments.lock().unwrap();
        let Some(experiment) = exps.get(experiment_id) else {
            return vec![];
        };
        let mut pairs: Vec<(String, f64)> = experiment
            .runs
            .iter()
            .filter_map(|r| r.metrics.get(metric).map(|&v| (r.run_id.clone(), v)))
            .collect();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs
    }

    // ----- Model registry --------------------------------------------------

    /// Register a new model version
    pub fn register_model(
        &self,
        model_id: impl Into<String>,
        version: u32,
        artifact_path: impl Into<String>,
        framework: impl Into<String>,
        metrics: HashMap<String, f64>,
        feature_importances: Vec<FeatureImportance>,
        run_id: Option<String>,
    ) -> Result<()> {
        let model_id = model_id.into();
        let mv = ModelVersion {
            model_id: model_id.clone(),
            version,
            experiment_run_id: run_id,
            framework: framework.into(),
            artifact_path: artifact_path.into(),
            deployment_status: ModelDeploymentStatus::Staging,
            metrics,
            feature_importances,
            registered_at: Utc::now(),
            promoted_at: None,
            notes: String::new(),
        };
        self.models
            .lock()
            .unwrap()
            .entry(model_id.clone())
            .or_default()
            .push(mv);
        info!(model_id = %model_id, version = version, "Model version registered");
        Ok(())
    }

    /// Promote a specific model version to production and archive the previous one
    pub fn promote_to_production(&self, model_id: &str, version: u32) -> Result<()> {
        let mut models = self.models.lock().unwrap();
        let versions = models
            .get_mut(model_id)
            .ok_or_else(|| anyhow!("Model not found: {}", model_id))?;

        let mut found = false;
        for mv in versions.iter_mut() {
            if mv.version == version {
                mv.deployment_status = ModelDeploymentStatus::Production;
                mv.promoted_at = Some(Utc::now());
                found = true;
            } else if mv.deployment_status == ModelDeploymentStatus::Production {
                mv.deployment_status = ModelDeploymentStatus::Archived;
            }
        }
        if !found {
            return Err(anyhow!(
                "Model {} version {} not found",
                model_id,
                version
            ));
        }
        info!(model_id = %model_id, version = version, "Model promoted to production");
        Ok(())
    }

    /// Roll back production to the previous version
    pub fn rollback(&self, model_id: &str) -> Result<u32> {
        let mut models = self.models.lock().unwrap();
        let versions = models
            .get_mut(model_id)
            .ok_or_else(|| anyhow!("Model not found: {}", model_id))?;

        // Find currently production version
        let prod_version = versions
            .iter()
            .find(|mv| mv.deployment_status == ModelDeploymentStatus::Production)
            .map(|mv| mv.version);

        let Some(prod_ver) = prod_version else {
            return Err(anyhow!("No production version for model: {}", model_id));
        };

        // Mark current as rolled back, find previous archived version
        let prev_version = versions
            .iter()
            .filter(|mv| mv.deployment_status == ModelDeploymentStatus::Archived)
            .map(|mv| mv.version)
            .filter(|&v| v < prod_ver)
            .max();

        let Some(prev_ver) = prev_version else {
            return Err(anyhow!(
                "No previous version to roll back to for model: {}",
                model_id
            ));
        };

        for mv in versions.iter_mut() {
            if mv.version == prod_ver {
                mv.deployment_status = ModelDeploymentStatus::RolledBack;
            } else if mv.version == prev_ver {
                mv.deployment_status = ModelDeploymentStatus::Production;
                mv.promoted_at = Some(Utc::now());
            }
        }
        warn!(model_id = %model_id, rolled_back_from = prod_ver, rolled_back_to = prev_ver, "Model rollback performed");
        Ok(prev_ver)
    }

    /// Get all versions for a model
    pub fn get_model_versions(&self, model_id: &str) -> Vec<ModelVersion> {
        self.models
            .lock()
            .unwrap()
            .get(model_id)
            .cloned()
            .unwrap_or_default()
    }

    // ----- Leakage checks --------------------------------------------------

    /// Perform a simple forward-looking leakage check on a named feature set.
    ///
    /// Heuristic rules:
    /// - Features ending in `_future`, `_next`, `_lead` → high risk
    /// - Features ending in `_lag0` or `_t0` → low risk warning
    /// - Everything else → none
    pub fn check_leakage(&self, feature_names: &[&str]) -> Vec<LeakageCheckResult> {
        feature_names
            .iter()
            .map(|&name| {
                let lower = name.to_lowercase();
                if lower.ends_with("_future")
                    || lower.ends_with("_next")
                    || lower.ends_with("_lead")
                    || lower.contains("future_")
                {
                    LeakageCheckResult {
                        feature_name: name.to_string(),
                        risk: LeakageRisk::High,
                        reason: format!(
                            "Feature '{}' name suggests forward-looking data",
                            name
                        ),
                    }
                } else if lower.ends_with("_lag0") || lower.ends_with("_t0") {
                    LeakageCheckResult {
                        feature_name: name.to_string(),
                        risk: LeakageRisk::Low,
                        reason: format!(
                            "Feature '{}' may reference the current bar's value",
                            name
                        ),
                    }
                } else {
                    LeakageCheckResult {
                        feature_name: name.to_string(),
                        risk: LeakageRisk::None,
                        reason: String::new(),
                    }
                }
            })
            .collect()
    }

    // ----- Drift monitoring ------------------------------------------------

    /// Record a drift measurement.  Returns `true` if the model should be
    /// flagged for retraining (enough features have drifted).
    pub fn record_drift(&self, measurement: DriftMeasurement) -> bool {
        let mut log = self.drift_log.lock().unwrap();
        let is_drifted = measurement.is_drifted;
        log.push(measurement);
        drop(log);

        // Count recent drifted features
        let log = self.drift_log.lock().unwrap();
        let total = log.len();
        let drifted = log.iter().filter(|m| m.is_drifted).count();
        let fraction = if total > 0 {
            drifted as f64 / total as f64
        } else {
            0.0
        };

        if fraction >= self.config.drift_feature_fraction_alert {
            warn!(
                drifted = drifted,
                total = total,
                fraction = format!("{:.2}", fraction),
                "Significant feature drift detected — model retraining recommended"
            );
        }
        is_drifted
    }

    /// Compute a PSI-style drift measurement from two sets of observed values.
    ///
    /// Uses a simple 10-bucket histogram comparison.
    pub fn compute_psi(
        &self,
        feature_name: impl Into<String>,
        reference: &[f64],
        current: &[f64],
    ) -> DriftMeasurement {
        let feature_name = feature_name.into();
        if reference.is_empty() || current.is_empty() {
            return DriftMeasurement {
                feature_name,
                psi: 0.0,
                ks_stat: 0.0,
                is_drifted: false,
                measured_at: Utc::now(),
            };
        }

        let n_buckets = 10usize;
        let psi = compute_psi_value(reference, current, n_buckets);
        let ks = compute_ks(reference, current);
        let is_drifted =
            psi >= self.config.psi_threshold || ks >= self.config.ks_threshold;

        DriftMeasurement {
            feature_name,
            psi,
            ks_stat: ks,
            is_drifted,
            measured_at: Utc::now(),
        }
    }

    /// Get all recorded drift measurements
    pub fn drift_log(&self) -> Vec<DriftMeasurement> {
        self.drift_log.lock().unwrap().clone()
    }
}

impl Default for MlTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Statistical helpers
// ---------------------------------------------------------------------------

fn compute_psi_value(reference: &[f64], current: &[f64], n_buckets: usize) -> f64 {
    let min_val = reference
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .min(current.iter().cloned().fold(f64::INFINITY, f64::min));
    let max_val = reference
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(current.iter().cloned().fold(f64::NEG_INFINITY, f64::max));

    if (max_val - min_val).abs() < f64::EPSILON {
        return 0.0;
    }

    let bucket_width = (max_val - min_val) / n_buckets as f64;
    let ref_count = reference.len() as f64;
    let cur_count = current.len() as f64;

    let mut psi = 0.0f64;
    for i in 0..n_buckets {
        let lo = min_val + i as f64 * bucket_width;
        let hi = lo + bucket_width;
        let ref_frac = (reference.iter().filter(|&&v| v >= lo && v < hi).count() as f64
            / ref_count)
            .max(1e-6);
        let cur_frac = (current.iter().filter(|&&v| v >= lo && v < hi).count() as f64
            / cur_count)
            .max(1e-6);
        psi += (cur_frac - ref_frac) * (cur_frac / ref_frac).ln();
    }
    psi
}

fn compute_ks(a: &[f64], b: &[f64]) -> f64 {
    let mut a_sorted = a.to_vec();
    let mut b_sorted = b.to_vec();
    a_sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b_sorted.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    let n = a_sorted.len();
    let m = b_sorted.len();
    let mut ks = 0.0f64;
    let (mut i, mut j) = (0usize, 0usize);

    while i < n && j < m {
        let cdf_a = i as f64 / n as f64;
        let cdf_b = j as f64 / m as f64;
        ks = ks.max((cdf_a - cdf_b).abs());
        if a_sorted[i] <= b_sorted[j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    ks
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

fn find_run_mut<'a>(
    exps: &'a mut HashMap<String, MlExperiment>,
    experiment_id: &str,
    run_id: &str,
) -> Result<&'a mut ExperimentRun> {
    let experiment = exps
        .get_mut(experiment_id)
        .ok_or_else(|| anyhow!("Experiment not found: {}", experiment_id))?;
    experiment
        .runs
        .iter_mut()
        .find(|r| r.run_id == run_id)
        .ok_or_else(|| anyhow!("Run not found: {}", run_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> MlTracker {
        MlTracker::new()
    }

    #[test]
    fn test_create_experiment_and_run() {
        let tracker = default_tracker();
        let exp_id = tracker.create_experiment("test_exp", "A test", HashMap::new());
        let run_id = tracker
            .start_run(&exp_id, HashMap::from([("lr".into(), 0.01)]), HashMap::new())
            .unwrap();
        tracker
            .log_metric(&exp_id, &run_id, "accuracy", 0.85)
            .unwrap();
        tracker
            .finish_run(&exp_id, &run_id, RunStatus::Completed, "done")
            .unwrap();

        let exp = tracker.get_experiment(&exp_id).unwrap();
        assert_eq!(exp.runs.len(), 1);
        assert_eq!(exp.runs[0].metrics["accuracy"], 0.85);
        assert_eq!(exp.runs[0].status, RunStatus::Completed);
    }

    #[test]
    fn test_compare_runs_sorted() {
        let tracker = default_tracker();
        let exp_id = tracker.create_experiment("compare_test", "", HashMap::new());
        for acc in [0.7, 0.9, 0.8_f64] {
            let run_id = tracker.start_run(&exp_id, HashMap::new(), HashMap::new()).unwrap();
            tracker.log_metric(&exp_id, &run_id, "accuracy", acc).unwrap();
            tracker.finish_run(&exp_id, &run_id, RunStatus::Completed, "").unwrap();
        }
        let ranked = tracker.compare_runs(&exp_id, "accuracy");
        assert_eq!(ranked.len(), 3);
        assert!((ranked[0].1 - 0.9).abs() < 1e-9, "Best run should come first");
    }

    #[test]
    fn test_model_register_and_promote() {
        let tracker = default_tracker();
        tracker
            .register_model("strat_model", 1, "/models/v1", "scikit-learn", HashMap::new(), vec![], None)
            .unwrap();
        tracker
            .register_model("strat_model", 2, "/models/v2", "scikit-learn", HashMap::new(), vec![], None)
            .unwrap();
        tracker.promote_to_production("strat_model", 2).unwrap();

        let versions = tracker.get_model_versions("strat_model");
        let prod: Vec<_> = versions
            .iter()
            .filter(|v| v.deployment_status == ModelDeploymentStatus::Production)
            .collect();
        assert_eq!(prod.len(), 1);
        assert_eq!(prod[0].version, 2);
    }

    #[test]
    fn test_model_rollback() {
        let tracker = default_tracker();
        tracker
            .register_model("m", 1, "/m/v1", "keras", HashMap::new(), vec![], None)
            .unwrap();
        tracker
            .register_model("m", 2, "/m/v2", "keras", HashMap::new(), vec![], None)
            .unwrap();
        tracker.promote_to_production("m", 1).unwrap();
        tracker.promote_to_production("m", 2).unwrap();
        let rolled_to = tracker.rollback("m").unwrap();
        assert_eq!(rolled_to, 1);
    }

    #[test]
    fn test_leakage_check() {
        let tracker = default_tracker();
        let features = vec![
            "rsi_14",
            "close_future",
            "volume_lag0",
            "sma_20",
        ];
        let results = tracker.check_leakage(&features);
        let high: Vec<_> = results.iter().filter(|r| r.risk == LeakageRisk::High).collect();
        let low: Vec<_> = results.iter().filter(|r| r.risk == LeakageRisk::Low).collect();
        let none: Vec<_> = results.iter().filter(|r| r.risk == LeakageRisk::None).collect();
        assert_eq!(high.len(), 1);
        assert_eq!(low.len(), 1);
        assert_eq!(none.len(), 2);
    }

    #[test]
    fn test_psi_identical_distributions() {
        let tracker = default_tracker();
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let m = tracker.compute_psi("feat", &data, &data);
        assert!(m.psi < 0.01, "Identical distribution should have near-zero PSI");
        assert!(!m.is_drifted);
    }

    #[test]
    fn test_psi_very_different_distributions() {
        let tracker = default_tracker();
        let a: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let b: Vec<f64> = (200..300).map(|x| x as f64).collect();
        let m = tracker.compute_psi("feat", &a, &b);
        assert!(m.is_drifted, "Very different distributions should be flagged as drifted");
    }

    #[test]
    fn test_drift_record_and_retrieve() {
        let tracker = default_tracker();
        let m = DriftMeasurement {
            feature_name: "rsi_14".into(),
            psi: 0.05,
            ks_stat: 0.03,
            is_drifted: false,
            measured_at: Utc::now(),
        };
        tracker.record_drift(m);
        assert_eq!(tracker.drift_log().len(), 1);
    }
}
