//! Whether a temporal trigger is due, when its deadline is `null` (issue #20).
//!
//! `when: task: Task.stopped_at + config.ttl <= now` asks one question: has
//! this task's deadline passed? A `stopped_at` that is `null` is a task that
//! has not stopped, so there is no deadline yet, and nothing is due. The
//! answer is *no*, decided — not an unknown.
//!
//! Read as an ordinary expression the same condition is undecided, and rightly:
//! `null + 1.hour` is not a timestamp, and `null <= now` asks to order two
//! things that have no order. The evaluator keeps saying so everywhere else —
//! a precondition, an invariant, a `then` — because there the question is not
//! known to be "is it due". Only a state rule's own `when` is read here, and
//! only in the shape that asks it:
//!
//! - one side reads the clock: `now`, or `now` moved by something;
//! - the other side is the deadline, on the side asked to be *earlier* —
//!   `deadline <= now`, `deadline < now`, `now >= deadline`, `now > deadline`;
//! - and the deadline is `null`, or a `null` moved by durations:
//!   `stopped_at`, `stopped_at + config.ttl`, `config.ttl + stopped_at`,
//!   `sent_at + a + b`.
//!
//! Only the whole `when`, never part of one: allium accepts a state trigger
//! that is a single comparison and rejects an `and` or an `or` there
//! (`allium.rule.invalidTrigger`) — a conjunct belongs in `requires`.
//!
//! The deadline has to be written in the trigger. A derived field that
//! computes one — `expires_at: stopped_at + config.ttl`, then `Task.expires_at
//! <= now` — evaluates to an unknown rather than a `null`, and is left to the
//! evaluator.
//!
//! Anything else is evaluated as it always was. In particular the opposite
//! question — `opens_at > now`, *is it still ahead* — is not one a `null`
//! answers either way, so it stays undecided with the evaluator's own reason
//! naming the comparison. Read as "no deadline, so never ahead" it would be the
//! simulator choosing a meaning the spec never wrote.

use allium_parser::ast::{BinaryOp, ComparisonOp, Expr};

use crate::{
    eval::{Env, Evaluation, eval},
    value::Value,
};

/// A state rule's `when` condition, read as a trigger: whether it holds now.
///
/// `false` with nothing unresolved when it asks whether a `null` deadline has
/// passed; otherwise exactly what [`eval`] says.
pub(crate) fn trigger(condition: &Expr, env: &Env<'_>) -> Evaluation {
    if asks_after_nothing(condition, env) {
        return Evaluation::known(Value::Bool(false));
    }
    eval(condition, env)
}

/// Whether `condition` asks if a deadline that is `null` has passed.
fn asks_after_nothing(condition: &Expr, env: &Env<'_>) -> bool {
    let Expr::Comparison { left, op, right, .. } = condition else { return false };
    let (deadline, clock) = match op {
        ComparisonOp::Lt | ComparisonOp::LtEq => (left, right),
        ComparisonOp::Gt | ComparisonOp::GtEq => (right, left),
        ComparisonOp::Eq | ComparisonOp::NotEq => return false,
    };
    reads_the_clock(clock) && absent(deadline, env)
}

/// `now`, or `now` moved: `now - config.grace`.
fn reads_the_clock(expr: &Expr) -> bool {
    match expr {
        Expr::Now { .. } => true,
        Expr::BinaryOp { left, op: BinaryOp::Add | BinaryOp::Sub, .. } => reads_the_clock(left),
        _ => false,
    }
}

