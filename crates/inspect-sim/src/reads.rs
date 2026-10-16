//! Whether a rule's postconditions read a field they may also write.
//!
//! Two siblings that write one field the same value agree only when neither
//! value came from reading it. `CountOnce` and `CountAgain` both say `c.n =
//! c.n + 1`, and both read the `n` the trigger found: each writes `1` over a
//! `0`, so they "agree" on a value no order of the two produces — run one
//! after the other, the second reads what the first wrote and the answer is
//! `2`. A lost update reported as a settled value.
//!
//! By name, and generously: any mention of the field's name outside the
//! target of an assignment is a read, whatever it is a field of, and so is a
//! derived field whose definition mentions it. An over-read only leaves a
//! field undecided that might have been settled, and says why; an under-read
//! states a value nothing produces.

use std::collections::BTreeSet;

use allium_parser::ast::{ComparisonOp, Expr};
use inspect_model::{RuleAst, ingest::subexpressions};

use inspect_model::Derivations as Derived;

/// Whether `rule`'s `let`s or `ensures` read `field`.
pub(crate) fn reads(rule: &RuleAst, field: &str, derived: &Derived) -> bool {
    let mut seen = BTreeSet::new();
    let mut looking = Looking { field, derived, seen: &mut seen };
    rule.lets.iter().any(|(_, value)| looking.read(value))
        || rule.ensures.iter().any(|clause| looking.statement(clause))
}

struct Looking<'a, 'b> {
    field: &'a str,
    derived: &'a Derived,
    /// Derived definitions already looked into, so two that mention each
    /// other are each read once.
    seen: &'b mut BTreeSet<String>,
}

impl Looking<'_, '_> {
    /// A postcondition: an assignment's target is written, not read, and
    /// everything else in it is read.
    fn statement(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Comparison { left, op: ComparisonOp::Eq, right, .. } => {
                let target = match left.as_ref() {
                    Expr::MemberAccess { object, .. } => self.read(object),
                    other => self.read(other),
                };
                target || self.read(right)
            }
            Expr::Block { items, .. } => items.iter().any(|item| self.statement(item)),
            Expr::Conditional { branches, else_body, .. } => {
                branches
                    .iter()
                    .any(|branch| self.read(&branch.condition) || self.statement(&branch.body))
                    || else_body.as_deref().is_some_and(|body| self.statement(body))
            }
            Expr::For { collection, filter, body, .. } => {
                self.read(collection)
                    || filter.as_deref().is_some_and(|filter| self.read(filter))
                    || self.statement(body)
            }
            other => self.read(other),
        }
    }

    /// Whether `expr` mentions the field, or a derived field or function
    /// whose definition does.
    fn read(&mut self, expr: &Expr) -> bool {
        let named = match expr {
            Expr::Ident(ident) => Some(ident.name.as_str()),
            Expr::MemberAccess { field, .. } | Expr::OptionalAccess { field, .. } => {
                Some(field.name.as_str())
            }
            _ => None,
        };
        if let Some(name) = named
            && (name == self.field || self.defined_reading(name))
        {
            return true;
        }
        subexpressions(expr).into_iter().any(|child| self.read(child))
    }

    /// Whether a derived field or function called `name`, on any entity,
    /// reads the field.
    fn defined_reading(&mut self, name: &str) -> bool {
        let suffix = format!(".{name}");
        let derived = self.derived;
        let bodies: Vec<(&String, &Expr)> = derived
            .fields
            .iter()
            .chain(derived.functions.iter().map(|(key, function)| (key, &function.body)))
            .filter(|(key, _)| key.ends_with(&suffix))
            .collect();
        bodies.into_iter().any(|(key, body)| self.seen.insert(key.clone()) && self.read(body))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use allium_parser::{
        Span,
        ast::{BinaryOp, ComparisonOp, CondBranch, Expr, Ident},
    };
    use inspect_model::{Derivations, RuleAst};

    use super::reads;

    const AT: Span = Span { start: 0, end: 0 };

    fn name(text: &str) -> Expr {
        Expr::Ident(Ident { span: AT, name: text.to_owned() })
    }

    fn of(object: Expr, field: &str) -> Expr {
        Expr::MemberAccess {
            span: AT,
            object: Box::new(object),
            field: Ident { span: AT, name: field.to_owned() },
        }
    }

    fn set(target: Expr, value: Expr) -> Expr {
        Expr::Comparison {
            span: AT,
            left: Box::new(target),
            op: ComparisonOp::Eq,
            right: Box::new(value),
        }
    }

    fn plus(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp { span: AT, left: Box::new(left), op: BinaryOp::Add, right: Box::new(right) }
    }

    fn rule(ensures: Vec<Expr>) -> RuleAst {
        RuleAst { ensures, ..RuleAst::default() }
    }

    #[test]
    fn writing_a_field_is_not_reading_it() {
        let derived = Derivations::default();
        let wrote = rule(vec![set(of(name("c"), "n"), name("one"))]);
        assert!(!reads(&wrote, "n", &derived));
    }

    #[test]
    fn an_increment_reads_what_it_writes() {
        let derived = Derivations::default();
        let bump = rule(vec![set(of(name("c"), "n"), plus(of(name("c"), "n"), name("one")))]);
        assert!(reads(&bump, "n", &derived));
        assert!(!reads(&bump, "m", &derived));
    }

    /// A condition is read even when it is an `=`, and a `let` is read.
    #[test]
    fn a_condition_and_a_let_are_reads() {
        let derived = Derivations::default();
        let guarded = rule(vec![Expr::Conditional {
            span: AT,
            branches: vec![CondBranch {
                span: AT,
                condition: set(of(name("c"), "n"), name("zero")),
                body: set(of(name("c"), "n"), name("one")),
            }],
            else_body: None,
        }]);
        assert!(reads(&guarded, "n", &derived));
        let mut held = rule(vec![set(of(name("c"), "n"), name("m"))]);
        held.lets.push(("m".to_owned(), of(name("c"), "n")));
        assert!(reads(&held, "n", &derived));
    }

    /// Through a derived field, and through two that mention each other
    /// without going round for ever.
    #[test]
    fn a_derived_field_that_reads_it_is_a_read() {
        let mut derived = Derivations::default();
        derived.fields.insert("m::C.next".to_owned(), plus(name("n"), name("one")));
        derived.fields.insert("m::C.loop".to_owned(), name("again"));
        derived.fields.insert("m::C.again".to_owned(), name("loop"));
        let via = rule(vec![set(of(name("c"), "n"), of(name("c"), "next"))]);
        assert!(reads(&via, "n", &derived));
        let round = rule(vec![set(of(name("c"), "n"), of(name("c"), "loop"))]);
        assert!(!reads(&round, "n", &derived));
    }
}
