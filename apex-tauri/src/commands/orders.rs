use crate::dto::{AccountBalanceDto, NewOrderRequestDto, OrderDto, PositionDto};
use crate::state::AppState;
use crate::validation;
use apex_core::domain::models::*;
use tauri::State;

/// Place a new order.
#[tauri::command]
pub async fn place_order(
    request: NewOrderRequestDto,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Input validation
    validation::validate_symbol(&request.symbol)?;
    validation::validate_quantity(request.quantity)?;
    validation::validate_price(request.price)?;
    validation::validate_price(request.stop_price)?;
    validation::validate_broker_id(&request.broker_id)?;
    if let Some(ref tag) = request.tag {
        validation::validate_string_length(tag, "tag")?;
    }

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
    validation::validate_string_length(&order_id, "order_id")?;
    validation::validate_broker_id(&broker_id)?;

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
    validation::validate_string_length(&order_id, "order_id")?;
    validation::validate_broker_id(&broker_id)?;
    if let Some(q) = new_quantity {
        validation::validate_quantity(q)?;
    }
    validation::validate_price(new_price)?;

    let params = ModifyParams {
        quantity: new_quantity,
        price: new_price,
        stop_price: None,
    };

    state
        .otm
        .modify_order(&OrderId(order_id), &broker_id, &params)
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
    Ok(state.otm.open_orders().iter().map(OrderDto::from).collect())
}

/// Get order history (all statuses, newest first) — backs the order blotter.
/// Falls back to the in-memory open-order map when storage has no rows yet,
/// so a fresh session still shows resting orders.
#[tauri::command]
pub async fn get_orders(
    symbol: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<OrderDto>, String> {
    if let Some(ref s) = symbol {
        validation::validate_symbol(s)?;
    }

    let mut orders = state
        .storage
        .query_orders(OrderQuery {
            symbol: symbol.clone().map(Symbol),
            status: None,
            broker_id: None,
            from: None,
            to: None,
            limit: Some(limit.unwrap_or(200).min(1000)),
        })
        .await
        .map_err(|e| format!("Failed to query orders: {}", e))?;

    // Merge in-memory open orders not yet flushed to storage.
    for open in state.otm.open_orders() {
        if !orders.iter().any(|o| o.id == open.id) {
            if symbol.as_ref().map(|s| open.symbol.0 == *s).unwrap_or(true) {
                orders.push(open);
            }
        }
    }
    orders.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    orders.truncate(limit.unwrap_or(200).min(1000));

    Ok(orders.iter().map(OrderDto::from).collect())
}

/// Get account balance for a broker.
#[tauri::command]
pub async fn get_account_balance(
    broker_id: String,
    state: State<'_, AppState>,
) -> Result<AccountBalanceDto, String> {
    validation::validate_broker_id(&broker_id)?;

    let balance = state
        .otm
        .get_account_balance(&broker_id)
        .await
        .map_err(|e| format!("Failed to get account balance: {}", e))?;

    Ok(AccountBalanceDto::from(&balance))
}
