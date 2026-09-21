//! Automation engine — persisted, scheduled rules that run model signals
//! (or other actions) on an interval. The engine itself is a plain rule
//! store with due-time bookkeeping; the host app supplies the executor that
//! turns a due rule into a prediction + order.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a rule does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationKind {
    /// Compute the model's signal on `symbol` from the latest bars and place
    /// `quantity` on `broker_id` when confidence ≥ `threshold`.
    ModelSignal,
}

/// A scheduled automation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    pub kind: AutomationKind,
    pub symbol: String,
    /// Model registry id (for `ModelSignal`).
    pub model_id: String,
    /// Seconds between runs (min 30).
    pub interval_secs: u64,
    /// Order quantity when the signal fires.
    pub quantity: f64,
    /// Minimum class probability required to place the order (0–1).
    pub threshold: f64,
    pub broker_id: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    /// Human-readable outcome of the most recent run.
    pub last_result: Option<String>,
    /// UTC day (YYYY-MM-DD) `orders_today` counts against.
    #[serde(default)]
    pub orders_day: Option<String>,
    /// Orders successfully placed on `orders_day`.
    #[serde(default)]
    pub orders_today: u32,
}

impl AutomationRule {
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }
        match self.last_run_at {
            None => true,
            Some(last) => {
                (now - last).num_seconds() >= self.interval_secs.min(i64::MAX as u64) as i64
            }
        }
    }
}

/// File-backed rule store. Rules persist to `automations.json` in the app
/// data dir so they survive restarts.
pub struct AutomationEngine {
    rules: RwLock<HashMap<String, AutomationRule>>,
    persist_path: PathBuf,
}

impl AutomationEngine {
    pub fn new(persist_path: PathBuf) -> Self {
        let engine = Self {
            rules: RwLock::new(HashMap::new()),
            persist_path,
        };
        engine.load();
        engine
    }

    fn load(&self) {
        let raw = match std::fs::read_to_string(&self.persist_path) {
            Ok(r) => r,
            Err(_) => return,
        };
        match serde_json::from_str::<Vec<AutomationRule>>(&raw) {
            Ok(list) => {
                let mut rules = self.rules.write().unwrap();
                for r in list {
                    rules.insert(r.id.clone(), r);
                }
            }
            Err(e) => tracing::warn!(error = %e, "Ignoring malformed automations.json"),
        }
    }

    fn save(&self) {
        let rules = self.rules.read().unwrap();
        let list: Vec<&AutomationRule> = rules.values().collect();
        if let Some(parent) = self.persist_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(&list) {
            Ok(json) => {
                // Atomic-ish write: temp file + rename. On Windows, `rename`
                // refuses to overwrite an existing destination — remove it
                // first, and log every failure instead of swallowing it.
                let tmp = self.persist_path.with_extension("json.tmp");
                if let Err(e) = std::fs::write(&tmp, &json) {
                    tracing::warn!(error = %e, "Failed to write automations tmp file");
                    return;
                }
                if self.persist_path.exists() {
                    if let Err(e) = std::fs::remove_file(&self.persist_path) {
                        tracing::warn!(error = %e, "Failed to replace automations.json");
                        return;
                    }
                }
                if let Err(e) = std::fs::rename(&tmp, &self.persist_path) {
                    tracing::warn!(error = %e, "Failed to rename automations tmp file");
                }
            }
            Err(e) => tracing::warn!(error = %e, "Failed to serialize automations"),
        }
    }

    pub fn list(&self) -> Vec<AutomationRule> {
        let rules = self.rules.read().unwrap();
        let mut v: Vec<AutomationRule> = rules.values().cloned().collect();
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        v
    }

