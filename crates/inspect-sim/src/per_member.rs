//! A `requires` written inside a rule-level `for`, read once per member.
//!
//! ```text
//! rule Muster {
//!     when: Muster(c)
//!     for h in Hooks where h.armed = true:
//!         requires: h.active = true
//!         ensures: h.swept = true
//! }
//! ```
//!
//! The reading taken: a `requires` is a precondition of the **rule**, wherever
//! it is written, and inside a loop it is quantified over the loop — the rule
//! fires only if it holds for every member the filter keeps. One kept member
//! failing refuses the whole rule, and nothing it ensures is applied, to that
//! member or any other. It is not a guard that skips the member: the language
//! already has two of those, the loop's own `where` and an `if` inside it, and
//! reading `requires` as a third would make it mean something different inside
//! a loop from everywhere else. The language does not settle this, and the
//! refusal says which reading was taken, naming the loop and the member that
//! failed, so a reader who meant the other one can see why the rule did not
//! run.
//!
//! So it is decided the way `all` is over the same members, with Kleene's
//! rules:
//!
//! - over a collection with no members — or none the filter keeps — it holds:
//!   there is nothing for it to be false about;
//! - one kept member failing it is false, whatever the others are, and the
//!   refusal names that member and the loop;
//! - otherwise a member it could not be decided for leaves it undecided, and
//!   the reason names the loop and the member before what could not be read;
//! - a member whose filter could not be decided matters only if it fails: a
//!   member that passes passes whether kept or not;
//! - a collection that may be missing members it could not decide on — a
//!   relationship whose `with` it could not settle for a candidate — cannot be
//!   settled by agreeing, because the member it left out was never asked; it is
//!   undecided, naming the loop, unless a member it holds already fails;
//! - a collection that is not one, or was never settled, is undecided, naming
//!   the loop;
//! - a loop that takes its members apart, `for (a, b) in …`, is undecided,
//!   quoting the binding: the simulator binds a member to one name, and leaving
//!   the rest unbound would let a trigger argument or `let` spelled like one of
//!   them answer for it.
//!
//! The members are the ones the fan-out of the loop's `ensures` visits: the
//! same collection, read in the same scope, with the same filter through
//! [`keeps`], so the two halves of one loop cannot disagree about which
//! members it has.

use allium_parser::ast::Expr;
use inspect_model::{PerMember, SpecGraph};

use crate::{
    apply::{bound_name, destructured, header, keeps},
    eval::{Env, Evaluation, Unresolved, eval, span_of},
    truth::Truth,
    value::Value,
};

/// A precondition inside a loop, read for every member: the answer, and when
/// it is `false`, which member refused it.
pub(crate) struct Precondition {
    pub(crate) evaluated: Evaluation,
    /// Said beside the clause when one kept member fails it: the loop, the
    /// member, and the reading that made one member enough. Not a reason the
    /// rule was undecided — it was not — so it is the clause's to show and
    /// never the rule's.
    pub(crate) refused_by: Option<Unresolved>,
}

impl From<Evaluation> for Precondition {
    fn from(evaluated: Evaluation) -> Self {
        Self { evaluated, refused_by: None }
    }
}

/// `clause`, written inside the loop `within`, read for each member it keeps.
///
/// `scope` makes the environment the rule's own preconditions are read in —
/// its arguments and `let`s — once per use, since a member is read in a scope
/// of its own.
pub(crate) fn precondition<'a>(
    within: &PerMember,
    clause: &Expr,
    scope: &dyn Fn() -> Env<'a>,
    spec: &SpecGraph,
) -> Precondition {
    let base = scope();
    let source = base.source;
    let quoted = |node: &Expr| span_of(node).and_then(|span| span.slice(source)).map(str::to_owned);
    if let Some(why) = destructured(
        &within.binding,
        &within.collection,
        source,
        "this precondition could not be read for its members",
    ) {
        return Evaluation { value: Value::Unknown, unresolved: vec![why] }.into();
    }
    let name = bound_name(&within.binding);
    let header = header(&name, &within.collection, source);

    let collection = eval(&within.collection, &base);
    let items = match collection.value {
        Value::Set(items) => items,
        other => {
            let reason = match (&other, quoted(&within.collection)) {
                (Value::Unknown, Some(text)) => format!(
                    "{header}: `{text}` was never settled, so this precondition could not be \
                     read for its members"
                ),
                _ => format!(
                    "{header} ranges over {}, so this precondition could not be read for its \
                     members",
                    other.described()
                ),
            };
            let mut unresolved = vec![Unresolved {
                reason,
                expression: quoted(&within.collection),
                span: span_of(&within.collection),
            }];
            unresolved.extend(collection.unresolved);
            return Evaluation { value: Value::Unknown, unresolved }.into();
        }
    };

    let mut notes = Vec::new();
    let mut undecided = Vec::new();
    for item in items {
        let kept = match &within.filter {
            Some(filter) => keeps(scope(), spec, &name, &item, filter),
            None => Evaluation { value: Value::Bool(true), unresolved: Vec::new() },
        };
        if kept.truth() == Truth::False {
            continue;
        }
        let read = member(within, &name, &item, clause, scope());
        match (kept.truth(), read.truth()) {
            // Passing, it does not matter whether the filter keeps it.
            (_, Truth::True) => notes.extend(read.unresolved),
            // One kept member failing settles it, whatever else is undecided.
            (Truth::True, Truth::False) => {
                let refused_by = Unresolved {
                    reason: format!(
                        "{header}: {} does not satisfy this precondition, and a `requires` inside \
                         a loop is read for every member the loop keeps, so the whole rule is \
                         refused",
                        item.render()
                    ),
                    expression: quoted(clause),
                    span: span_of(clause),
                };
                return Precondition {
                    evaluated: Evaluation {
                        value: Value::Bool(false),
                        unresolved: read.unresolved,
                    },
                    refused_by: Some(refused_by),
                };
            }
            (Truth::True, _) => {
                undecided.push(Unresolved {
                    reason: format!(
                        "{header} could not decide this precondition for {}",
                        item.render()
                    ),
                    expression: quoted(clause),
                    span: span_of(clause),
                });
                undecided.extend(read.unresolved);
            }
            // Failing or undecided, and the filter may or may not keep it:
            // whether it counts is the filter's question, and that is the one
            // named first.
            (_, _) => {
                let filter = within.filter.as_ref();
                let about = if read.truth() == Truth::False {
                    "this precondition does not hold for it"
                } else {
                    "this precondition could not be decided for it either"
                };
                undecided.push(Unresolved {
                    reason: format!(
                        "{header} could not tell whether its filter keeps {}, and {about}",
                        item.render()
                    ),
                    expression: filter.and_then(quoted),
                    span: filter.and_then(span_of),
                });
                undecided.extend(kept.unresolved);
                undecided.extend(read.unresolved);
            }
        }
    }

    // A collection that came back with reasons may have left a member out, and
    // that one was never asked: said whenever it is undecided, so a reader of
    // a member it could not decide about also learns there may be more — the
    // same sentence the fan-out of the loop's `ensures` says.
    let missing = (!collection.unresolved.is_empty()).then(|| {
        let mut unresolved = vec![Unresolved {
            reason: missing_members(
                &header,
                &collection.unresolved,
                "this precondition could not be checked for them",
            ),
            expression: quoted(&within.collection),
            span: span_of(&within.collection),
        }];
        unresolved.extend(collection.unresolved);
        unresolved
    });
    if !undecided.is_empty() {
        undecided.extend(missing.into_iter().flatten());
        return Evaluation { value: Value::Unknown, unresolved: undecided }.into();
    }
    // Every member it holds passed, which does not settle it when it may have
    // left one out.
    if let Some(unresolved) = missing {
        return Evaluation { value: Value::Unknown, unresolved }.into();
    }
    Evaluation { value: Value::Bool(true), unresolved: notes }.into()
}

