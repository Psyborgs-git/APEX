use crate::dto::RiskStatusDto;
use crate::state::AppState;

/// Get current risk status.
pub async fn get_risk_status(state: &AppState) -> Result<RiskStatusDto, String> {
    Ok(RiskStatusDto {
        session_pnl: state.risk.session_pnl(),
        is_halted: state.risk.is_halted(),
        max_daily_loss: state.risk.config().max_daily_loss,
    })
}

/// Reset the trading halt (explicit UI action only).
pub async fn reset_halt(state: &AppState) -> Result<(), String> {
    state.risk.reset_halt();
    Ok(())
}
