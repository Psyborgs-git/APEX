use crate::state::AppState;
use apex_core::application::alert_engine::{AlertDelivery, AlertRule, StoredAlert};

/// Add a new alert rule.
pub async fn add_alert(id: String, rule_json: String, state: &AppState) -> Result<(), String> {
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
pub async fn remove_alert(rule_id: String, state: &AppState) -> Result<bool, String> {
    Ok(state.alerts.remove_rule(&rule_id).await)
}
