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
///
/// A set holds each member once — `{a, b, a}` is `{a, b}` — and a list keeps
/// what it was given. [`Value::set_of`] is the one place a set is made from
/// what a spec wrote, so a default and a rule spelled alike agree.
///
/// A member written as a bare word that nothing binds, and that the spec
/// declares as a state, is that state: `for k in {a, b}` with `enum Kind { a
/// | b }` ranges over two words, as `kind = a` and an emission's `a` already
/// read them (issue #26). The test is the one a definition's bare word is put
/// to — [`Env::could_be_a_state`] and a declaration — so a parameter nobody
/// passed, a field nobody set, or a word no enumeration declares stays
/// undecided and says which. A member bound to a value nobody knows says
/// that too, rather than leaving the set looking complete.
#[must_use]
pub fn set_literal(elements: &[Expr], ordered: bool, env: &Env<'_>) -> Evaluation {
    let mut unresolved = Vec::new();
    let items: Vec<Value> = elements
        .iter()
        .map(|item| {
            let evaluated = super::eval(item, env);
            if evaluated.value.is_unknown()
                && let Some(state) = super::ast::bare_name(item)
                && env.derived.states.contains(state)
                && env.could_be_a_state(state)
            {
                return Value::Enum(state.to_owned());
            }
            // A member that is undecided and says nothing — a name bound to a
            // value nobody knows — says so here, or `{x, b}` reads as a
            // complete set of two and a loop over it runs as if it knew.
            if evaluated.value.holds_unknown() && evaluated.unresolved.is_empty() {
                let name = super::ast::bare_name(item).unwrap_or("this member");
                unresolved.push(super::unresolved_at(
                    format!("`{name}` is bound to a value nobody knows"),
                    item,
                    env.source,
                ));
            }
            unresolved.extend(evaluated.unresolved);
            evaluated.value
        })
        .collect();
    let value = if ordered { Value::Set(items) } else { Value::set_of(items) };
    Evaluation { value, unresolved }
}

#[cfg(test)]
mod tests {
    use allium_parser::ast::Ident;

