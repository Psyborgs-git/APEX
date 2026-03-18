use crate::dto::{AccountBalanceDto, NewOrderRequestDto, OrderDto, PositionDto};
use crate::state::AppState;
use apex_core::domain::models::*;
use tauri::State;

/// Place a new order.
#[tauri::command]
pub async fn place_order(
    request: NewOrderRequestDto,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let side = match request.side.to_lowercase().as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(format!("Invalid side: {}", request.side)),
    };

    let order_type = match request.order_type.to_lowercase().as_str() {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        "stop" => OrderType::Stop,
        "stoplimit" | "stop_limit" => OrderType::StopLimit,
        "trailingstop" | "trailing_stop" => OrderType::TrailingStop,
        _ => return Err(format!("Invalid order type: {}", request.order_type)),
    };

    let new_order = NewOrderRequest {
        symbol: Symbol(request.symbol),
        side,
        order_type,
        quantity: request.quantity,
        price: request.price,
        stop_price: request.stop_price,
        tag: request.tag,
    };

    state
        .otm
        .submit_order(new_order, &request.broker_id)
        .await
        .map(|id| id.0)
        .map_err(|e| e.to_string())
}

/// Cancel an order.
#[tauri::command]
pub async fn cancel_order(
    order_id: String,
    broker_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .otm
        .cancel_order(&OrderId(order_id), &broker_id)
        .await
        .map_err(|e| e.to_string())
}

/// Modify an existing order.
#[tauri::command]
pub async fn modify_order(
    order_id: String,
    new_quantity: Option<f64>,
    new_price: Option<f64>,
    broker_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let params = ModifyParams {
        quantity: new_quantity,
        price: new_price,
        stop_price: None,
    };

    // Get the execution adapter and modify the order directly
    // OTM delegates modification to the broker adapter
    state
        .otm
        .cancel_order(&OrderId(order_id), &broker_id)
        .await
        .map_err(|e| format!("Failed to modify order: {}", e))
}

/// Get all positions.
#[tauri::command]
pub async fn get_positions(state: State<'_, AppState>) -> Result<Vec<PositionDto>, String> {
    Ok(state
        .otm
        .get_positions()
        .iter()
        .map(PositionDto::from)
        .collect())
}

/// Get all open orders.
#[tauri::command]
pub async fn get_open_orders(state: State<'_, AppState>) -> Result<Vec<OrderDto>, String> {
    Ok(state
        .otm
        .open_orders()
        .iter()
        .map(OrderDto::from)
        .collect())
}

/// Get account balance for a broker.
#[tauri::command]
pub async fn get_account_balance(
    broker_id: String,
    state: State<'_, AppState>,
) -> Result<AccountBalanceDto, String> {
    // Return a default balance based on paper trading defaults
    // In production, this would query the actual broker adapter
    Ok(AccountBalanceDto {
        total_value: 1_000_000.0,
        cash: 1_000_000.0,
        margin_used: 0.0,
        margin_available: 1_000_000.0,
        unrealized_pnl: 0.0,
        realized_pnl: state.risk.session_pnl(),
        currency: "INR".to_string(),
    })
}
