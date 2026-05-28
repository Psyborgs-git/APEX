use crate::dto::BrokerConnectionDto;
use crate::state::AppState;
use crate::validation;
use tauri::State;

/// List all broker connection surfaces and their current readiness.
#[tauri::command]
pub async fn list_broker_connections(
    state: State<'_, AppState>,
) -> Result<Vec<BrokerConnectionDto>, String> {
    Ok(state.broker_connections())
}

/// Set or update a live broker session token/JWT at runtime.
#[tauri::command]
pub async fn set_broker_session(
    broker_id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<BrokerConnectionDto, String> {
    validation::validate_broker_id(&broker_id)?;
    validation::validate_session_token(&session_token)?;

    state
        .set_broker_session(&broker_id, session_token.trim())
        .map_err(|err| err.to_string())?;

    state
        .broker_connection(&broker_id)
        .ok_or_else(|| format!("Unknown broker: {}", broker_id))
}

/// Clear a live broker session token/JWT.
#[tauri::command]
pub async fn clear_broker_session(
    broker_id: String,
    state: State<'_, AppState>,
) -> Result<BrokerConnectionDto, String> {
    validation::validate_broker_id(&broker_id)?;

    state
        .clear_broker_session(&broker_id)
        .map_err(|err| err.to_string())?;

    state
        .broker_connection(&broker_id)
        .ok_or_else(|| format!("Unknown broker: {}", broker_id))
}