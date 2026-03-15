use crate::domain::models::*;
use anyhow::Result;
use async_trait::async_trait;

use super::market_data::AdapterHealth;

/// Port trait for order execution (broker adapters)
#[async_trait]
pub trait ExecutionPort: Send + Sync {
    /// Place a new order
    async fn place_order(&self, order: &NewOrderRequest) -> Result<OrderId>;
    /// Cancel an existing order
    async fn cancel_order(&self, order_id: &OrderId) -> Result<()>;
    /// Modify an existing order
    async fn modify_order(&self, order_id: &OrderId, params: &ModifyParams) -> Result<()>;
    /// Get the current status of an order
    async fn get_order_status(&self, order_id: &OrderId) -> Result<Order>;
    /// Get all current positions
    async fn get_positions(&self) -> Result<Vec<Position>>;
    /// Get account balance
    async fn get_account_balance(&self) -> Result<AccountBalance>;
    /// Unique broker identifier
    fn broker_id(&self) -> &'static str;
    /// Supported order types for this broker
    fn supported_order_types(&self) -> &[OrderType];
    /// Current health status
    fn health(&self) -> AdapterHealth;
}
