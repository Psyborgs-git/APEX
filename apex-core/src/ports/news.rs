use crate::domain::models::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

/// Stream of news items from a feed
pub type NewsStream = mpsc::Receiver<NewsItem>;

/// Port trait for news feeds
#[async_trait]
pub trait NewsPort: Send + Sync {
    /// Subscribe to news updates with given filters
    async fn subscribe(&self, filters: NewsFilter) -> Result<NewsStream>;
    /// Search historical news
    async fn search(&self, query: &str, since: DateTime<Utc>, limit: usize) -> Result<Vec<NewsItem>>;
    /// Unique adapter identifier
    fn adapter_id(&self) -> &'static str;
}
