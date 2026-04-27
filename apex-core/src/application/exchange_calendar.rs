use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::domain::models::{SessionType, TradingSession};

/// Market state at a particular point in time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketState {
    PreMarket,
    Open,
    Closed,
    PostMarket,
    Holiday(String),
}

/// An exchange calendar tracks trading hours, holidays, and current session
/// state for a given exchange.
pub struct ExchangeCalendar {
    exchange:  String,
    sessions:  Vec<TradingSession>,
    /// Set of dates (as ISO-8601 strings "YYYY-MM-DD") that are holidays
    holidays:  HashSet<String>,
}

impl ExchangeCalendar {
    /// Create a new calendar for the given exchange with no sessions or holidays.
    pub fn new(exchange: impl Into<String>) -> Self {
        Self {
            exchange:  exchange.into(),
            sessions:  Vec::new(),
            holidays:  HashSet::new(),
        }
    }

    /// Build a pre-configured NSE (National Stock Exchange, India) calendar.
    /// Regular session is 09:15 – 15:30 IST = 03:45 – 10:00 UTC Mon–Fri.
    pub fn nse() -> Self {
        let mut cal = Self::new("NSE");
        cal.add_session(TradingSession {
            exchange:     "NSE".into(),
            session_type: SessionType::RegularMarket,
            open_hhmm:    "03:45".into(),
            close_hhmm:   "10:00".into(),
            weekdays:     vec![1, 2, 3, 4, 5],
        });
        cal
    }

    /// Build a pre-configured NYSE calendar.
    /// Regular session is 09:30 – 16:00 ET = 14:30 – 21:00 UTC Mon–Fri.
    pub fn nyse() -> Self {
        let mut cal = Self::new("NYSE");
        cal.add_session(TradingSession {
            exchange:     "NYSE".into(),
            session_type: SessionType::PreMarket,
            open_hhmm:    "09:00".into(),
            close_hhmm:   "14:30".into(),
            weekdays:     vec![1, 2, 3, 4, 5],
        });
        cal.add_session(TradingSession {
            exchange:     "NYSE".into(),
            session_type: SessionType::RegularMarket,
            open_hhmm:    "14:30".into(),
            close_hhmm:   "21:00".into(),
            weekdays:     vec![1, 2, 3, 4, 5],
        });
        cal.add_session(TradingSession {
            exchange:     "NYSE".into(),
            session_type: SessionType::PostMarket,
            open_hhmm:    "21:00".into(),
            close_hhmm:   "01:00".into(),
            weekdays:     vec![1, 2, 3, 4, 5],
        });
        cal
    }

    /// Add a trading session to the calendar
    pub fn add_session(&mut self, session: TradingSession) {
        self.sessions.push(session);
    }

    /// Mark a date (YYYY-MM-DD in UTC) as a holiday
    pub fn add_holiday(&mut self, date: &str, _description: impl Into<String>) {
        self.holidays.insert(date.to_string());
    }

    /// Add multiple holidays from an iterator of (date, description) pairs
    pub fn add_holidays(
        &mut self,
        holidays: impl IntoIterator<Item = (String, String)>,
    ) {
        for (date, desc) in holidays {
            self.add_holiday(&date, desc);
        }
    }

    /// Determine the current market state for a given UTC timestamp.
    pub fn market_state_at(&self, now: DateTime<Utc>) -> MarketState {
        // Check holiday
        let date_str = now.format("%Y-%m-%d").to_string();
        if let Some(label) = self.holidays.get(&date_str) {
            return MarketState::Holiday(label.clone());
        }

        // Check sessions — later sessions override earlier ones
        let mut state = MarketState::Closed;
        for session in &self.sessions {
            if self.session_active(session, now) {
                state = match session.session_type {
                    SessionType::PreMarket    => MarketState::PreMarket,
                    SessionType::RegularMarket => MarketState::Open,
                    SessionType::PostMarket   |
                    SessionType::AfterHours   => MarketState::PostMarket,
                };
            }
        }
        state
    }

    /// Return true if the regular market is open at the given time.
    pub fn is_open(&self, now: DateTime<Utc>) -> bool {
        self.market_state_at(now) == MarketState::Open
    }

    /// Return true if the given date is a holiday on this calendar.
    pub fn is_holiday(&self, date: NaiveDate) -> bool {
        let key = date.format("%Y-%m-%d").to_string();
        self.holidays.contains(&key)
    }

    /// List all registered sessions
    pub fn sessions(&self) -> &[TradingSession] {
        &self.sessions
    }

