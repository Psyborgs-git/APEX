use crate::dto::{NewOrderRequestDto, OrderDto, PositionDto};
use crate::state::AppState;
use apex_core::domain::models::*;

/// Place a new order.
pub async fn place_order(request: NewOrderRequestDto, state: &AppState) -> Result<String, String> {
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
pub async fn cancel_order(
    order_id: String,
    broker_id: String,
    state: &AppState,
) -> Result<(), String> {
    state
        .otm
        .cancel_order(&OrderId(order_id), &broker_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get all positions.
pub async fn get_positions(state: &AppState) -> Result<Vec<PositionDto>, String> {
    Ok(state
        .otm
        .get_positions()
        .iter()
        .map(PositionDto::from)
        .collect())
}

/// Get all open orders.
pub async fn get_open_orders(state: &AppState) -> Result<Vec<OrderDto>, String> {
    Ok(state
        .otm
        .open_orders()
        .iter()
        .map(OrderDto::from)
        .collect())
}
