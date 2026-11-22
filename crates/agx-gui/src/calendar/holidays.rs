//! Recurring nationwide U.S. federal holidays and their observed dates.
//! https://www.opm.gov/policy-data-oversight/pay-leave/federal-holidays/
use chrono::{Datelike, NaiveDate, Weekday};

pub(super) fn on(date: NaiveDate) -> Option<String> {
    let year = date.year();
    if year < 1971 {
        return None;
    }
    let fixed = [
        ("New Year's Day", year, 1, 1),
        ("Juneteenth", year, 6, 19),
        ("Independence Day", year, 7, 4),
        ("Veterans Day", year, 11, 11),
        ("Christmas Day", year, 12, 25),
        ("New Year's Day", year + 1, 1, 1),
    ];
    for (name, year, month, day) in fixed {
        if (month == 6 && year < 2021) || (month == 11 && year < 1978) {
            continue;
        }
        let holiday = NaiveDate::from_ymd_opt(year, month, day)?;
        if date == holiday {
            return Some(name.into());
        }
        let observed = match holiday.weekday() {
            Weekday::Sat => holiday.pred_opt(),
            Weekday::Sun => holiday.succ_opt(),
            _ => None,
        };
        if observed == Some(date) {
            return Some(format!("{name} (observed)"));
        }
    }

    let weekday = date.weekday();
    let week = (date.day() - 1) / 7 + 1;
    let name = match (date.month(), weekday, week) {
        (1, Weekday::Mon, 3) if year >= 1986 => "Martin Luther King Jr. Day",
        (2, Weekday::Mon, 3) => "Washington's Birthday",
        (5, Weekday::Mon, _) if date.day() >= 25 => "Memorial Day",
        (9, Weekday::Mon, 1) => "Labor Day",
        (10, Weekday::Mon, 2) => "Columbus Day",
        (10, Weekday::Mon, 4) if year < 1978 => "Veterans Day",
        (11, Weekday::Thu, 4) => "Thanksgiving Day",
        _ => return None,
    };
    Some(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_holidays_and_observances_in_the_correct_month() {
        let holiday = |year, month, day| on(NaiveDate::from_ymd_opt(year, month, day).unwrap());
        assert_eq!(holiday(2026, 11, 26).as_deref(), Some("Thanksgiving Day"));
        assert_eq!(
            holiday(2026, 7, 3).as_deref(),
            Some("Independence Day (observed)")
        );
        assert_eq!(holiday(2026, 7, 4).as_deref(), Some("Independence Day"));
        assert_eq!(
            holiday(2027, 12, 31).as_deref(),
            Some("New Year's Day (observed)")
        );
        assert_eq!(holiday(2020, 6, 19), None);
    }
}
