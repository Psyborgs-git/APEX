use crate::state::AppState;
use crate::validation;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Deserialize)]
pub struct CopilotMessage {
    /// "user" or "assistant"
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopilotReply {
    pub reply: String,
    pub model: String,
}

fn copilot_api_key() -> Option<String> {
    // OPEN_ROUTER is the primary env var per repo convention; fall back to the
    // canonical OpenRouter name too.
    for name in ["OPEN_ROUTER", "OPENROUTER_API_KEY"] {
        if let Ok(raw) = std::env::var(name) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Chat with the AI copilot. Sends the message plus rolling history to
/// OpenRouter with a system prompt containing live terminal context
/// (positions, watchlist quotes, risk status).
#[tauri::command]
pub async fn copilot_chat(
    message: String,
    history: Option<Vec<CopilotMessage>>,
    state: State<'_, AppState>,
) -> Result<CopilotReply, String> {
    validation::validate_string_length(&message, "message")?;

    if !state.copilot.enabled {
        return Err("Copilot is disabled in configuration".to_string());
    }
    let api_key = copilot_api_key().ok_or_else(|| {
        "OpenRouter API key not configured — set the OPEN_ROUTER env var".to_string()
    })?;

    // Assemble live context for the system prompt.
    let positions = state.otm.get_positions();
    let quotes: Vec<String> = state
        .aggregator
        .quote_cache()
        .iter()
        .take(20)
        .map(|e| {
            let q = e.value();
            format!(
                "{} last={:.2} chg={:+.2}% vol={}",
                e.key(),
                q.last,
                q.change_pct,
                q.volume
            )
        })
        .collect();
    let pos_text: Vec<String> = positions
        .iter()
        .take(20)
        .map(|p| {
            format!(
                "{} qty={:.0} avg={:.2} pnl={:+.2}",
                p.symbol.0, p.quantity, p.avg_price, p.pnl
            )
        })
        .collect();

    let system_prompt = format!(
        "You are APEX Copilot, the assistant inside a local trading terminal. \
         Answer tersely, use the live context below when relevant, and never \
         invent prices. This is decision support, not financial advice.\n\n\
         LIVE WATCHLIST QUOTES:\n{}\n\nOPEN POSITIONS:\n{}\n\nSESSION P&L: {:.2}",
        if quotes.is_empty() {
            "(no quotes loaded)".to_string()
        } else {
            quotes.join("\n")
        },
        if pos_text.is_empty() {
            "(no open positions)".to_string()
        } else {
            pos_text.join("\n")
        },
        state.risk.session_pnl(),
    );

    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt,
    })];
    if let Some(history) = history {
        for m in history.iter().rev().take(20).rev() {
            let role = match m.role.as_str() {
                "assistant" => "assistant",
                _ => "user",
            };
            messages.push(serde_json::json!({"role": role, "content": m.content}));
        }
    }
    messages.push(serde_json::json!({"role": "user", "content": message}));

    let body = serde_json::json!({
        "model": state.copilot.model,
        "messages": messages,
        "max_tokens": state.copilot.max_tokens,
    });

    let resp = state
        .http
        .post(format!("{}/chat/completions", state.copilot.base_url))
        .bearer_auth(api_key)
        .header("HTTP-Referer", "https://apex.local")
        .header("X-Title", "APEX Terminal")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenRouter request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenRouter error {}: {}", status, text.chars().take(300).collect::<String>()));
    }

    #[derive(Deserialize)]
    struct Choice {
        message: Msg,
    }
    #[derive(Deserialize)]
    struct Msg {
        content: Option<String>,
    }
    #[derive(Deserialize)]
    struct ChatResp {
        choices: Vec<Choice>,
        #[serde(default)]
        model: Option<String>,
    }

    let parsed: ChatResp = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenRouter response: {}", e))?;

    let reply = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_else(|| "(empty response)".to_string());

    Ok(CopilotReply {
        reply,
        model: parsed
            .model
            .unwrap_or_else(|| state.copilot.model.clone()),
    })
}
