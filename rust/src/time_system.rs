/// Time system for continuous gameplay simulation
/// Time advances at configurable rate while unpaused

/// Default simulation speed: 2 days per second
pub const DEFAULT_DAYS_PER_SECOND: f64 = 2.0;

/// Days per month for salary calculations
pub const DAYS_PER_MONTH: u32 = 30;

/// Time system state
#[derive(Debug, Clone)]
pub struct TimeSystem {
    /// Current game day (1-indexed)
    pub current_day: u32,
    /// Starting year for date display
    pub start_year: u32,
    /// Whether time is paused
    pub paused: bool,
    /// Fractional day accumulator (for smooth time advancement)
    pub fractional_day: f64,
    /// Simulation speed in days per second
    pub days_per_second: f64,
    /// Last day when salaries were deducted
    pub last_salary_day: u32,
}

impl TimeSystem {
    /// Create a new time system starting at day 1
    pub fn new() -> Self {
        Self {
            current_day: 1,
            start_year: 2001,
            paused: false,
            fractional_day: 0.0,
            days_per_second: DEFAULT_DAYS_PER_SECOND,
            last_salary_day: 1,
        }
    }

    /// Advance time by delta_seconds
    /// Returns the number of whole days that passed
    pub fn advance(&mut self, delta_seconds: f64) -> u32 {
        if self.paused || delta_seconds <= 0.0 {
            return 0;
        }

        // Add fractional time
        self.fractional_day += delta_seconds * self.days_per_second;

        // Extract whole days
        let days_passed = self.fractional_day.floor() as u32;
        self.fractional_day -= days_passed as f64;

        // Advance current day
        self.current_day += days_passed;

        days_passed
    }

    /// Toggle pause state
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Set pause state explicitly
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Check if salary is due and update last_salary_day if so
    /// Returns true if a month has passed since last salary payment
    pub fn check_salary_due(&mut self) -> bool {
        let days_since_salary = self.current_day.saturating_sub(self.last_salary_day);
        if days_since_salary >= DAYS_PER_MONTH {
            self.last_salary_day = self.current_day;
            true
        } else {
            false
        }
    }

    /// Get formatted date string (e.g., "Day 45, Year 2001")
    pub fn get_date_string(&self) -> String {
        let year = self.start_year + (self.current_day - 1) / 365;
        let day_of_year = ((self.current_day - 1) % 365) + 1;
        format!("Day {}, Year {}", day_of_year, year)
    }

    /// Get current year
    pub fn get_current_year(&self) -> u32 {
        self.start_year + (self.current_day - 1) / 365
    }

    /// Get day of the current year (1-365)
    pub fn get_day_of_year(&self) -> u32 {
        ((self.current_day - 1) % 365) + 1
    }

    /// Get days until next salary payment
    pub fn days_until_salary(&self) -> u32 {
        let days_since_salary = self.current_day.saturating_sub(self.last_salary_day);
        DAYS_PER_MONTH.saturating_sub(days_since_salary)
    }
}

impl Default for TimeSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_time_system() {
        let ts = TimeSystem::new();
        assert_eq!(ts.current_day, 1);
        assert_eq!(ts.start_year, 2001);
        assert!(!ts.paused);
        assert_eq!(ts.fractional_day, 0.0);
    }

    #[test]
    fn test_advance_time() {
        let mut ts = TimeSystem::new();

        // Advance 0.5 seconds at 2 days/second = 1 day
        let days = ts.advance(0.5);
        assert_eq!(days, 1);
        assert_eq!(ts.current_day, 2);

        // Advance 0.25 seconds = 0.5 days (no whole day)
        let days = ts.advance(0.25);
        assert_eq!(days, 0);
        assert_eq!(ts.current_day, 2);
        assert!((ts.fractional_day - 0.5).abs() < 0.001);

        // Advance another 0.25 seconds = 0.5 more days, now 1 whole day
        let days = ts.advance(0.25);
        assert_eq!(days, 1);
        assert_eq!(ts.current_day, 3);
    }

    #[test]
    fn test_pause() {
        let mut ts = TimeSystem::new();
        ts.toggle_pause();
        assert!(ts.paused);

        // Time should not advance while paused
        let days = ts.advance(10.0);
        assert_eq!(days, 0);
        assert_eq!(ts.current_day, 1);

        ts.toggle_pause();
        assert!(!ts.paused);

        // Time should advance now
        let days = ts.advance(0.5);
        assert_eq!(days, 1);
    }

    #[test]
    fn test_salary_due() {
        let mut ts = TimeSystem::new();
        assert!(!ts.check_salary_due()); // Day 1, just started

        ts.current_day = 30;
        assert!(!ts.check_salary_due()); // 29 days since salary

        ts.current_day = 31;
        assert!(ts.check_salary_due()); // 30 days since salary
        assert_eq!(ts.last_salary_day, 31);

        // Should not be due immediately after payment
        assert!(!ts.check_salary_due());
    }

    #[test]
    fn test_date_string() {
        let mut ts = TimeSystem::new();
        assert_eq!(ts.get_date_string(), "Day 1, Year 2001");

        ts.current_day = 365;
        assert_eq!(ts.get_date_string(), "Day 365, Year 2001");

        ts.current_day = 366;
        assert_eq!(ts.get_date_string(), "Day 1, Year 2002");
    }
}
