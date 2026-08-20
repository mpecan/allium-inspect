//! Literals: numbers, booleans, durations and sets.
//!
//! Each of these is a small parse with one thing worth getting right, and each
//! of those things is a place where a plausible shortcut produces a wrong answer
//! that nothing would notice:
//!
//! - a number is reported as *text* so digit separators survive, and
//!   `2_000_000_000` parsed naively is not a number at all;
//! - a duration carries a unit, and `21.days` reduced to `21` compares equal to
//!   twenty-one of anything;
//! - a boolean may arrive as either a JSON boolean or the word.

use serde_json::Value as Json;

use super::{Evaluation, ast::text_of};
use crate::{eval::Env, value::Value};

/// `20`, `2_000_000_000`, `1.5`.
#[must_use]
pub fn number(inner: &Json, node: &Json, env: &Env<'_>) -> Evaluation {
    let raw = text_of(inner).replace('_', "");
    if raw.is_empty() {
        return Evaluation::unknown("a number literal with no digits", node, env.source);
    }
    if let Ok(whole) = raw.parse::<i64>() {
        return Evaluation::known(Value::Int(whole));
    }
    match raw.parse::<f64>() {
        Ok(decimal) => Evaluation::known(Value::Float(decimal)),
        Err(_) => Evaluation::unknown(format!("`{raw}` is not a number"), node, env.source),
    }
}

/// `true`, `false`.
#[must_use]
pub fn boolean(inner: &Json, node: &Json, env: &Env<'_>) -> Evaluation {
    if let Some(value) = inner.get("value").and_then(Json::as_bool) {
        return Evaluation::known(Value::Bool(value));
    }
    match text_of(inner).as_str() {
        "true" => Evaluation::known(Value::Bool(true)),
        "false" => Evaluation::known(Value::Bool(false)),
        other => Evaluation::unknown(format!("`{other}` is not a boolean"), node, env.source),
    }
}

/// `21.days`, `200.seconds`, `24.hours`.
///
/// Reduced to milliseconds, which is the only unit the world's clock has. The
/// *kind* is kept as a duration rather than an integer, so that a spec
/// comparing a duration to a bare number is reported as incomparable instead of
/// quietly answered.
#[must_use]
pub fn duration(inner: &Json, node: &Json, env: &Env<'_>) -> Evaluation {
    let raw = text_of(inner);
    let Some((amount, unit)) = raw.split_once('.') else {
        return Evaluation::unknown(format!("`{raw}` is not a duration"), node, env.source);
    };
    let Ok(amount) = amount.replace('_', "").parse::<i64>() else {
        return Evaluation::unknown(format!("`{raw}` has no amount"), node, env.source);
    };
    let Some(millis) = unit_millis(unit) else {
        return Evaluation::unknown(
            format!("`{unit}` is not a unit of time this evaluator knows"),
            node,
            env.source,
        );
    };
    match amount.checked_mul(millis) {
        Some(total) => Evaluation::known(Value::Duration(total)),
        None => Evaluation::unknown(format!("`{raw}` is too long to measure"), node, env.source),
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

/// `{a, b, c}`.
#[must_use]
pub fn set_literal(inner: &Json, env: &Env<'_>) -> Evaluation {
    let mut unresolved = Vec::new();
    let items = inner
        .get("items")
        .or_else(|| inner.get("elements"))
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let evaluated = super::eval(item, env);
                    unresolved.extend(evaluated.unresolved);
                    evaluated.value
                })
                .collect()
        })
        .unwrap_or_default();
    Evaluation { value: Value::Set(items), unresolved }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::world::World;

    fn env(world: &World) -> Env<'_> {
        Env::new(world, "m", "")
    }

    fn evaluate(tag: &str, value: &str) -> Evaluation {
        let world = World::new();
        let node = json!({ tag: {"span": {"start": 0, "end": 1}, "value": value} });
        let (_, inner) = super::super::ast::tagged(&node).expect("tagged");
        match tag {
            "NumberLiteral" => number(inner, &node, &env(&world)),
            "BoolLiteral" => boolean(inner, &node, &env(&world)),
            _ => duration(inner, &node, &env(&world)),
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

    #[test]
    fn booleans_parse_from_the_word_or_the_json_value() {
        assert_eq!(evaluate("BoolLiteral", "true").value, Value::Bool(true));
        assert_eq!(evaluate("BoolLiteral", "false").value, Value::Bool(false));

        let world = World::new();
        let node = json!({"BoolLiteral": {"value": true}});
        let (_, inner) = super::super::ast::tagged(&node).expect("tagged");
        assert_eq!(boolean(inner, &node, &env(&world)).value, Value::Bool(true));
    }

    #[test]
    fn something_that_is_not_a_boolean_says_so() {
        let evaluated = evaluate("BoolLiteral", "maybe");
        assert_eq!(evaluated.value, Value::Unknown);
        assert!(evaluated.unresolved[0].reason.contains("not a boolean"));
    }

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
        let node = json!({"items": [
            {"NumberLiteral": {"value": "1"}},
            {"NumberLiteral": {"value": "2"}},
        ]});
        let evaluated = set_literal(&node, &env(&world));
        assert_eq!(evaluated.value, Value::Set(vec![Value::Int(1), Value::Int(2)]));
        assert!(evaluated.unresolved.is_empty());
    }

    #[test]
    fn a_set_literal_carries_what_it_could_not_decide() {
        let world = World::new();
        let node =
            json!({"items": [{"Ident": {"span": {"start": 0, "end": 1}, "name": "absent"}}]});
        let evaluated = set_literal(&node, &env(&world));
        assert_eq!(evaluated.value, Value::Set(vec![Value::Unknown]));
        assert_eq!(evaluated.unresolved.len(), 1);
    }

    #[test]
    fn an_empty_set_literal_is_an_empty_collection() {
        let world = World::new();
        assert_eq!(set_literal(&json!({}), &env(&world)).value, Value::Set(Vec::new()));
    }
}