/// `clause` about one member: the loop's name bound to it, and the `let`s the
/// loop wrote above the clause bound in order.
///
/// What a `let` could not settle is carried with the answer, so a clause that
/// reads one is never undecided without saying why.
fn member(
    within: &PerMember,
    name: &str,
    item: &Value,
    clause: &Expr,
    mut scope: Env<'_>,
) -> Evaluation {
    scope.bindings.insert(name.to_owned(), item.clone());
    let mut carried = Vec::new();
    for (bound, value) in &within.lets {
        let evaluated = eval(value, &scope);
        carried.extend(evaluated.unresolved);
        scope.bindings.insert(bound.clone(), evaluated.value);
    }
    let mut read = eval(clause, &scope);
    read.unresolved.extend(carried);
    read
}

/// "`for w in r.active_watches` may be missing members it could not decide on
/// (`Watch#2` has no `b` set), so …" — the one sentence both halves of a loop
/// say about a collection that came back with reasons.
///
/// With the collection's first reason inside it, because the walk shows a
/// rule's first reason and nothing after it: without the parenthesis a reader
/// was told members may be missing and never which, or why — and the fix, a
/// field the journey did not set on a member it cast, was one line away and
/// out of sight (issue #23).
pub(crate) fn missing_members(header: &str, notes: &[Unresolved], so: &str) -> String {
    let because = notes.first().map(|note| match &note.expression {
        Some(expression) if !note.reason.contains(&format!("`{expression}`")) => {
            format!(" ({}, in `{expression}`)", note.reason)
        }
        _ => format!(" ({})", note.reason),
    });
    format!(
        "{header} may be missing members it could not decide on{}, so {so}",
        because.unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::{Unresolved, missing_members};

    fn note(reason: &str, expression: Option<&str>) -> Unresolved {
        Unresolved {
            reason: reason.to_owned(),
            expression: expression.map(str::to_owned),
            span: None,
        }
    }

    /// A reason that does not quote what it is about is told where.
    #[test]
    fn a_reason_that_does_not_quote_its_expression_says_where() {
        let notes = [note("could not tell", Some("active = true"))];
        assert_eq!(
            missing_members("`for w in X`", &notes, "so-text"),
            "`for w in X` may be missing members it could not decide on (could not tell, in \
             `active = true`), so so-text"
        );
    }

    /// One that already quotes it is not told twice.
    #[test]
    fn a_reason_that_quotes_its_expression_is_not_told_it_again() {
        let notes = [note("nothing is bound to `active = true`", Some("active = true"))];
        assert_eq!(
            missing_members("`for w in X`", &notes, "so-text"),
            "`for w in X` may be missing members it could not decide on (nothing is bound to \
             `active = true`), so so-text"
        );
    }

    /// The first reason, and only the first: the walk shows one.
    #[test]
    fn the_first_reason_is_the_one_given() {
        let notes = [note("first", None), note("second", None)];
        assert_eq!(
            missing_members("`for w in X`", &notes, "so-text"),
            "`for w in X` may be missing members it could not decide on (first), so so-text"
        );
    }

    /// No reasons, no parenthesis.
    #[test]
    fn with_no_reason_there_is_no_parenthesis() {
        assert_eq!(
            missing_members("`for w in X`", &[], "so-text"),
            "`for w in X` may be missing members it could not decide on, so so-text"
        );
    }
}
