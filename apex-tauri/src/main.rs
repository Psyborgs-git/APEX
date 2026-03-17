mod commands;
mod dto;
mod state;

use commands::{alerts, market, orders, risk};
use tauri::Manager;
use tracing_subscriber::{fmt, EnvFilter};

fn main() {
    fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("apex=info".parse().expect("static directive must parse")),
        )
        .with_target(true)
        .init();

    tracing::info!("APEX Terminal starting...");

    tauri::Builder::default()
        .setup(|app| {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            let app_state = rt.block_on(async { state::AppState::init().await })?;

            tracing::info!("App state initialized successfully");
            tracing::info!(
                "Risk engine: max_daily_loss = {}, halted = {}",
                app_state.risk.config().max_daily_loss,
                app_state.risk.is_halted()
            );

            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            market::get_quote,
            market::subscribe_symbols,
            orders::place_order,
            orders::cancel_order,
            orders::get_positions,
            orders::get_open_orders,
            alerts::add_alert,
            alerts::remove_alert,
            risk::get_risk_status,
            risk::reset_halt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running APEX Terminal");
}
