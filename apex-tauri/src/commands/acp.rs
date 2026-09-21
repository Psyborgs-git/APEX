//! ACP (Agent Client Protocol) connector — spawns a configured external
//! agent command (IDE, CLI agent) and speaks newline-delimited JSON-RPC
//! over its stdio: `initialize` → `session/new` → `session/prompt`, then
//! streams `session/update` notifications (`agent_message_chunk`) until
//! the prompt response resolves.

use crate::commands::copilot::CopilotMessage;
use crate::state::AppState;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::timeout;

const ACP_TIMEOUT: Duration = Duration::from_secs(180);

struct AcpClient {
    child: Child,
    stdin: ChildStdin,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: u64,
}

impl AcpClient {
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin
            .write_all(req.to_string().as_bytes())
            .await
            .and_then(|_| Ok(()))
            .map_err(|e| format!("ACP write failed: {e}"))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("ACP write failed: {e}"))?;
        Ok(id)
    }

    /// Read until the response matching `id` arrives; collects
    /// `agent_message_chunk` texts seen along the way.
    async fn await_response(&mut self, id: u64, chunks: &mut String) -> Result<Value, String> {
        loop {
            let line = timeout(ACP_TIMEOUT, self.lines.next_line())
                .await
                .map_err(|_| "ACP agent timed out".to_string())?
                .map_err(|e| format!("ACP read failed: {e}"))?
                .ok_or_else(|| "ACP agent closed its output".to_string())?;
            let msg: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue, // agents may print non-JSON chatter on stdout
            };

            // session/update notification → accumulate streamed text
            if msg.get("method").and_then(|m| m.as_str()) == Some("session/update") {
                let update = &msg["params"]["update"];
                let kind = update.get("sessionUpdate").and_then(|v| v.as_str());
                if matches!(kind, Some("agent_message_chunk") | Some("agent_thought_chunk")) {
                    if let Some(t) = update["content"]["text"].as_str() {
                        if kind == Some("agent_message_chunk") {
                            chunks.push_str(t);
                        }
                    }
                }
                continue;
            }

            // Any other request (e.g. fs/read_text_file, permission) →
            // reply with a polite refusal so the agent doesn't hang.
            if msg.get("id").is_some() && msg.get("method").is_some() {
                let rid = msg["id"].clone();
                let reply = json!({"jsonrpc": "2.0", "id": rid,
                    "error": {"code": -32601, "message": "unsupported by APEX ACP client"}});
                let _ = self.stdin.write_all(reply.to_string().as_bytes()).await;
                let _ = self.stdin.write_all(b"\n").await;
                continue;
            }

            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(format!("ACP error: {}", err));
                }
                return Ok(msg.get("result").cloned().unwrap_or(json!({})));
            }
        }
    }
}

/// Send `message` (plus short history) to the configured ACP agent and
/// return the assembled reply text.
pub(crate) async fn acp_prompt(
    state: &AppState,
    message: &str,
    history: &[CopilotMessage],
) -> Result<String, String> {
    let command = state.acp.command.trim();
    if command.is_empty() {
        return Err(
            "ACP provider selected but no agent command configured — set acp.command in Settings"
                .to_string(),
        );
    }

    let cwd = if state.acp.cwd.trim().is_empty() {
        std::env::temp_dir()
    } else {
        std::path::PathBuf::from(state.acp.cwd.trim())
    };

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn ACP agent `{command}`: {e}"))?;

    let stdin = child.stdin.take().ok_or("ACP agent has no stdin")?;
    let stdout = child.stdout.take().ok_or("ACP agent has no stdout")?;
    let mut client = AcpClient {
        child,
        stdin,
        lines: BufReader::new(stdout).lines(),
        next_id: 1,
    };

    let run = async {
        let mut chunks = String::new();

        let init_id = client
            .send(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": {"readTextFile": false, "writeTextFile": false},
                        "terminal": false,
                    },
                }),
            )
            .await?;
        client.await_response(init_id, &mut chunks).await?;

        let new_id = client
            .send(
                "session/new",
                json!({"cwd": cwd.to_string_lossy(), "mcpServers": []}),
            )
            .await?;
        let session_id = client
            .await_response(new_id, &mut chunks)
            .await?
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if session_id.is_empty() {
            return Err("ACP agent returned no session id".to_string());
        }

        // Fold recent history into a single prompt turn.
        let mut text = String::new();
        for m in history.iter().rev().take(8).rev() {
            text.push_str(&format!("{}: {}\n", m.role, m.content));
        }
        text.push_str(&format!("user: {message}"));

        let prompt_id = client
            .send(
                "session/prompt",
                json!({
                    "sessionId": session_id,
                    "prompt": [{"type": "text", "text": text}],
                }),
            )
            .await?;
        client.await_response(prompt_id, &mut chunks).await?;
        Ok::<String, String>(chunks)
    }
    .await;

    let _ = client.child.kill().await;
    run
}
