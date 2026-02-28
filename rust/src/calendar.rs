use std::fmt;

use serde::{Serialize, Deserialize};

/// A game date with real-world calendar (year/month/day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameDate {
    pub year: u32,
    pub month: u32, // 1-12
    pub day: u32,   // 1-31
}

impl GameDate {
    pub fn new(year: u32, month: u32, day: u32) -> Self {
        debug_assert!(month >= 1 && month <= 12);
        debug_assert!(day >= 1 && day <= days_in_month(year, month));
        GameDate { year, month, day }
    }

    /// Default game start date.
    pub fn default_start() -> Self {
        GameDate::new(2001, 1, 1)
    }

    /// Advance to the next day.
    pub fn next_day(self) -> Self {
        let dim = days_in_month(self.year, self.month);
        if self.day < dim {
            GameDate { day: self.day + 1, ..self }
        } else if self.month < 12 {
            GameDate { month: self.month + 1, day: 1, ..self }
        } else {
            GameDate { year: self.year + 1, month: 1, day: 1 }
        }
    }

    /// True on the first day of any month.
    pub fn is_first_of_month(&self) -> bool {
        self.day == 1
    }

    /// Days in the current month.
    pub fn days_in_month(&self) -> u32 {
        days_in_month(self.year, self.month)
    }

    /// Day of year (Jan 1 = 1).
    pub fn day_of_year(&self) -> u32 {
        let mut total = 0;
        for m in 1..self.month {
            total += days_in_month(self.year, m);
        }
        total + self.day
    }

    /// Count of days between two dates (self must be <= other).
    pub fn days_until(&self, other: &GameDate) -> u32 {
        // Simple brute force — fine for game timescales
        let mut count = 0;
        let mut d = *self;
        while d < *other {
            d = d.next_day();
            count += 1;
        }
        count
    }

    /// Short month name.
    pub fn month_name(&self) -> &'static str {
        MONTH_NAMES[(self.month - 1) as usize]
    }
}

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl fmt::Display for GameDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}, {}", self.month_name(), self.day, self.year)
    }
}

/// Whether a year is a leap year.
pub fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Days in a given month of a given year.
pub fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 => 31,
        2 => if is_leap_year(year) { 29 } else { 28 },
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => panic!("Invalid month: {}", month),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_start() {
        let d = GameDate::default_start();
        assert_eq!(d.year, 2001);
        assert_eq!(d.month, 1);
        assert_eq!(d.day, 1);
    }

    #[test]
    fn test_next_day_normal() {
        let d = GameDate::new(2001, 1, 15);
        let n = d.next_day();
        assert_eq!(n, GameDate::new(2001, 1, 16));
    }

    #[test]
    fn test_next_day_end_of_month() {
        let d = GameDate::new(2001, 1, 31);
        assert_eq!(d.next_day(), GameDate::new(2001, 2, 1));
    }

    #[test]
    fn test_next_day_end_of_year() {
        let d = GameDate::new(2001, 12, 31);
        assert_eq!(d.next_day(), GameDate::new(2002, 1, 1));
    }

    #[test]
    fn test_next_day_feb_non_leap() {
        let d = GameDate::new(2001, 2, 28);
        assert_eq!(d.next_day(), GameDate::new(2001, 3, 1));
    }

    #[test]
    fn test_next_day_feb_leap() {
        let d = GameDate::new(2004, 2, 28);
        assert_eq!(d.next_day(), GameDate::new(2004, 2, 29));
        assert_eq!(d.next_day().next_day(), GameDate::new(2004, 3, 1));
    }

    #[test]
    fn test_leap_years() {
        assert!(is_leap_year(2000)); // divisible by 400
        assert!(!is_leap_year(1900)); // divisible by 100 but not 400
        assert!(is_leap_year(2004)); // divisible by 4
        assert!(!is_leap_year(2001)); // not divisible by 4
    }

    #[test]
    fn test_is_first_of_month() {
        assert!(GameDate::new(2001, 1, 1).is_first_of_month());
        assert!(GameDate::new(2001, 6, 1).is_first_of_month());
        assert!(!GameDate::new(2001, 1, 2).is_first_of_month());
    }

    #[test]
    fn test_day_of_year() {
        assert_eq!(GameDate::new(2001, 1, 1).day_of_year(), 1);
        assert_eq!(GameDate::new(2001, 2, 1).day_of_year(), 32);
        assert_eq!(GameDate::new(2001, 12, 31).day_of_year(), 365);
        assert_eq!(GameDate::new(2004, 12, 31).day_of_year(), 366); // leap year
    }

    #[test]
    fn test_display() {
        assert_eq!(GameDate::new(2001, 1, 1).to_string(), "Jan 1, 2001");
        assert_eq!(GameDate::new(2001, 12, 25).to_string(), "Dec 25, 2001");
    }

    #[test]
    fn test_days_until() {
        let a = GameDate::new(2001, 1, 1);
        let b = GameDate::new(2001, 1, 31);
        assert_eq!(a.days_until(&b), 30);

        // Full year
        let c = GameDate::new(2002, 1, 1);
        assert_eq!(a.days_until(&c), 365);

        // Leap year
        let d = GameDate::new(2004, 1, 1);
        let e = GameDate::new(2005, 1, 1);
        assert_eq!(d.days_until(&e), 366);
    }

    #[test]
    fn test_days_until_same_date() {
        let d = GameDate::new(2001, 6, 15);
        assert_eq!(d.days_until(&d), 0);
    }

    #[test]
    fn test_ordering() {
        let a = GameDate::new(2001, 1, 1);
        let b = GameDate::new(2001, 1, 2);
        let c = GameDate::new(2001, 2, 1);
        let d = GameDate::new(2002, 1, 1);
        assert!(a < b);
        assert!(b < c);
        assert!(c < d);
    }

    #[test]
    fn test_advance_full_year() {
        // Advance 365 days from Jan 1 non-leap year should land on Jan 1 next year
        let mut d = GameDate::new(2001, 1, 1);
        for _ in 0..365 {
            d = d.next_day();
        }
        assert_eq!(d, GameDate::new(2002, 1, 1));
    }
}
