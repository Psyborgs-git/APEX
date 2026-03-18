use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::domain::models::*;

/// RSS/News aggregator and sentiment analyzer
///
/// Capabilities:
/// - Fetch RSS/Atom feeds from multiple sources
/// - De-duplicate news items
/// - Extract ticker symbols from content
/// - Calculate sentiment scores
/// - Publish news items to message bus
pub struct NewsEngine {
    feeds: Arc<DashMap<String, NewsFeed>>,
    client: Client,
    news_cache: Arc<DashMap<String, NewsItem>>, // Key: URL
    news_tx: mpsc::UnboundedSender<NewsItem>,
}

/// Configuration for a news feed
#[derive(Debug, Clone)]
pub struct NewsFeed {
    pub name: String,
    pub url: String,
    pub feed_type: FeedType,
    pub priority: u8, // 1-10, higher = more important
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FeedType {
    Rss,
    Atom,
}

impl NewsEngine {
    /// Create a new news engine
    ///
    /// # Arguments
    /// * `news_tx` - Channel to publish news items to message bus
    pub fn new(news_tx: mpsc::UnboundedSender<NewsItem>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            feeds: Arc::new(DashMap::new()),
            client,
            news_cache: Arc::new(DashMap::new()),
            news_tx,
        }
    }

    /// Add a news feed to monitor
    pub fn add_feed(&self, feed: NewsFeed) {
        let feed_name = feed.name.clone();
        self.feeds.insert(feed_name.clone(), feed);
        info!("Added news feed: {}", feed_name);
    }

    /// Remove a news feed
    pub fn remove_feed(&self, name: &str) {
        self.feeds.remove(name);
        info!("Removed news feed: {}", name);
    }

    /// Enable/disable a feed
    pub fn set_feed_enabled(&self, name: &str, enabled: bool) {
        if let Some(mut feed) = self.feeds.get_mut(name) {
            feed.enabled = enabled;
            info!("Set feed '{}' enabled={}", name, enabled);
        }
    }

    /// List all configured feeds
    pub fn list_feeds(&self) -> Vec<NewsFeed> {
        self.feeds.iter().map(|e| e.value().clone()).collect()
    }

    /// Fetch and parse an RSS feed
    async fn fetch_rss_feed(&self, feed: &NewsFeed) -> Result<Vec<NewsItem>> {
        let response = self.client
            .get(&feed.url)
            .send()
            .await
            .context("Failed to fetch RSS feed")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
        }

        let content = response.text().await?;

