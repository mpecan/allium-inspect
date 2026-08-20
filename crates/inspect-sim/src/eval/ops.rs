//! The operators: comparison, the connectives, and arithmetic.
//!
//! Three things happen here that are worth naming, because each is a place
//! where the obvious implementation gives a confidently wrong answer.
//!
//! **A comparison decides what a bare name meant.** In `copy.status =
//! available`, `available` parses as a plain identifier, indistinguishable from
//! a rule parameter that was never supplied. Reading it as a state whenever it
//! is unbound would turn a missing argument into a satisfied precondition;
//! reading it as unknown always would make every status check undecided. The
//! other side of the comparison says which was meant, so the decision is made
//! here and nowhere else.
//!
//! **Ordering across kinds has no answer rather than a negative one.** A string
//! is not less than a duration and it is not greater than one either; the
//! question is malformed, and answering it either way would report a
//! precondition as settled that the spec never posed.
//!
//! **Arithmetic keeps units.** A timestamp plus a duration is a timestamp; two
//! timestamps subtract to a duration; a duration plus an integer is nothing at
//! all. Collapsing them to integers would let `due_at + 21` typecheck its way
//! to an answer that means nothing.

use std::ops::Not;

use serde_json::Value as Json;

use super::{
    Env, Evaluation, Unresolved,
    ast::{bare_name, span_of, string_at, truth_value},
    eval,
};
use crate::{truth::Truth, value::Value};

/// `left <op> right`.
///
/// Where the identifier-versus-state judgement is made. When one side is a
/// state and the other is a bare name nothing is bound to, the name is read as
/// a state — which is what `copy.status = available` means. When neither side
/// says so, an unbound name stays undecided, so a missing rule argument is
/// reported rather than quietly satisfied.
pub fn compare(inner: &Json, env: &Env<'_>) -> Evaluation {
    let (Some(left_node), Some(right_node)) = (inner.get("left"), inner.get("right")) else {
        return Evaluation::unknown("a comparison missing a side", inner, env.source);
    };
    let operator = string_at(inner, "op").unwrap_or_default();

    let mut left = eval(left_node, env);
    let mut right = eval(right_node, env);

    if matches!(left.value, Value::Enum(_))
        && right.value.is_unknown()
        && let Some(state) = bare_name(right_node)
    {
        right = Evaluation::known(Value::Enum(state));
    } else if matches!(right.value, Value::Enum(_))
        && left.value.is_unknown()
        && let Some(state) = bare_name(left_node)
    {
        left = Evaluation::known(Value::Enum(state));
    }

    let (left_truth, right_truth) = (left.truth(), right.truth());
    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);

    let truth = match operator.as_str() {
        "Eq" => left.value.equals(&right.value),
        "NotEq" => left.value.equals(&right.value).not(),
        "Lt" | "LtEq" | "Gt" | "GtEq" => match left.value.compare(&right.value) {
            Some(ordering) => Truth::from_bool(match operator.as_str() {
                "Lt" => ordering.is_lt(),
                "LtEq" => ordering.is_le(),
                "Gt" => ordering.is_gt(),
                _ => ordering.is_ge(),
            }),
            None => {
                unresolved.push(Unresolved {
                    reason: format!(
                        "{} cannot be ordered against {}",
                        left.value.described(),
                        right.value.described()
                    ),
                    expression: span_of(inner)
                        .and_then(|span| span.slice(env.source))
                        .map(str::to_owned),
                    span: span_of(inner),
                });
                Truth::Unknown
            }
        },
        "Implies" => left_truth.implies(right_truth),
        other => {
            unresolved.push(Unresolved {
                reason: format!("`{other}` is not a comparison this evaluator knows"),
                expression: None,
                span: span_of(inner),
            });
            Truth::Unknown
        }
    };

    Evaluation { value: truth_value(truth), unresolved }
}

/// `and`, `or`, `implies`.
pub fn logical(inner: &Json, env: &Env<'_>) -> Evaluation {
    let (Some(left_node), Some(right_node)) = (inner.get("left"), inner.get("right")) else {
        return Evaluation::unknown("a connective missing a side", inner, env.source);
    };
    let left = eval(left_node, env);
    let right = eval(right_node, env);
    let (left_truth, right_truth) = (left.truth(), right.truth());
    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);

    let truth = match string_at(inner, "op").unwrap_or_default().as_str() {
        "And" => left_truth.and(right_truth),
        "Or" => left_truth.or(right_truth),
        "Implies" => left_truth.implies(right_truth),
        other => {
            unresolved.push(Unresolved {
                reason: format!("`{other}` is not a connective this evaluator knows"),
                expression: None,
                span: span_of(inner),
            });
            Truth::Unknown
        }
    };
    Evaluation { value: truth_value(truth), unresolved }
}

/// `+`, `-`, `*`, `/` over numbers, durations and timestamps.
pub fn arithmetic(inner: &Json, env: &Env<'_>) -> Evaluation {
    let (Some(left_node), Some(right_node)) = (inner.get("left"), inner.get("right")) else {
        return Evaluation::unknown("an operation missing a side", inner, env.source);
    };
    let left = eval(left_node, env);
    let right = eval(right_node, env);
    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);
    let operator = string_at(inner, "op").unwrap_or_default();

    let value = match (&left.value, operator.as_str(), &right.value) {
        // A timestamp plus a duration is a timestamp; the units are the point.
        (Value::Timestamp(at), "Add", Value::Duration(by)) => Value::Timestamp(at + by),
        (Value::Timestamp(at), "Sub", Value::Duration(by)) => Value::Timestamp(at - by),
        (Value::Timestamp(later), "Sub", Value::Timestamp(earlier)) => {
            Value::Duration(later - earlier)
        }
        (Value::Duration(left), "Add", Value::Duration(right)) => Value::Duration(left + right),
        (Value::Duration(left), "Sub", Value::Duration(right)) => Value::Duration(left - right),
        (Value::Int(left), _, Value::Int(right)) => match operator.as_str() {
            "Add" => Value::Int(left + right),
            "Sub" => Value::Int(left - right),
            "Mul" => Value::Int(left * right),
            "Div" if *right != 0 => Value::Int(left / right),
            _ => Value::Unknown,
        },
        _ => Value::Unknown,
    };

    if value.is_unknown() && !left.value.is_unknown() && !right.value.is_unknown() {
        unresolved.push(Unresolved {
            reason: format!(
                "`{operator}` is not defined between {} and {}",
                left.value.described(),
                right.value.described()
            ),
            expression: span_of(inner).and_then(|span| span.slice(env.source)).map(str::to_owned),
            span: span_of(inner),
        });
    }
    Evaluation { value, unresolved }
}
