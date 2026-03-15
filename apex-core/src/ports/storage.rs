use crate::domain::models::*;
use anyhow::Result;
use async_trait::async_trait;

/// Port trait for data persistence
#[async_trait]
pub trait StoragePort: Send + Sync {
    /// Write a batch of ticks
    async fn write_ticks(&self, ticks: &[Tick]) -> Result<()>;
    /// Write a batch of OHLCV bars
    async fn write_ohlcv(&self, bars: &[OHLCV]) -> Result<()>;
    /// Query OHLCV data
    async fn query_ohlcv(&self, params: OHLCVQuery) -> Result<Vec<OHLCV>>;
    /// Write a new order record
    async fn write_order(&self, order: &Order) -> Result<()>;
    /// Update an existing order record
    async fn update_order(&self, order: &Order) -> Result<()>;
    /// Query orders
    async fn query_orders(&self, params: OrderQuery) -> Result<Vec<Order>>;
    /// Write/update a position
    async fn write_position(&self, pos: &Position) -> Result<()>;
    /// Query positions for a broker
    async fn query_positions(&self, broker_id: &str) -> Result<Vec<Position>>;
}
