// The daily puzzle: picked from the local date, so everyone gets the same
// puzzle each day without needing an internet connection

use chrono::{Days, Local, NaiveDate};

use super::ladder::Difficulty;

pub const DIFFICULTY: Difficulty = Difficulty::Medium;
pub const LENGTH: usize = 4;

/// Daily puzzle #1
fn first_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn yesterday(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(1)).unwrap_or(date)
}

/// The puzzle number shown to players (#1 on launch day)
pub fn number(date: NaiveDate) -> i64 {
    (date - first_day()).num_days() + 1
}

pub fn seed(date: NaiveDate) -> u64 {
    // Offset so daily seeds don't overlap the 0..1,000,000 range of shared puzzle numbers
    1_000_000 + number(date).unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbering() {
        assert_eq!(number(first_day()), 1);
        assert_eq!(number(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()), 31);
        assert_eq!(number(NaiveDate::from_ymd_opt(2027, 10, 31).unwrap()), 366);
    }

    #[test]
    fn test_seeds_differ_by_day() {
        let a = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_ne!(seed(a), seed(yesterday(a)));
        assert_eq!(number(yesterday(a)), number(a) - 1);
        assert!(seed(a) >= 1_000_000);
    }
}
