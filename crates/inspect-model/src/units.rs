//! The units of a `Duration`, in one table.
//!
//! Allium spells a duration `24.hours`, singular or plural, and lists
//! `.months` and `.years` among the units without saying how long either is.
//! A month is 28 to 31 days and a year 365 or 366 depending on which one, and
//! `now` here is a number the user advances, not a date — so neither has a
//! length this tool could use without guessing which month it is. They are
//! recognised, so what reads one can say *that* is why it stopped, instead of
//! "no default this simulator can read" (issue #41).
//!
//! The table was written twice, once for a config default and once for a
//! literal in a rule, and the two had drifted: the second de-pluralised `ms`
//! into `m` and did not know it.

/// Milliseconds in one `unit`, singular or plural; `None` for a unit with no
/// fixed length or none at all.
#[must_use]
pub fn millis(unit: &str) -> Option<i64> {
    match singular(unit) {
        "millisecond" | "ms" => Some(1),
        "second" | "sec" => Some(1_000),
        "minute" | "min" => Some(60_000),
        "hour" | "hr" => Some(3_600_000),
        "day" => Some(86_400_000),
        "week" => Some(604_800_000),
        _ => None,
    }
}

/// Whether `unit` is a calendar unit: a duration with no fixed length.
#[must_use]
pub fn is_calendar(unit: &str) -> bool {
    matches!(singular(unit), "month" | "year")
}

/// Whether `written` — `13.months`, `1_000.years` — is a whole number of a
/// calendar unit.
#[must_use]
pub fn is_calendar_duration(written: &str) -> bool {
    written.trim().split_once('.').is_some_and(|(amount, unit)| {
        amount.replace('_', "").parse::<i64>().is_ok() && is_calendar(unit)
    })
}

/// One `s` off, not every trailing `s`: `ms` is a unit in its own right, and
/// de-pluralising it leaves `m`, which is nothing.
fn singular(unit: &str) -> &str {
    if unit == "ms" { unit } else { unit.strip_suffix('s').unwrap_or(unit) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_reads_the_same_singular_or_plural() {
        assert_eq!(millis("day"), Some(86_400_000));
        assert_eq!(millis("days"), Some(86_400_000));
        assert_eq!(millis("weeks"), Some(604_800_000));
        assert_eq!(millis("seconds"), Some(1_000));
    }

    #[test]
    fn milliseconds_are_not_de_pluralised_into_nothing() {
        assert_eq!(millis("ms"), Some(1));
        assert_eq!(millis("milliseconds"), Some(1));
    }

    #[test]
    fn a_calendar_unit_has_no_length_but_is_recognised() {
        for unit in ["month", "months", "year", "years"] {
            assert_eq!(millis(unit), None, "{unit}");
            assert!(is_calendar(unit), "{unit}");
        }
        assert!(!is_calendar("days"));
        assert!(!is_calendar("fortnights"));
    }

    #[test]
    fn a_calendar_duration_is_a_whole_number_of_one() {
        assert!(is_calendar_duration("13.months"));
        assert!(is_calendar_duration(" 1_000.years "));
        assert!(!is_calendar_duration("396.days"));
        assert!(!is_calendar_duration("months"));
        assert!(!is_calendar_duration("x.months"));
    }
}
