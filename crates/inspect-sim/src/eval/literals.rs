//! Literals: numbers, durations and sets.
//!
//! Each of these is a small parse with one thing worth getting right, and each
//! of those things is a place where a plausible shortcut produces a wrong answer
//! that nothing would notice:
//!
//! - a number is reported as *text* so digit separators survive, and
//!   `2_000_000_000` parsed naively is not a number at all;
//! - a duration carries a unit, and `21.days` reduced to `21` compares equal to
//!   twenty-one of anything.
//!
//! Booleans used to be here too, for a third reason that turned out not to be
//! one: the JSON could carry either a real boolean or the word, so both were
//! handled. Typed, `BoolLiteral` holds a `bool` and there is nothing to parse.

use allium_parser::ast::Expr;

use super::Evaluation;
use crate::{eval::Env, value::Value};

/// `20`, `2_000_000_000`, `1.5`.
#[must_use]
pub fn number(written: &str, at: &Expr, env: &Env<'_>) -> Evaluation {
    let raw = written.replace('_', "");
    if raw.is_empty() {
        return Evaluation::unknown("a number literal with no digits", at, env.source);
    }
    if let Ok(whole) = raw.parse::<i64>() {
        return Evaluation::known(Value::Int(whole));
    }
    match raw.parse::<f64>() {
        Ok(decimal) => Evaluation::known(Value::Float(decimal)),
        Err(_) => Evaluation::unknown(format!("`{raw}` is not a number"), at, env.source),
    }
}

/// `21.days`, `200.seconds`, `24.hours`.
///
/// Reduced to milliseconds, which is the only unit the world's clock has. The
/// *kind* is kept as a duration rather than an integer, so that a spec
/// comparing a duration to a bare number is reported as incomparable instead of
/// quietly answered.
#[must_use]
pub fn duration(raw: &str, at: &Expr, env: &Env<'_>) -> Evaluation {
    let Some((amount, unit)) = raw.split_once('.') else {
        return Evaluation::unknown(format!("`{raw}` is not a duration"), at, env.source);
    };
    let Ok(amount) = amount.replace('_', "").parse::<i64>() else {
        return Evaluation::unknown(format!("`{raw}` has no amount"), at, env.source);
    };
    let Some(millis) = unit_millis(unit) else {
        return Evaluation::unknown(
            format!("`{unit}` is not a unit of time this evaluator knows"),
            at,
            env.source,
        );
    };
    match amount.checked_mul(millis) {
        Some(total) => Evaluation::known(Value::Duration(total)),
        None => Evaluation::unknown(format!("`{raw}` is too long to measure"), at, env.source),
    }
}

/// Milliseconds in one of `unit`, singular or plural.
fn unit_millis(unit: &str) -> Option<i64> {
    match unit.trim_end_matches('s') {
        "millisecond" | "ms" => Some(1),
        "second" | "sec" => Some(1_000),
        "minute" | "min" => Some(60_000),
        "hour" | "hr" => Some(3_600_000),
        "day" => Some(86_400_000),
        "week" => Some(604_800_000),
        _ => None,
    }
}

/// `{a, b, c}`, and the ordered `[a, b, c]` beside it.
#[must_use]
pub fn set_literal(elements: &[Expr], env: &Env<'_>) -> Evaluation {
    let mut unresolved = Vec::new();
    let items = elements
        .iter()
        .map(|item| {
            let evaluated = super::eval(item, env);
            unresolved.extend(evaluated.unresolved);
            evaluated.value
        })
        .collect();
    Evaluation { value: Value::Set(items), unresolved }
}

#[cfg(test)]
mod tests {
    use allium_parser::ast::Ident;

    use super::*;
    use crate::world::World;