/// A deadline that is not there: `null`, or `null` moved by durations, on
/// either side of a `+`.
///
/// The offset has to be a duration this world knows. `null + 1.hour` is no
/// deadline whatever the hour; `null + x` with `x` undecided might not have
/// been a deadline at all, and saying so is the evaluator's job.
fn absent(expr: &Expr, env: &Env<'_>) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right, .. } => {
            let duration = |side: &Expr| matches!(eval(side, env).value, Value::Duration(_));
            match op {
                BinaryOp::Add => {
                    (absent(left, env) && duration(right)) || (duration(left) && absent(right, env))
                }
                // `ttl - stopped_at` is no deadline moved; it is not a deadline.
                BinaryOp::Sub => absent(left, env) && duration(right),
                BinaryOp::Mul | BinaryOp::Div => false,
            }
        }
        _ => eval(expr, env).value == Value::Null,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use allium_parser::{
        Span,
        ast::{BinaryOp, ComparisonOp, Expr, Ident},
    };

    use super::trigger;
    use crate::{eval::Env, truth::Truth, value::Value, world::World};

    const AT: Span = Span { start: 0, end: 0 };

    fn name(text: &str) -> Expr {
        Expr::Ident(Ident { span: AT, name: text.to_owned() })
    }

    fn now() -> Expr {
        Expr::Now { span: AT }
    }

    fn plus(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp { span: AT, left: Box::new(left), op: BinaryOp::Add, right: Box::new(right) }
    }

    fn minus(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp { span: AT, left: Box::new(left), op: BinaryOp::Sub, right: Box::new(right) }
    }

    fn compare(left: Expr, op: ComparisonOp, right: Expr) -> Expr {
        Expr::Comparison { span: AT, left: Box::new(left), op, right: Box::new(right) }
    }

    /// `stopped` null, `started` a timestamp, `ttl` an hour, `vague` unknown,
    /// `stamp` a timestamp where a duration belongs.
    fn read(condition: &Expr) -> (Truth, usize) {
        let mut world = World::new();
        world.now = 10 * 3_600_000;
        let mut env = Env::new(&world, "m", "");
        env.bindings.insert("stopped".to_owned(), Value::Null);
        env.bindings.insert("started".to_owned(), Value::Timestamp(0));
        env.bindings.insert("ttl".to_owned(), Value::Duration(3_600_000));
        env.bindings.insert("vague".to_owned(), Value::Unknown);
        env.bindings.insert("stamp".to_owned(), Value::Timestamp(0));
        let evaluated = trigger(condition, &env);
        (evaluated.truth(), evaluated.unresolved.len())
    }

    #[test]
    fn a_null_deadline_moved_by_a_duration_has_not_passed() {
        let due = compare(plus(name("stopped"), name("ttl")), ComparisonOp::LtEq, now());
        assert_eq!(read(&due), (Truth::False, 0));
    }

    #[test]
    fn a_bare_null_deadline_has_not_passed() {
        assert_eq!(read(&compare(name("stopped"), ComparisonOp::LtEq, now())), (Truth::False, 0));
        assert_eq!(read(&compare(name("stopped"), ComparisonOp::Lt, now())), (Truth::False, 0));
    }

    #[test]
    fn the_clock_may_be_written_on_the_left() {
        let due = compare(now(), ComparisonOp::GtEq, plus(name("stopped"), name("ttl")));
        assert_eq!(read(&due), (Truth::False, 0));
        assert_eq!(read(&compare(now(), ComparisonOp::Gt, name("stopped"))), (Truth::False, 0));
    }

    #[test]
    fn a_deadline_moved_more_than_once_is_still_absent() {
        let deadline = minus(plus(name("stopped"), name("ttl")), name("ttl"));
        assert_eq!(read(&compare(deadline, ComparisonOp::Lt, now())), (Truth::False, 0));
    }

    #[test]
    fn a_moved_clock_still_reads_the_clock() {
        let due = compare(name("stopped"), ComparisonOp::LtEq, minus(now(), name("ttl")));
        assert_eq!(read(&due), (Truth::False, 0));
    }

    #[test]
    fn a_deadline_that_is_set_is_read_as_it_always_was() {
        let due = compare(plus(name("started"), name("ttl")), ComparisonOp::LtEq, now());
        assert_eq!(read(&due), (Truth::True, 0));
        let not_yet = compare(plus(name("started"), name("ttl")), ComparisonOp::Gt, now());
        assert_eq!(read(&not_yet), (Truth::False, 0));
    }

    /// Whether a `null` is still ahead is not a question it answers.
    #[test]
    fn a_null_asked_to_be_ahead_of_the_clock_stays_undecided_with_a_reason() {
        assert_eq!(read(&compare(name("stopped"), ComparisonOp::Gt, now())), (Truth::Unknown, 1));
        assert_eq!(read(&compare(now(), ComparisonOp::Lt, name("stopped"))), (Truth::Unknown, 1));
        assert_eq!(read(&compare(name("stopped"), ComparisonOp::Eq, now())), (Truth::False, 0));
        // Not a deadline question, and `null != now` is true: read as one it
        // would turn into a silent false.
        assert_eq!(read(&compare(name("stopped"), ComparisonOp::NotEq, now())), (Truth::True, 0));
    }

    /// The offset may be written first: `config.ttl + stopped_at`.
    #[test]
    fn a_null_deadline_with_the_duration_first_has_not_passed() {
        let due = compare(plus(name("ttl"), name("stopped")), ComparisonOp::LtEq, now());
        assert_eq!(read(&due), (Truth::False, 0));
        // But a duration less a missing timestamp is not a deadline moved.
        let odd = compare(minus(name("ttl"), name("stopped")), ComparisonOp::LtEq, now());
        assert_eq!(read(&odd).0, Truth::Unknown);
    }

    /// An offset nobody knows might not have been a duration at all, and one
    /// that is a timestamp makes no deadline; both are the evaluator's to say.
    #[test]
    fn a_null_moved_by_something_not_a_known_duration_stays_undecided() {
        let vague = compare(plus(name("stopped"), name("vague")), ComparisonOp::LtEq, now());
        assert_eq!(read(&vague).0, Truth::Unknown);
        let stamp = compare(plus(name("stopped"), name("stamp")), ComparisonOp::LtEq, now());
        let (truth, reasons) = read(&stamp);
        assert_eq!(truth, Truth::Unknown);
        assert!(reasons > 0, "an unknown with no reason");
    }

    /// Only the clock: `stopped <= started` is not asking whether anything is
    /// due, and a `null` there is as undecided as anywhere else.
    #[test]
    fn a_null_compared_with_something_other_than_the_clock_stays_undecided() {
        let other = compare(name("stopped"), ComparisonOp::LtEq, name("started"));
        assert_eq!(read(&other), (Truth::Unknown, 1));
        let moved =
            compare(name("stopped"), ComparisonOp::LtEq, plus(name("started"), name("ttl")));
        assert_eq!(read(&moved), (Truth::Unknown, 1));
    }
}