        // Simple RSS parsing (in production, use a proper RSS parser like `rss` crate)
        self.parse_rss_content(&content, &feed.name)
    }

    /// Simple RSS/Atom content parser
    /// Note: In production, use the `rss` or `atom_syndication` crates
    fn parse_rss_content(&self, content: &str, source: &str) -> Result<Vec<NewsItem>> {
        let mut items = Vec::new();

        // Extremely simplified parsing - just extract title, link, description
        // In production, use proper XML parsing

        for line in content.lines() {
            let line = line.trim();

            // Look for <item> or <entry> blocks
            if line.contains("<title>") && line.contains("</title>") {
                let title = self.extract_xml_tag_content(line, "title");

                let url = format!("http://example.com/{}", Uuid::new_v4()); // Placeholder

                let item = NewsItem {
                    id: Uuid::new_v4(),
                    headline: title,
                    summary: String::new(),
                    source: source.to_string(),
                    url,
                    published: Utc::now(),
                    symbols: vec![],
                    sentiment: None,
                };

                items.push(item);
            }
        }

        Ok(items)
    }

    /// Extract content from XML tag
    fn extract_xml_tag_content(&self, line: &str, tag: &str) -> String {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);

        if let Some(start_pos) = line.find(&start_tag) {
            if let Some(end_pos) = line.find(&end_tag) {
                let content_start = start_pos + start_tag.len();
                return line[content_start..end_pos].to_string();
            }
        }

        String::new()
    }

    /// Extract ticker symbols from text using simple heuristics
    fn extract_symbols(&self, text: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Look for patterns like $AAPL, #MSFT, or standalone uppercase words
        let words: Vec<&str> = text.split_whitespace().collect();

        for word in words {
            if word.starts_with('$') && word.len() > 2 {
                let symbol = word[1..].trim_end_matches(|c: char| !c.is_alphanumeric());
                if !symbol.is_empty() {
                    symbols.push(Symbol(symbol.to_uppercase()));
                }
            } else if word.len() >= 2 && word.chars().all(|c| c.is_uppercase() || c == '.') {
                // Potential ticker (all uppercase, 2-5 chars)
                if word.len() <= 5 {
                    symbols.push(Symbol(word.to_string()));
                }
            }
        }

        symbols
    }

    /// Calculate sentiment score using simple keyword matching
    /// Returns -1.0 (bearish) to +1.0 (bullish)
    fn calculate_sentiment(&self, text: &str) -> f32 {
        let text_lower = text.to_lowercase();

        // Positive keywords
        let positive_keywords = [
            "bullish", "gains", "surge", "rally", "profit", "growth",
            "upgrade", "outperform", "buy", "strong", "positive",
        ];

        // Negative keywords
        let negative_keywords = [
            "bearish", "losses", "crash", "decline", "loss", "downgrade",
            "sell", "underperform", "weak", "negative", "warning",
        ];

        let mut score = 0;

        for keyword in &positive_keywords {
            if text_lower.contains(keyword) {
                score += 1;
            }
        }

        for keyword in &negative_keywords {
            if text_lower.contains(keyword) {
                score -= 1;
            }
        }

        // Normalize to [-1.0, 1.0]
        let max_keywords = positive_keywords.len().max(negative_keywords.len()) as f32;
        (score as f32 / max_keywords).clamp(-1.0, 1.0)
    }

    /// Process a news item: extract symbols, calculate sentiment
    fn enrich_news_item(&self, mut item: NewsItem) -> NewsItem {
        let combined_text = format!("{} {}", item.headline, item.summary);

        // Extract ticker symbols
        item.symbols = self.extract_symbols(&combined_text);

        // Calculate sentiment
        item.sentiment = Some(self.calculate_sentiment(&combined_text));

        item
    }

    /// Check if news item is duplicate (by URL)
    fn is_duplicate(&self, url: &str) -> bool {
        self.news_cache.contains_key(url)
    }

    /// Cache a news item
    fn cache_news_item(&self, item: &NewsItem) {
        self.news_cache.insert(item.url.clone(), item.clone());

        // Limit cache size to 10,000 items
        if self.news_cache.len() > 10_000 {
            // Remove oldest entries (simplified - just clear some)
            let keys_to_remove: Vec<String> = self.news_cache
                .iter()
                .take(1000)
                .map(|e| e.key().clone())
                .collect();

            for key in keys_to_remove {
                self.news_cache.remove(&key);
            }
        }
    }

    /// Fetch all enabled feeds and publish news items
    pub async fn fetch_all_feeds(&self) {
        let feeds: Vec<NewsFeed> = self.feeds
            .iter()
            .filter(|e| e.value().enabled)
            .map(|e| e.value().clone())
            .collect();

        for feed in feeds {
            match self.fetch_rss_feed(&feed).await {
                Ok(items) => {
                    debug!("Fetched {} items from {}", items.len(), feed.name);

                    for item in items {
                        // Skip duplicates
                        if self.is_duplicate(&item.url) {
                            continue;
                        }

                        // Enrich with symbols and sentiment
                        let enriched_item = self.enrich_news_item(item);

                        // Cache
                        self.cache_news_item(&enriched_item);

                        // Publish to message bus
                        if let Err(e) = self.news_tx.send(enriched_item.clone()) {
                            error!("Failed to publish news item: {}", e);
                        }

                        debug!("Published news: {}", enriched_item.headline);
                    }
                }
                Err(e) => {
                    error!("Failed to fetch feed '{}': {}", feed.name, e);
                }
            }
        }
    }

    /// Start continuous feed polling loop
    pub async fn start_polling_loop(self: Arc<Self>, poll_interval_secs: u64) {
        let mut ticker = interval(Duration::from_secs(poll_interval_secs));

        tokio::spawn(async move {
            loop {
                ticker.tick().await;
                self.fetch_all_feeds().await;
            }
        });

        info!("News engine polling loop started (interval: {}s)", poll_interval_secs);
    }

    /// Search cached news items
    pub fn search_news(
        &self,
        query: &str,
        symbol_filter: Option<&Symbol>,
        limit: usize,
    ) -> Vec<NewsItem> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<NewsItem> = self.news_cache
            .iter()
            .map(|e| e.value().clone())
            .filter(|item| {
                // Text search
                let matches_query = item.headline.to_lowercase().contains(&query_lower)
                    || item.summary.to_lowercase().contains(&query_lower);

                // Symbol filter
                let matches_symbol = symbol_filter.map_or(true, |sym| {
                    item.symbols.contains(sym)
                });

                matches_query && matches_symbol
            })
            .collect();

        // Sort by published date (newest first)
        results.sort_by(|a, b| b.published.cmp(&a.published));

        results.into_iter().take(limit).collect()
    }

    /// Get news items for a specific symbol
    pub fn get_news_for_symbol(&self, symbol: &Symbol, limit: usize) -> Vec<NewsItem> {
        let mut results: Vec<NewsItem> = self.news_cache
            .iter()
            .filter(|e| e.value().symbols.contains(symbol))
            .map(|e| e.value().clone())
            .collect();

        results.sort_by(|a, b| b.published.cmp(&a.published));
        results.into_iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_symbols() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let engine = NewsEngine::new(tx);

        let text = "Apple $AAPL is rallying while MSFT shows weakness";
        let symbols = engine.extract_symbols(text);

        assert!(symbols.contains(&Symbol("AAPL".to_string())));
        // Note: Simple implementation may not catch all patterns
    }

    #[test]
    fn test_sentiment_calculation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let engine = NewsEngine::new(tx);

        let positive_text = "Stock surges on strong earnings and bullish outlook";
        let negative_text = "Company crashes after bearish warning and losses";

        let pos_score = engine.calculate_sentiment(positive_text);
        let neg_score = engine.calculate_sentiment(negative_text);

        assert!(pos_score > 0.0, "Positive text should have positive sentiment");
        assert!(neg_score < 0.0, "Negative text should have negative sentiment");
    }

    #[test]
    fn test_feed_management() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let engine = NewsEngine::new(tx);

        let feed = NewsFeed {
            name: "Test Feed".to_string(),
            url: "http://example.com/rss".to_string(),
            feed_type: FeedType::Rss,
            priority: 5,
            enabled: true,
        };

        engine.add_feed(feed);

        let feeds = engine.list_feeds();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].name, "Test Feed");

        engine.remove_feed("Test Feed");
        assert_eq!(engine.list_feeds().len(), 0);
    }
}
