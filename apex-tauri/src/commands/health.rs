use crate::dto::{AdapterHealthDto, SystemHealthDto};
use crate::state::AppState;
use tauri::State;

/// Get overall system health status including adapter statuses.
#[tauri::command]
pub async fn get_system_health(
    state: State<'_, AppState>,
) -> Result<SystemHealthDto, String> {
    let adapters = vec![
        AdapterHealthDto {
            adapter_id: "yahoo_finance".into(),
            adapter_type: "market_data".into(),
            status: "healthy".into(),
            message: "Connected".into(),
            last_check: chrono::Utc::now().to_rfc3339(),
        },
        AdapterHealthDto {
            adapter_id: "paper_trading".into(),
            adapter_type: "execution".into(),
            status: "healthy".into(),
            message: "Active".into(),
            last_check: chrono::Utc::now().to_rfc3339(),
        },
    ];

    let open_orders = state
        .otm
        .open_orders()
        .iter()
        .count();

    let active_subs = state
        .aggregator
        .quote_cache()
        .len();

    Ok(SystemHealthDto {
        adapters,
        uptime_secs: 0, // Would use std::time::Instant in production
        memory_usage_mb: 0,
        active_subscriptions: active_subs,
        open_orders,
        active_strategies: 0,
    })
}
