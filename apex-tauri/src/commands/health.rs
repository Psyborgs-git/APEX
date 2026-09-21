use crate::dto::{AdapterHealthDto, SystemHealthDto};
use crate::state::AppState;
use apex_core::ports::market_data::AdapterHealth;
use tauri::State;

fn adapter_health_dto(
    adapter_id: String,
    adapter_type: &str,
    health: AdapterHealth,
) -> AdapterHealthDto {
    let (status, message) = match health {
        AdapterHealth::Healthy => ("healthy".to_string(), "Connected".to_string()),
        AdapterHealth::Degraded(message) => ("degraded".to_string(), message),
        AdapterHealth::Unhealthy(message) => ("unhealthy".to_string(), message),
    };

    AdapterHealthDto {
        adapter_id,
        adapter_type: adapter_type.to_string(),
        status,
        message,
        last_check: chrono::Utc::now().to_rfc3339(),
    }
}

/// Get overall system health status including adapter statuses.
#[tauri::command]
pub async fn get_system_health(
    state: State<'_, AppState>,
) -> Result<SystemHealthDto, String> {
    let mut adapters: Vec<AdapterHealthDto> = state
        .aggregator
        .adapter_health()
        .into_iter()
        .map(|(adapter_id, health)| adapter_health_dto(adapter_id, "market_data", health))
        .collect();

    adapters.extend(
        state
            .otm
            .execution_health()
            .into_iter()
            .map(|(adapter_id, health)| adapter_health_dto(adapter_id, "execution", health)),
    );

    let open_orders = state
        .otm
        .open_orders()
        .iter()
        .count();

    let active_subs = state.aggregator.active_subscription_count();

    // Resident set size of this process in MB (Linux /proc; 0 elsewhere).
    let memory_usage_mb = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .map(|l| l.to_string())
        })
        .and_then(|l| {
            l.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(0);

    Ok(SystemHealthDto {
        adapters,
        uptime_secs: state.started_at.elapsed().as_secs(),
        memory_usage_mb,
        active_subscriptions: active_subs,
        open_orders,
        active_strategies: 0,
    })
}
