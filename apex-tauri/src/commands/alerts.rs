use crate::state::AppState;
use apex_core::application::alert_engine::{AlertDelivery, AlertRule, StoredAlert};
use serde::Serialize;
use tauri::State;

/// Alert rule DTO for frontend.
#[derive(Debug, Clone, Serialize)]
pub struct AlertRuleDto {
    pub id: String,
    pub rule: String,
    pub enabled: bool,
}

/// Add a new alert rule.
#[tauri::command]
pub async fn add_alert(
    id: String,
    rule_json: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rule: AlertRule =
        serde_json::from_str(&rule_json).map_err(|e| format!("Invalid alert rule: {}", e))?;

    state
        .alerts
        .add_rule(StoredAlert {
            id,
            rule,
            delivery: vec![AlertDelivery::InApp],
            enabled: true,
        })
        .await;

    Ok(())
}

/// Remove an alert rule.
#[tauri::command]
pub async fn remove_alert(rule_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.alerts.remove_rule(&rule_id).await)
}

/// Get all alert rules.
#[tauri::command]
pub async fn get_alert_rules(state: State<'_, AppState>) -> Result<Vec<AlertRuleDto>, String> {
    let rules = state.alerts.get_rules().await;
    Ok(rules
        .into_iter()
        .map(|r| AlertRuleDto {
            id: r.id,
            rule: serde_json::to_string(&r.rule).unwrap_or_default(),
            enabled: r.enabled,
        })
        .collect())
}