    use super::*;
    use crate::{truth::Truth, world::World};

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
        let evaluated = set_literal(&elements, false, &env(&world));
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
        let evaluated = set_literal(&elements, false, &env(&world));
        assert_eq!(evaluated.value, Value::Set(vec![Value::Unknown]));
        assert_eq!(evaluated.unresolved.len(), 1);
    }

    #[test]
    fn a_set_literal_holds_a_repeat_once_and_a_list_keeps_it() {
        // Issue #25: `{a, b, a}` is the set `{a, b}`, as the seed reads the
        // same words in a default. `[1, 2, 1]` is three things in order.
        let world = World::new();
        let elements = [literal("1"), literal("2"), literal("1")];
        let set = set_literal(&elements, false, &env(&world)).value;
        assert_eq!(set, Value::Set(vec![Value::Int(1), Value::Int(2)]));
        assert_eq!(set.equals(&Value::Set(vec![Value::Int(2), Value::Int(1)])), Truth::True);
        assert_eq!(
            set_literal(&elements, true, &env(&world)).value,
            Value::Set(vec![Value::Int(1), Value::Int(2), Value::Int(1)])
        );
    }

    fn word(name: &str) -> Expr {
        Expr::Ident(Ident { span: allium_parser::Span { start: 0, end: 1 }, name: name.to_owned() })
    }

    /// A spec set declaring `enum Kind { a | b }`.
    fn kinds() -> inspect_model::Derivations {
        let mut derived = inspect_model::Derivations::default();
        derived.states.extend(["a".to_owned(), "b".to_owned()]);
        derived
    }

    #[test]
    fn a_set_literal_of_declared_states_holds_the_states() {
        // Issue #26: `for k in {a, b}` with `enum Kind { a | b }`. Each word
        // was "nothing is bound", so the loop ran over nothing it could name.
        let world = World::new();
        let derived = kinds();
        let env = env(&world).deriving(&derived);
        let evaluated = set_literal(&[word("a"), word("b")], false, &env);
        assert_eq!(
            evaluated.value,
            Value::Set(vec![Value::Enum("a".to_owned()), Value::Enum("b".to_owned())])
        );
        assert!(evaluated.unresolved.is_empty(), "{:?}", evaluated.unresolved);
        let listed = set_literal(&[word("b"), word("a"), word("b")], true, &env).value;
        assert_eq!(
            listed,
            Value::Set(vec![
                Value::Enum("b".to_owned()),
                Value::Enum("a".to_owned()),
                Value::Enum("b".to_owned()),
            ])
        );
    }

    #[test]
    fn a_word_no_enumeration_declares_stays_undecided_in_a_set_literal() {
        // Only a declared state: `c` may be something this simulator does not
        // model, and the set says which word.
        let world = World::new();
        let derived = kinds();
        let env = env(&world).deriving(&derived);
        let evaluated = set_literal(&[word("a"), word("c")], false, &env);
        assert!(evaluated.value.holds_unknown());
        assert_eq!(evaluated.unresolved.len(), 1);
        assert_eq!(evaluated.unresolved[0].reason, "nothing is bound to `c`");
    }

    #[test]
    fn a_bound_name_spelled_like_a_state_is_its_binding_in_a_set_literal() {
        let world = World::new();
        let derived = kinds();
        let env = env(&world).deriving(&derived).bind("a", Value::Int(7));
        let evaluated = set_literal(&[word("a")], false, &env);
        assert_eq!(evaluated.value, Value::Set(vec![Value::Int(7)]));
    }

    #[test]
    fn a_parameter_nobody_passed_is_not_read_as_a_state_in_a_set_literal() {
        // A rule parameter spelled `a` that nobody passed is a value nobody
        // knows, not the state `a`.
        let world = World::new();
        let derived = kinds();
        let env = env(&world).deriving(&derived).declaring(["a".to_owned()]);
        let evaluated = set_literal(&[word("a"), word("b")], false, &env);
        assert!(evaluated.value.holds_unknown());
        assert_eq!(evaluated.unresolved[0].reason, "nothing is bound to `a`");
    }

    #[test]
    fn a_member_bound_to_a_value_nobody_knows_says_so_in_a_set_literal() {
        // `let x = box.a` with `a` unset, then `{x, b}`. The binding carries
        // no reason of its own, and with `b` now read as the state nothing
        // else in the set did either: a loop over it ran as if it knew both
        // members, and made two slots where `x = b` would have made one.
        let world = World::new();
        let derived = kinds();
        // The source the note quotes: `word` spans its first byte.
        let env = Env::new(&world, "m", "x").deriving(&derived).bind("x", Value::Unknown);
        let evaluated = set_literal(&[word("x"), word("b")], false, &env);
        assert!(evaluated.value.holds_unknown());
        assert_eq!(evaluated.unresolved.len(), 1, "{:?}", evaluated.unresolved);
        assert_eq!(evaluated.unresolved[0].reason, "`x` is bound to a value nobody knows");
        assert_eq!(evaluated.unresolved[0].expression.as_deref(), Some("x"));
    }

    #[test]
    fn a_field_of_this_spelled_like_a_state_is_not_read_as_the_state_in_a_set_literal() {
        // `options: {a, b}` on an entity that stores a field `a` it never
        // set. `a` is the field, unset — not the state `a`.
        let mut world = World::new();
        let id = world.create("Box", "m");
        let mut derived = kinds();
        derived.stored.insert(inspect_model::derived_key("m", "Box", "a"));
        let env = env(&world).deriving(&derived).bind("this", Value::Ref(id));
        let evaluated = set_literal(&[word("a"), word("b")], false, &env);
        assert!(evaluated.value.holds_unknown(), "{:?}", evaluated.value);
        assert!(!evaluated.unresolved.is_empty());
        assert!(
            evaluated.unresolved.iter().all(|note| !note.reason.contains("nothing is bound")),
            "{:?}",
            evaluated.unresolved
        );
    }

    #[test]
    fn an_empty_set_literal_is_an_empty_collection() {
        let world = World::new();
        assert_eq!(set_literal(&[], false, &env(&world)).value, Value::Set(Vec::new()));
    }
}
