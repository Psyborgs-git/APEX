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

    // --- P2: Instrument master & corporate actions -------------------------

    /// Upsert an instrument metadata record
    async fn upsert_instrument(&self, meta: &InstrumentMetadata) -> Result<()>;
    /// Look up an instrument by symbol
    async fn get_instrument(&self, symbol: &Symbol) -> Result<Option<InstrumentMetadata>>;
    /// Query instruments by arbitrary criteria
    async fn query_instruments(&self, params: InstrumentQuery) -> Result<Vec<InstrumentMetadata>>;

    /// Write a corporate action record
    async fn write_corporate_action(&self, action: &CorporateAction) -> Result<()>;
    /// Query corporate actions for a symbol, sorted chronologically
    async fn query_corporate_actions(&self, symbol: &Symbol) -> Result<Vec<CorporateAction>>;

    // --- P3: Strategy state persistence ------------------------------------

    /// Persist arbitrary strategy state as a JSON blob
    async fn save_strategy_state(
        &self,
        strategy_id: &str,
        state_json: &str,
    ) -> Result<()>;
    /// Load the most recent persisted state for a strategy
    async fn load_strategy_state(&self, strategy_id: &str) -> Result<Option<String>>;
}