    /// Exchange identifier
    pub fn exchange(&self) -> &str {
        &self.exchange
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn session_active(&self, session: &TradingSession, now: DateTime<Utc>) -> bool {
        // Check weekday
        let wd = iso_weekday(now.weekday());
        if !session.weekdays.contains(&wd) {
            return false;
        }

        // Parse open / close as UTC minutes-since-midnight
        let open_mins  = parse_hhmm(&session.open_hhmm);
        let close_mins = parse_hhmm(&session.close_hhmm);
        let now_mins   = now.hour() * 60 + now.minute();

        if close_mins > open_mins {
            // Same-day session
            now_mins >= open_mins && now_mins < close_mins
        } else {
            // Overnight session (e.g. post-market spanning midnight)
            now_mins >= open_mins || now_mins < close_mins
        }
    }
}

fn parse_hhmm(hhmm: &str) -> u32 {
    let parts: Vec<&str> = hhmm.split(':').collect();
    if parts.len() == 2 {
        let h: u32 = parts[0].parse().unwrap_or(0);
        let m: u32 = parts[1].parse().unwrap_or(0);
        h * 60 + m
    } else {
        0
    }
}

fn iso_weekday(wd: Weekday) -> u8 {
    match wd {
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
        Weekday::Sun => 7,
    }
}

/// Registry of named exchange calendars
pub struct CalendarRegistry {
    calendars: HashMap<String, ExchangeCalendar>,
}

impl Default for CalendarRegistry {
    fn default() -> Self {
        let mut reg = Self { calendars: HashMap::new() };
        reg.register(ExchangeCalendar::nse());
        reg.register(ExchangeCalendar::nyse());
        reg
    }
}

impl CalendarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a calendar (replaces any existing calendar with the same exchange id)
    pub fn register(&mut self, calendar: ExchangeCalendar) {
        self.calendars.insert(calendar.exchange.clone(), calendar);
    }

    /// Get a calendar by exchange identifier
    pub fn get(&self, exchange: &str) -> Option<&ExchangeCalendar> {
        self.calendars.get(exchange)
    }

    /// Check if a given exchange is open at the given time
    pub fn is_open(&self, exchange: &str, now: DateTime<Utc>) -> bool {
        self.get(exchange).map_or(false, |c| c.is_open(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_nse_open_during_regular_session() {
        let cal = ExchangeCalendar::nse();
        // Wednesday 2026-04-22 09:00 UTC = 14:30 IST — well within 03:45-10:00 UTC
        let t = Utc.with_ymd_and_hms(2026, 4, 22, 7, 0, 0).unwrap();
        assert!(cal.is_open(t));
    }

    #[test]
    fn test_nse_closed_outside_session() {
        let cal = ExchangeCalendar::nse();
        // Wednesday 2026-04-22 11:00 UTC — after 10:00 UTC close
        let t = Utc.with_ymd_and_hms(2026, 4, 22, 11, 0, 0).unwrap();
        assert!(!cal.is_open(t));
    }

    #[test]
    fn test_nse_closed_on_weekend() {
        let cal = ExchangeCalendar::nse();
        // Saturday
        let t = Utc.with_ymd_and_hms(2026, 4, 25, 6, 0, 0).unwrap();
        assert!(!cal.is_open(t));
    }

    #[test]
    fn test_holiday_overrides_session() {
        let mut cal = ExchangeCalendar::nse();
        cal.add_holiday("2026-04-22", "Test holiday");
        let t = Utc.with_ymd_and_hms(2026, 4, 22, 7, 0, 0).unwrap();
        assert!(!cal.is_open(t));
        match cal.market_state_at(t) {
            MarketState::Holiday(_) => {}
            other => panic!("Expected Holiday, got {:?}", other),
        }
    }

    #[test]
    fn test_nyse_pre_market() {
        let cal = ExchangeCalendar::nyse();
        // Monday 2026-04-20 10:00 UTC — within pre-market 09:00-14:30 UTC
        let t = Utc.with_ymd_and_hms(2026, 4, 20, 10, 0, 0).unwrap();
        assert_eq!(cal.market_state_at(t), MarketState::PreMarket);
    }

    #[test]
    fn test_calendar_registry_default() {
        let registry = CalendarRegistry::new();
        assert!(registry.get("NSE").is_some());
        assert!(registry.get("NYSE").is_some());
        assert!(registry.get("NONEXISTENT").is_none());
    }

    #[test]
    fn test_registry_is_open() {
        let registry = CalendarRegistry::new();
        // Wednesday 2026-04-22 07:00 UTC — NSE open
        let t = Utc.with_ymd_and_hms(2026, 4, 22, 7, 0, 0).unwrap();
        assert!(registry.is_open("NSE", t));
        assert!(!registry.is_open("UNKNOWN_EXCHANGE", t));
    }
}