    fn env(world: &World) -> Env<'_> {
        Env::new(world, "m", "")
    }

    /// The expression these are read out of. Only its span is used — for the
    /// note an undecided literal carries — so one shape does for both.
    fn literal(value: &str) -> Expr {
        Expr::NumberLiteral {
            span: allium_parser::Span { start: 0, end: 1 },
            value: value.to_owned(),
        }
    }

    fn at() -> Expr {
        Expr::Null { span: allium_parser::Span { start: 0, end: 1 } }
    }

    fn evaluate(tag: &str, value: &str) -> Evaluation {
        let world = World::new();
        match tag {
            "NumberLiteral" => number(value, &at(), &env(&world)),
            _ => duration(value, &at(), &env(&world)),
        }
    }

    #[test]
    fn a_whole_number_stays_whole() {
        assert_eq!(evaluate("NumberLiteral", "20").value, Value::Int(20));
        assert_eq!(evaluate("NumberLiteral", "-3").value, Value::Int(-3));
    }

    #[test]
    fn digit_separators_survive() {
        // `2_000_000_000` is how a real spec writes a byte cap, and parsing it
        // naively yields no number at all.
        assert_eq!(evaluate("NumberLiteral", "2_000_000_000").value, Value::Int(2_000_000_000));
    }

    #[test]
    fn a_decimal_stays_a_decimal() {
        assert_eq!(evaluate("NumberLiteral", "1.5").value, Value::Float(1.5));
    }

    #[test]
    fn something_that_is_not_a_number_says_so() {
        let evaluated = evaluate("NumberLiteral", "twenty");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("not a number"));
    }

    #[test]
    fn an_empty_number_says_so() {
        let evaluated = evaluate("NumberLiteral", "");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("no digits"));
    }

    // Booleans used to be tested here twice: once for the word `"true"` and
    // once for a real JSON `true`, because the document could carry either and
    // the reader handled both. `BoolLiteral` holds a `bool`, so there is
    // nothing left to parse and nothing left to get wrong — the dispatcher
    // reads the field.

    #[test]
    fn every_unit_of_time_reduces_to_milliseconds() {
        let table = [
            ("1.milliseconds", 1),
            ("1.seconds", 1_000),
            ("1.minutes", 60_000),
            ("1.hours", 3_600_000),
            ("1.days", 86_400_000),
            ("1.weeks", 604_800_000),
            ("21.days", 21 * 86_400_000),
            ("200.seconds", 200_000),
        ];
        for (written, millis) in table {
            assert_eq!(
                evaluate("DurationLiteral", written).value,
                Value::Duration(millis),
                "{written}"
            );
        }
    }

    #[test]
    fn a_singular_unit_reads_the_same_as_a_plural() {
        assert_eq!(evaluate("DurationLiteral", "1.day").value, Value::Duration(86_400_000));
    }

    #[test]
    fn a_duration_is_not_an_integer() {
        // `21.days` reduced to `21` would compare equal to twenty-one of
        // anything, which is the whole reason the kind is preserved.
        let Value::Duration(_) = evaluate("DurationLiteral", "21.days").value else {
            panic!("a duration must stay a duration");
        };
    }

    #[test]
    fn an_unrecognised_unit_says_which_one() {
        let evaluated = evaluate("DurationLiteral", "3.fortnights");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("fortnight"), "{:?}", evaluated.unresolved);
    }

    #[test]
    fn a_duration_with_no_unit_says_so() {
        let evaluated = evaluate("DurationLiteral", "21");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("not a duration"));
    }

    #[test]
    fn a_duration_with_no_amount_says_so() {
        let evaluated = evaluate("DurationLiteral", "many.days");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("no amount"));
    }

    #[test]
    fn a_duration_too_long_to_measure_says_so_rather_than_wrapping() {
        let evaluated = evaluate("DurationLiteral", "9223372036854775807.days");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("too long"));
    }

    #[test]
    fn a_set_literal_evaluates_its_elements() {
        let world = World::new();
        let elements = [literal("1"), literal("2")];
        let evaluated = set_literal(&elements, &env(&world));
        assert_eq!(evaluated.value, Value::Set(vec![Value::Int(1), Value::Int(2)]));
        assert!(evaluated.unresolved.is_empty());
    }

    #[test]
    fn a_set_literal_carries_what_it_could_not_decide() {
        let world = World::new();
        let elements = [Expr::Ident(Ident {
            span: allium_parser::Span { start: 0, end: 1 },
            name: "absent".to_owned(),
        })];
        let evaluated = set_literal(&elements, &env(&world));
        assert_eq!(evaluated.value, Value::Set(vec![Value::Unknown]));
        assert_eq!(evaluated.unresolved.len(), 1);
    }

    #[test]
    fn an_empty_set_literal_is_an_empty_collection() {
        let world = World::new();
        assert_eq!(set_literal(&[], &env(&world)).value, Value::Set(Vec::new()));
    }
}
