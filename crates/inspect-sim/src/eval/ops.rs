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
//!
//! Each operator is one of allium's own closed enums, so none of the three
//! functions below has a "that is not an operator I know" branch any more. One
//! of those branches was unreachable: a comparison could never carry `Implies`,
//! because the language puts it on a connective. Reading the operator out of a
//! string is what made it look possible.

use std::ops::Not;

use allium_parser::ast::{BinaryOp, ComparisonOp, Expr, LogicalOp};

use super::{
    Env, Evaluation, Unresolved,
    ast::{bare_name, span_of, truth_value},
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
pub fn compare(
    left_node: &Expr,
    operator: ComparisonOp,
    right_node: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    let mut left = eval(left_node, env);
    let mut right = eval(right_node, env);

    if matches!(left.value, Value::Enum(_))
        && right.value.is_unknown()
        && let Some(state) = bare_name(right_node)
    {
        right = Evaluation::known(Value::Enum(state.to_owned()));
    } else if matches!(right.value, Value::Enum(_))
        && left.value.is_unknown()
        && let Some(state) = bare_name(left_node)
    {
        left = Evaluation::known(Value::Enum(state.to_owned()));
    }

    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);

    let truth = match operator {
        ComparisonOp::Eq => left.value.equals(&right.value),
        ComparisonOp::NotEq => left.value.equals(&right.value).not(),
        ordering_op => match left.value.compare(&right.value) {
            Some(ordering) => Truth::from_bool(match ordering_op {
                ComparisonOp::Lt => ordering.is_lt(),
                ComparisonOp::LtEq => ordering.is_le(),
                ComparisonOp::Gt => ordering.is_gt(),
                _ => ordering.is_ge(),
            }),
            None => {
                unresolved.push(unorderable(&left.value, &right.value, left_node, right_node, env));
                Truth::Unknown
            }
        },
    };

    Evaluation { value: truth_value(truth), unresolved }
}

/// Why two values could not be ordered, quoting the pair.
fn unorderable(
    left: &Value,
    right: &Value,
    left_node: &Expr,
    right_node: &Expr,
    env: &Env<'_>,
) -> Unresolved {
    let span = between(left_node, right_node);
    Unresolved {
        reason: format!("{} cannot be ordered against {}", left.described(), right.described()),
        expression: span.and_then(|span| span.slice(env.source)).map(str::to_owned),
        span,
    }
}

/// The span covering both sides of an operator.
///
/// The typed tree does carry a span on the operator node itself, but the two
/// sides are what a reader needs to see and the operator's own span is not
/// passed down here. Joining the operands gives the same text without threading
/// a third argument through every call.
fn between(left: &Expr, right: &Expr) -> Option<inspect_model::Span> {
    let (start, end) = (span_of(left)?, span_of(right)?);
    Some(inspect_model::Span::new(start.start, end.end))
}

/// `and`, `or`, `implies`.
pub fn logical(
    left_node: &Expr,
    operator: LogicalOp,
    right_node: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    let left = eval(left_node, env);
    let right = eval(right_node, env);
    let (left_truth, right_truth) = (left.truth(), right.truth());
    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);

    let truth = match operator {
        LogicalOp::And => left_truth.and(right_truth),
        LogicalOp::Or => left_truth.or(right_truth),
        LogicalOp::Implies => left_truth.implies(right_truth),
    };
    Evaluation { value: truth_value(truth), unresolved }
}

/// `+`, `-`, `*`, `/` over numbers, durations and timestamps.
pub fn arithmetic(
    left_node: &Expr,
    operator: BinaryOp,
    right_node: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    let left = eval(left_node, env);
    let right = eval(right_node, env);
    let mut unresolved = left.unresolved;
    unresolved.extend(right.unresolved);

    // Checked throughout. These are numbers a person typed into a world, so
    // running off the end of an `i64` is ordinary input rather than a bug in a
    // spec — and `just run` is a debug build, where the unchecked form aborts
    // the whole process instead of reporting anything. `literals.rs` and
    // `seed.rs` already went through `checked_mul`; this was the one cluster
    // left doing it raw.
    let computed = match (&left.value, operator, &right.value) {
        // A timestamp plus a duration is a timestamp; the units are the point.
        (Value::Timestamp(at), BinaryOp::Add, Value::Duration(by)) => {
            at.checked_add(*by).map(Value::Timestamp).ok_or(Fault::Overflow)
        }
        (Value::Timestamp(at), BinaryOp::Sub, Value::Duration(by)) => {
            at.checked_sub(*by).map(Value::Timestamp).ok_or(Fault::Overflow)
        }
        (Value::Timestamp(later), BinaryOp::Sub, Value::Timestamp(earlier)) => {
            later.checked_sub(*earlier).map(Value::Duration).ok_or(Fault::Overflow)
        }
        (Value::Duration(left), BinaryOp::Add, Value::Duration(right)) => {
            left.checked_add(*right).map(Value::Duration).ok_or(Fault::Overflow)
        }
        (Value::Duration(left), BinaryOp::Sub, Value::Duration(right)) => {
            left.checked_sub(*right).map(Value::Duration).ok_or(Fault::Overflow)
        }
        (Value::Int(_), BinaryOp::Div, Value::Int(0)) => Err(Fault::DividedByZero),
        (Value::Int(left), _, Value::Int(right)) => {
            let checked = match operator {
                BinaryOp::Add => left.checked_add(*right),
                BinaryOp::Sub => left.checked_sub(*right),
                BinaryOp::Mul => left.checked_mul(*right),
                BinaryOp::Div => left.checked_div(*right),
            };
            checked.map(Value::Int).ok_or(Fault::Overflow)
        }
        _ => Err(Fault::Undefined),
    };

    let value = match computed {
        Ok(value) => value,
        Err(fault) => {
            // An operand that was already undecided has said why. Adding a
            // second reason on top of it would blame the operator for a gap
            // somewhere underneath it.
            if !left.value.is_unknown() && !right.value.is_unknown() {
                let span = between(left_node, right_node);
                let reason = match fault {
                    Fault::Undefined => format!(
                        "`{}` is not defined between {} and {}",
                        name_of(operator),
                        left.value.described(),
                        right.value.described()
                    ),
                    Fault::Overflow => format!(
                        "`{}` between {} and {} runs past what a whole number holds here",
                        name_of(operator),
                        left.value.described(),
                        right.value.described()
                    ),
                    Fault::DividedByZero => "divided by zero".to_owned(),
                };
                unresolved.push(Unresolved {
                    reason,
                    expression: span.and_then(|span| span.slice(env.source)).map(str::to_owned),
                    span,
                });
            }
            Value::Unknown
        }
    };
    Evaluation { value, unresolved }
}

/// Why an arithmetic operator produced nothing.
///
/// Three different answers a reader can act on. Collapsing them into one
/// unknown would tell somebody whose world overflowed that `+` is not defined
/// between two whole numbers, which is both wrong and unfixable.
enum Fault {
    /// The operator means nothing between these two kinds of value.
    Undefined,
    /// It means something, and the answer does not fit.
    Overflow,
    DividedByZero,
}

/// An arithmetic operator as the spec spells it.
fn name_of(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
    }
}
