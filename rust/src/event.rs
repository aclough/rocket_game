use std::collections::VecDeque;
use std::fmt;

use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;

/// Game events — informational records of things that happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    GameStarted,
    DayAdvanced,
    MonthStart,
    MoneyChanged { amount: f64, reason: String },
    // Future phases: LaunchResult, ContractOffered, FlawDiscovered, TechUnlocked, etc.
}

impl fmt::Display for GameEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameEvent::GameStarted => write!(f, "Company founded"),
            GameEvent::DayAdvanced => write!(f, "Day advanced"),
            GameEvent::MonthStart => write!(f, "New month"),
            GameEvent::MoneyChanged { amount, reason } => {
                if *amount >= 0.0 {
                    write!(f, "+${:.0}: {}", amount, reason)
                } else {
                    write!(f, "-${:.0}: {}", amount.abs(), reason)
                }
            }
        }
    }
}

/// How important an event is, for UI display and pause decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventImportance {
    /// Routine housekeeping (month ticks). Shown dim.
    Routine,
    /// Notable events worth reading. Shown bright.
    Notable,
    // Future: Critical — would auto-pause
}

impl GameEvent {
    pub fn importance(&self) -> EventImportance {
        match self {
            GameEvent::DayAdvanced | GameEvent::MonthStart => EventImportance::Routine,
            GameEvent::GameStarted
            | GameEvent::MoneyChanged { .. } => EventImportance::Notable,
        }
    }
}

/// A timestamped event log with a maximum size (ring buffer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog {
    events: VecDeque<(GameDate, GameEvent)>,
    max_size: usize,
}

impl EventLog {
    pub fn new(max_size: usize) -> Self {
        EventLog {
            events: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Push a new event. If at capacity, the oldest event is dropped.
    pub fn push(&mut self, date: GameDate, event: GameEvent) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back((date, event));
    }

    /// Get the N most recent events (newest first).
    pub fn recent(&self, n: usize) -> Vec<&(GameDate, GameEvent)> {
        self.events.iter().rev().take(n).collect()
    }

    /// Total number of events currently stored.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Iterate all events oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &(GameDate, GameEvent)> {
        self.events.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> GameDate {
        GameDate::new(2001, 1, day)
    }

    #[test]
    fn test_push_and_recent() {
        let mut log = EventLog::new(100);
        log.push(date(1), GameEvent::GameStarted);
        log.push(date(1), GameEvent::DayAdvanced);
        log.push(date(2), GameEvent::DayAdvanced);

        assert_eq!(log.len(), 3);

        let recent = log.recent(2);
        assert_eq!(recent.len(), 2);
        // Newest first
        assert_eq!(recent[0].0, date(2));
        assert_eq!(recent[1].0, date(1));
    }

    #[test]
    fn test_ring_buffer() {
        let mut log = EventLog::new(3);
        for d in 1..=5 {
            log.push(date(d), GameEvent::DayAdvanced);
        }
        assert_eq!(log.len(), 3);
        // Should have days 3, 4, 5
        let all: Vec<_> = log.iter().collect();
        assert_eq!(all[0].0, date(3));
        assert_eq!(all[2].0, date(5));
    }

    #[test]
    fn test_recent_more_than_available() {
        let mut log = EventLog::new(100);
        log.push(date(1), GameEvent::GameStarted);
        let recent = log.recent(10);
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn test_display_game_started() {
        assert_eq!(GameEvent::GameStarted.to_string(), "Company founded");
    }

    #[test]
    fn test_display_money_changed() {
        let e = GameEvent::MoneyChanged { amount: -50000.0, reason: "Salaries".into() };
        assert_eq!(e.to_string(), "-$50000: Salaries");

        let e2 = GameEvent::MoneyChanged { amount: 100000.0, reason: "Contract".into() };
        assert_eq!(e2.to_string(), "+$100000: Contract");
    }

    #[test]
    fn test_importance() {
        use super::EventImportance;
        assert_eq!(GameEvent::DayAdvanced.importance(), EventImportance::Routine);
        assert_eq!(GameEvent::MonthStart.importance(), EventImportance::Routine);
        assert_eq!(GameEvent::GameStarted.importance(), EventImportance::Notable);
        assert_eq!(GameEvent::MoneyChanged { amount: 0.0, reason: "test".into() }.importance(), EventImportance::Notable);
    }

    #[test]
    fn test_empty_log() {
        let log = EventLog::new(10);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        assert!(log.recent(5).is_empty());
    }
}
