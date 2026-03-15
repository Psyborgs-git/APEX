mod commands;
mod dto;
mod state;

use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("apex=info".parse().expect("static directive must parse")),
        )
        .with_target(true)
        .init();

    tracing::info!("APEX Terminal starting...");

    let app_state = state::AppState::init().await?;

    tracing::info!("App state initialized successfully");
    tracing::info!(
        "Risk engine: max_daily_loss = {}, halted = {}",
        app_state.risk.config().max_daily_loss,
        app_state.risk.is_halted()
    );

    // In production, this would launch the Tauri window:
    // tauri::Builder::default()
    //     .manage(app_state)
    //     .invoke_handler(tauri::generate_handler![...])
    //     .run(tauri::generate_context!())
    //     .expect("error while running tauri application");

    tracing::info!("APEX Terminal ready. (Tauri UI will be enabled when tauri crate is added)");

    Ok(())
}
