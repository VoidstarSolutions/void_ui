//! Pure date arithmetic for the date-picker component.
//!
//! This module contains no masonry imports — it is pure chrono arithmetic only.
//!
//! `day_in_range` / `month_in_range` / `year_in_range` implement simple min/max
//! bounds. A general disabled-date `Matcher` trait is a known v1 limitation
//! intentionally left out of scope.

use chrono::{Datelike, Duration, NaiveDate};

/// Day-of-week header labels starting on Sunday.
pub(crate) const WEEKDAY_LABELS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

/// Number of years shown per page in the year-picker view.
pub(crate) const YEAR_PAGE_SIZE: i32 = 20;

/// Returns exactly 42 dates (6 full weeks) for the calendar grid of the given
/// month. Leading and trailing days from adjacent months are included so that
/// the grid always starts on a Sunday and ends on a Saturday.  The fixed 6-row
/// height prevents the popover from changing size as the user navigates months.
pub(crate) fn day_grid(year: i32, month: u32) -> [NaiveDate; 42] {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let days_from_sunday = first.weekday().num_days_from_sunday();
    let grid_start = first - Duration::days(i64::from(days_from_sunday));

    let mut grid = [NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(); 42];
    for (i, cell) in grid.iter_mut().enumerate() {
        // i is at most 41, so the cast to i64 is safe.
        *cell = grid_start + Duration::days(i64::try_from(i).unwrap());
    }
    grid
}

/// Adds `delta` months to the given `(year, month)`, handling rollover
/// correctly for both positive and negative deltas.
pub(crate) fn add_months(year: i32, month: u32, delta: i32) -> (i32, u32) {
    // month is always 1-12, so the conversion to i32 is safe.
    let total = (i32::try_from(month).unwrap() - 1) + delta;
    let year_offset = total.div_euclid(12);
    let result_month = total.rem_euclid(12);
    // result_month is 0-11 (rem_euclid guarantees non-negative), safe to cast to u32.
    (year + year_offset, u32::try_from(result_month + 1).unwrap())
}

