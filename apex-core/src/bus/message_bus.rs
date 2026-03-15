use crate::domain::models::*;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Buffer size for broadcast channels
const DEFAULT_CHANNEL_SIZE: usize = 4096;

/// Topics for the internal message bus
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    /// Tick data for a specific symbol
    Tick(String),
    /// Quote update for a specific symbol
    Quote(String),
    /// Order status update for a specific order
    OrderUpdate(String),
    /// Position updates
    PositionUpdate,
    /// News items
    NewsItem,
    /// Strategy signal for a specific strategy
    StrategySignal(String),
    /// Alert events
    Alert,
    /// System health events
    SystemHealth,
}

/// Messages that flow through the bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusMessage {
    TickData(Tick),
    QuoteData(Quote),
    OrderData(Order),
    PositionData(Position),
    News(crate::domain::models::NewsItem),
    Signal(TradingSignal),
    AlertFired(AlertMessage),
    Health(HealthMessage),
}

/// Alert message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertMessage {
    pub rule_id: String,
    pub message: String,
    pub severity: AlertSeverity,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Health status message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMessage {
    pub adapter_id: String,
    pub status: String,
    pub message: Option<String>,
}

/// Topic-based pub/sub message bus using broadcast channels for fan-out
pub struct MessageBus {
    senders: DashMap<Topic, broadcast::Sender<BusMessage>>,
    channel_size: usize,
}

impl MessageBus {
    /// Create a new message bus with default channel size
    pub fn new() -> Self {
        Self {
            senders: DashMap::new(),
            channel_size: DEFAULT_CHANNEL_SIZE,
        }
    }

    /// Create a new message bus with a custom channel size
    pub fn with_capacity(channel_size: usize) -> Self {
        Self {
            senders: DashMap::new(),
            channel_size,
        }
    }

    /// Publish a message to a topic. Creates the channel if it doesn't exist.
    /// Returns the number of receivers that received the message.
    pub fn publish(&self, topic: Topic, message: BusMessage) -> usize {
        let sender = self
            .senders
            .entry(topic)
            .or_insert_with(|| broadcast::channel(self.channel_size).0);
        // If no receivers, send returns Err but that's OK
        sender.send(message).unwrap_or(0)
    }

    /// Subscribe to a topic. Creates the channel if it doesn't exist.
    pub fn subscribe(&self, topic: Topic) -> broadcast::Receiver<BusMessage> {
        let sender = self
            .senders
            .entry(topic)
            .or_insert_with(|| broadcast::channel(self.channel_size).0);
        sender.subscribe()
    }

    /// Get the number of active topics
    pub fn topic_count(&self) -> usize {
        self.senders.len()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_publish_subscribe_tick() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.subscribe(Topic::Tick("AAPL".into()));

        let tick = Tick {
            time: Utc::now(),
            symbol: Symbol("AAPL".into()),
            bid: 150.0,
            ask: 150.05,
            last: 150.02,
            volume: 100,
            source: "test".into(),
        };

        let count = bus.publish(
            Topic::Tick("AAPL".into()),
            BusMessage::TickData(tick.clone()),
        );
        assert_eq!(count, 1);

        let received: BusMessage = rx.recv().await.unwrap();
        if let BusMessage::TickData(t) = received {
            assert_eq!(t.symbol, Symbol("AAPL".into()));
            assert_eq!(t.bid, 150.0);
        } else {
            panic!("Expected TickData message");
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = Arc::new(MessageBus::new());
        let mut rx1 = bus.subscribe(Topic::Alert);
        let mut rx2 = bus.subscribe(Topic::Alert);

        let alert = AlertMessage {
            rule_id: "test".into(),
            message: "Price above threshold".into(),
            severity: AlertSeverity::Warning,
        };

        let count = bus.publish(Topic::Alert, BusMessage::AlertFired(alert));
        assert_eq!(count, 2);

        // Both receivers should get the message
        let _: BusMessage = rx1.recv().await.unwrap();
        let _: BusMessage = rx2.recv().await.unwrap();
    }

    #[tokio::test]
    async fn test_different_topics_isolated() {
        let bus = Arc::new(MessageBus::new());
        let mut rx_aapl = bus.subscribe(Topic::Tick("AAPL".into()));
        let _rx_goog = bus.subscribe(Topic::Tick("GOOG".into()));

        let tick = Tick {
            time: Utc::now(),
            symbol: Symbol("AAPL".into()),
            bid: 150.0,
            ask: 150.05,
            last: 150.02,
            volume: 100,
            source: "test".into(),
        };

        bus.publish(Topic::Tick("AAPL".into()), BusMessage::TickData(tick));

        // AAPL subscriber should get the message
        let result: Result<BusMessage, _> = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx_aapl.recv(),
        )
        .await
        .expect("timed out waiting for message");
        assert!(result.is_ok());
    }

    #[test]
    fn test_publish_without_subscribers() {
        let bus = MessageBus::new();
        // Publishing to a topic with no subscribers should return 0
        let count = bus.publish(
            Topic::SystemHealth,
            BusMessage::Health(HealthMessage {
                adapter_id: "test".into(),
                status: "healthy".into(),
                message: None,
            }),
        );
        assert_eq!(count, 0);
    }

    #[test]
    fn test_topic_count() {
        let bus = MessageBus::new();
        assert_eq!(bus.topic_count(), 0);

        let _rx = bus.subscribe(Topic::Alert);
        assert_eq!(bus.topic_count(), 1);

        let _rx2 = bus.subscribe(Topic::Tick("AAPL".into()));
        assert_eq!(bus.topic_count(), 2);
    }
}
