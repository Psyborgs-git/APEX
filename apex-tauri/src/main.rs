mod commands;
mod config;
mod dto;
mod state;
mod tracing_setup;
mod validation;

use commands::{alerts, brokers, data, health, market, ml, notebook, orders, risk, settings};
use commands::strategy;
use tauri::Manager;

fn load_env_file() {
    let Ok(current_dir) = std::env::current_dir() else {
        return;
    };

    for dir in current_dir.ancestors() {
        let env_path = dir.join(".env");
        if env_path.exists() {
            let _ = dotenvy::from_path_override(env_path);
            break;
        }
    }
}

fn main() {
    load_env_file();
    tracing_setup::init();

    tracing::info!("APEX Terminal starting...");

    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let runtime_paths = commands::python_runtime::RuntimePaths::resolve(&app_handle)
                .expect("Runtime path resolution failed");
            let app_state_paths = runtime_paths.clone();
            
            tauri::async_runtime::block_on(async move {
                let app_state = state::AppState::init(app_state_paths).await.expect("App state initialization failed");
                
                tracing::info!("App state initialized successfully");
                tracing::info!(
                    "Risk engine: max_daily_loss = {}, halted = {}",
                    app_state.risk.config().max_daily_loss,
                    app_state.risk.is_halted()
                );

                // Start real-time event push from message bus → frontend
                app_state.start_event_push(app_handle.clone());
                
                app_handle.manage(app_state);
            });

            app.manage(runtime_paths.clone());

            // Register ML model registry state
            app.manage(ml::ModelRegistry::new(&runtime_paths));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            market::get_quote,
            market::get_ohlcv,
            market::subscribe_symbols,
            orders::place_order,
            orders::cancel_order,
            orders::modify_order,
            orders::get_positions,
            orders::get_open_orders,
            orders::get_account_balance,
            alerts::add_alert,
            alerts::remove_alert,
            alerts::get_alert_rules,
            risk::get_risk_status,
            risk::reset_halt,
            data::get_historical_data,
            data::get_watchlist_symbols,
            strategy::list_strategy_files,
            strategy::create_strategy_file,
            strategy::save_strategy_file,
            strategy::delete_strategy_file,
            strategy::run_strategy_file,
            strategy::run_strategy_backtest,
            ml::list_ml_models,
            ml::train_ml_model,
            ml::delete_ml_model,
            brokers::list_broker_connections,
            brokers::set_broker_session,
            brokers::clear_broker_session,
            health::get_system_health,
            settings::get_app_settings,
            settings::save_app_settings,
            notebook::list_notebooks,
            notebook::load_notebook,
            notebook::create_notebook,
            notebook::save_notebook,
            notebook::run_notebook_cell,
        ])
        .run(tauri::generate_context!())
        .expect("error while running APEX Terminal");
}
