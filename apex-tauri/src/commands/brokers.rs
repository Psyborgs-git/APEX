use crate::dto::BrokerConnectionDto;
use crate::state::AppState;
use crate::validation;
use serde::Deserialize;
use sha2::{Digest, Sha256};
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

/// Exchange a Zerodha Kite `request_token` (from the Kite web login
/// redirect `https://kite.trade/connect/login?api_key=…`) for a daily
/// `access_token` and install it on the Zerodha adapters.
///
/// Requires `ZERODHA_API_KEY` + `ZERODHA_API_SECRET` env vars (the secret
/// never leaves this call). The Kite checksum is
/// `sha256(api_key + request_token + api_secret)`.
#[tauri::command]
pub async fn zerodha_login(
    request_token: String,
    state: State<'_, AppState>,
) -> Result<BrokerConnectionDto, String> {
    let request_token = request_token.trim();
    if request_token.is_empty() || request_token.len() > 256 {
        return Err("Invalid request_token".to_string());
    }
    let api_key = std::env::var("ZERODHA_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            "ZERODHA_API_KEY env var not set — configure it and restart the app".to_string()
        })?;
    let api_secret = std::env::var("ZERODHA_API_SECRET")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            "ZERODHA_API_SECRET env var not set — required to exchange the request token"
                .to_string()
        })?;

    let checksum = hex::encode(
        Sha256::digest(format!("{}{}{}", api_key, request_token, api_secret).as_bytes()).to_vec(),
    );

    #[derive(Deserialize)]
    struct TokenResponse {
        data: Option<TokenData>,
        #[serde(default)]
        message: Option<String>,
    }
    #[derive(Deserialize)]
    struct TokenData {
        access_token: String,
    }

    let resp = state
        .http
        .post("https://api.kite.trade/session/token")
        .form(&[
            ("api_key", api_key.as_str()),
            ("request_token", request_token),
            ("checksum", checksum.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Kite session request failed: {e}"))?;

    let body: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Kite session response: {e}"))?;
    let token = body
        .data
        .map(|d| d.access_token)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            format!(
                "Kite login failed: {}",
                body.message
                    .unwrap_or_else(|| "no access_token returned".into())
            )
        })?;

    state
        .set_broker_session("zerodha", &token)
        .map_err(|e| e.to_string())?;
    state
        .broker_connection("zerodha")
        .ok_or_else(|| "Unknown broker: zerodha".to_string())
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