    pub fn get(&self, id: &str) -> Option<AutomationRule> {
        self.rules.read().unwrap().get(id).cloned()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_rule(
        &self,
        name: String,
        kind: AutomationKind,
        symbol: String,
        model_id: String,
        interval_secs: u64,
        quantity: f64,
        threshold: f64,
        broker_id: String,
    ) -> AutomationRule {
        let rule = AutomationRule {
            id: Uuid::new_v4().to_string(),
            name,
            kind,
            symbol,
            model_id,
            interval_secs: interval_secs.max(30),
            quantity,
            threshold: threshold.clamp(0.0, 1.0),
            broker_id,
            enabled: true,
            created_at: Utc::now(),
            last_run_at: None,
            last_result: None,
            orders_day: None,
            orders_today: 0,
        };
        self.rules
            .write()
            .unwrap()
            .insert(rule.id.clone(), rule.clone());
        self.save();
        rule
    }

    pub fn remove_rule(&self, id: &str) -> bool {
        let removed = self.rules.write().unwrap().remove(id).is_some();
        if removed {
            self.save();
        }
        removed
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut rules = self.rules.write().unwrap();
        match rules.get_mut(id) {
            Some(r) => {
                r.enabled = enabled;
                drop(rules);
                self.save();
                true
            }
            None => false,
        }
    }

    /// Ids of rules currently due to run.
    pub fn due_rules(&self) -> Vec<AutomationRule> {
        let now = Utc::now();
        self.rules
            .read()
            .unwrap()
            .values()
            .filter(|r| r.is_due(now))
            .cloned()
            .collect()
    }

    /// Whether the rule may place another order today (UTC), under `cap`.
    pub fn can_place_today(&self, id: &str, cap: u32) -> bool {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        match self.rules.read().unwrap().get(id) {
            Some(r) => r.orders_day.as_deref() != Some(today.as_str()) || r.orders_today < cap,
            None => false,
        }
    }

    /// Increment the rule's successful-order count for the current UTC day
    /// (resets when the day rolls over).
    pub fn record_order(&self, id: &str) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut rules = self.rules.write().unwrap();
        if let Some(r) = rules.get_mut(id) {
            if r.orders_day.as_deref() != Some(today.as_str()) {
                r.orders_day = Some(today);
                r.orders_today = 0;
            }
            r.orders_today += 1;
            drop(rules);
            self.save();
        }
    }

    /// Stamp a run outcome back onto the rule.
    pub fn record_run(&self, id: &str, result: String) {
        let mut rules = self.rules.write().unwrap();
        if let Some(r) = rules.get_mut(id) {
            r.last_run_at = Some(Utc::now());
            r.last_result = Some(result.chars().take(400).collect());
            drop(rules);
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (AutomationEngine, PathBuf) {
        let dir = std::env::temp_dir().join(format!("apex-auto-test-{}", Uuid::new_v4()));
        let path = dir.join("automations.json");
        (AutomationEngine::new(path.clone()), path)
    }

    #[test]
    fn add_list_remove() {
        let (e, path) = engine();
        let r = e.add_rule(
            "r1".into(),
            AutomationKind::ModelSignal,
            "AAPL".into(),
            "m1".into(),
            60,
            5.0,
            0.6,
            "paper".into(),
        );
        assert_eq!(e.list().len(), 1);
        assert!(path.exists());
        assert!(e.remove_rule(&r.id));
        assert!(e.list().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn due_scheduling() {
        let (e, path) = engine();
        let r = e.add_rule(
            "r".into(),
            AutomationKind::ModelSignal,
            "AAPL".into(),
            "m".into(),
            30,
            1.0,
            0.5,
            "paper".into(),
        );
        assert!(r.is_due(Utc::now()));
        e.record_run(&r.id, "ok".into());
        let stored = e.get(&r.id).unwrap();
        assert!(!stored.is_due(Utc::now()));
        assert_eq!(stored.last_result.as_deref(), Some("ok"));
        e.set_enabled(&r.id, false);
        assert!(e.due_rules().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn persists_across_instances() {
        let (e, path) = engine();
        e.add_rule(
            "persisted".into(),
            AutomationKind::ModelSignal,
            "MSFT".into(),
            "m".into(),
            120,
            2.0,
            0.5,
            "paper".into(),
        );
        let e2 = AutomationEngine::new(path.clone());
        assert_eq!(e2.list().len(), 1);
        assert_eq!(e2.list()[0].name, "persisted");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn daily_order_cap() {
        let (e, path) = engine();
        let r = e.add_rule(
            "capped".into(),
            AutomationKind::ModelSignal,
            "AAPL".into(),
            "m".into(),
            60,
            1.0,
            0.5,
            "paper".into(),
        );
        assert!(e.can_place_today(&r.id, 2));
        e.record_order(&r.id);
        e.record_order(&r.id);
        assert!(!e.can_place_today(&r.id, 2));
        // count survives reload
        let e2 = AutomationEngine::new(path.clone());
        assert!(!e2.can_place_today(&r.id, 2));
        assert!(e2.can_place_today(&r.id, 3));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