/// Returns the full English name for a 1-indexed month number.
pub(crate) fn month_label(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Returns the year-page number that `year` belongs to.
/// Page 0 covers years 0–19, page 1 covers 20–39, page -1 covers -20..-1, etc.
pub(crate) fn year_page_of(year: i32) -> i32 {
    year.div_euclid(YEAR_PAGE_SIZE)
}

/// Returns all 20 years on the given page, in ascending order.
pub(crate) fn years_in_page(page: i32) -> [i32; 20] {
    let mut years = [0i32; 20];
    for (i, y) in years.iter_mut().enumerate() {
        // i is at most 19, so the conversion to i32 is safe.
        *y = page * YEAR_PAGE_SIZE + i32::try_from(i).unwrap();
    }
    years
}

/// Returns `true` if `date` falls within `[min, max]` (both bounds optional).
pub(crate) fn day_in_range(
    date: NaiveDate,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
) -> bool {
    min.is_none_or(|m| date >= m) && max.is_none_or(|m| date <= m)
}

/// Returns `true` if any day in the given month falls within `[min, max]`.
/// A month is considered out-of-range only when *every* day in it is excluded.
pub(crate) fn month_in_range(
    year: i32,
    month: u32,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
) -> bool {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    // Last day of the month: first day of next month minus one day.
    let (next_year, next_month) = add_months(year, month, 1);
    let last = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - Duration::days(1);

    // The month is out of range only if the entire [first, last] interval is
    // excluded — i.e. last < min or first > max.
    let below_min = min.is_some_and(|m| last < m);
    let above_max = max.is_some_and(|m| first > m);
    !below_min && !above_max
}

/// Returns `true` if any day in the given year falls within `[min, max]`.
/// A year is considered out-of-range only when *every* day in it is excluded.
pub(crate) fn year_in_range(year: i32, min: Option<NaiveDate>, max: Option<NaiveDate>) -> bool {
    let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let last = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();

    let below_min = min.is_some_and(|m| last < m);
    let above_max = max.is_some_and(|m| first > m);
    !below_min && !above_max
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── day_grid invariants ────────────────────────────────────────────────────

    #[test]
    fn day_grid_always_42_elements() {
        // Check a variety of months including edge cases
        for &(y, m) in &[(2024, 1), (2024, 2), (2024, 3), (2023, 12), (2000, 1)] {
            let grid = day_grid(y, m);
            assert_eq!(grid.len(), 42, "grid length for {y}-{m:02}");
        }
    }

    #[test]
    fn day_grid_first_element_is_sunday() {
        use chrono::Weekday;
        for &(y, m) in &[(2024, 1), (2024, 3), (2024, 6), (2023, 11)] {
            let grid = day_grid(y, m);
            assert_eq!(
                grid[0].weekday(),
                Weekday::Sun,
                "first cell not Sunday for {y}-{m:02}"
            );
        }
    }

    #[test]
    fn day_grid_march_2024_known_values() {
        // 2024-03-01 is a Friday; preceding Sunday is 2024-02-25.
        let grid = day_grid(2024, 3);
        assert_eq!(grid[0], NaiveDate::from_ymd_opt(2024, 2, 25).unwrap());
        // Last cell: 42 days from 2024-02-25 → 2024-04-06.
        assert_eq!(grid[41], NaiveDate::from_ymd_opt(2024, 4, 6).unwrap());
    }

    #[test]
    fn day_grid_contains_first_of_month() {
        let grid = day_grid(2024, 3);
        let first = NaiveDate::from_ymd_opt(2024, 3, 1).unwrap();
        assert!(grid.contains(&first));
    }

    // ── Leap year February ────────────────────────────────────────────────────

    #[test]
    fn day_grid_leap_year_february_has_29_feb_days() {
        let grid = day_grid(2024, 2);
        let count = grid
            .iter()
            .filter(|d| d.year() == 2024 && d.month() == 2)
            .count();
        assert_eq!(count, 29);
    }

    // ── Month rollover ────────────────────────────────────────────────────────

    #[test]
    fn add_months_forward_rollover() {
        assert_eq!(add_months(2023, 12, 1), (2024, 1));
    }

    #[test]
    fn add_months_backward_rollover() {
        assert_eq!(add_months(2024, 1, -1), (2023, 12));
    }

    #[test]
    fn add_months_no_rollover() {
        assert_eq!(add_months(2024, 6, 3), (2024, 9));
        assert_eq!(add_months(2024, 6, -3), (2024, 3));
    }

    // ── Negative year rollover ────────────────────────────────────────────────

    #[test]
    fn add_months_negative_year_rollover() {
        // Going back one month from year -1, January → year -2, December.
        assert_eq!(add_months(-1, 1, -1), (-2, 12));
    }

    // ── Year paging ───────────────────────────────────────────────────────────

    #[test]
    fn year_page_of_basics() {
        assert_eq!(year_page_of(0), 0);
        assert_eq!(year_page_of(19), 0);
        assert_eq!(year_page_of(20), 1);
        assert_eq!(year_page_of(-1), -1);
    }

    #[test]
    fn years_in_page_zero() {
        let page = years_in_page(0);
        // i is at most 19, so the conversion to i32 is safe.
        let expected: [i32; 20] = core::array::from_fn(|i| i32::try_from(i).unwrap());
        assert_eq!(page, expected);
    }

    #[test]
    fn years_in_page_negative_one() {
        let page = years_in_page(-1);
        assert_eq!(page[0], -20);
        assert_eq!(page[19], -1);
    }

    // ── Range helpers ─────────────────────────────────────────────────────────

    #[test]
    fn day_in_range_unbounded() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert!(day_in_range(date, None, None));
    }

    #[test]
    fn day_in_range_min_after_date() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let min = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        assert!(!day_in_range(date, Some(min), None));
    }

    #[test]
    fn day_in_range_max_before_date() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let max = NaiveDate::from_ymd_opt(2024, 5, 31).unwrap();
        assert!(!day_in_range(date, None, Some(max)));
    }

    #[test]
    fn day_in_range_within_bounds() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let min = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let max = NaiveDate::from_ymd_opt(2024, 6, 30).unwrap();
        assert!(day_in_range(date, Some(min), Some(max)));
    }

    #[test]
    fn month_in_range_partial_overlap_is_in_range() {
        // min is mid-June; June itself partially overlaps so it is in-range.
        let min = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert!(month_in_range(2024, 6, Some(min), None));
    }

    #[test]
    fn month_in_range_fully_before_min() {
        let min = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        assert!(!month_in_range(2024, 6, Some(min), None));
    }

    #[test]
    fn month_in_range_fully_after_max() {
        let max = NaiveDate::from_ymd_opt(2024, 5, 31).unwrap();
        assert!(!month_in_range(2024, 6, None, Some(max)));
    }

    #[test]
    fn year_in_range_partial_overlap() {
        // min is mid-year; the year itself is still in-range.
        let min = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert!(year_in_range(2024, Some(min), None));
    }

    #[test]
    fn year_in_range_fully_before_min() {
        let min = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        assert!(!year_in_range(2024, Some(min), None));
    }

    #[test]
    fn year_in_range_fully_after_max() {
        let max = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        assert!(!year_in_range(2024, None, Some(max)));
    }
}
