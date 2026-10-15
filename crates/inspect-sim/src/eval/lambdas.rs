//! `items.all(i => …)` and `items.any(i => …)`: a collection asked about
//! through a lambda.
//!
//! These are the quantifiers a spec writes *inline*, beside the `for x in …:`
//! form an invariant is made of, and they mean the same two things: every
//! member satisfies the predicate, or at least one does. They used to fall
//! through to the function-call path, which looked for a function called `all`
//! on a collection, found none, and reported "`all` was called on a collection,
//! which defines nothing" — however completely a journey had said what was in
//! it (issue #7). That sent authors to rewrite real logic as black-box
//! predicates a journey then had to stipulate, which hides the logic it was
//! meant to check.
//!
//! Kleene throughout, by [`Truth::all`] and [`Truth::any`]: a member that
//! settles the answer settles it whatever the others are, and a member nobody
//! described leaves the answer open only when nothing else closed it. Every
//! reason a member's predicate gave is kept, the same as `for … in` and `and`
//! keep theirs — and where a member came back undecided *without* one, this
//! adds it, because an unknown with no reason is indistinguishable from a bug.
//!
//! Only these two names, only on a collection, and only with a single named
//! parameter. Anything else keeps the path it had, so an entity that defines a
//! function called `all` still has it called, and a lambda reached anywhere
//! else is still named as not simulated.

use allium_parser::ast::{CallArg, Expr};

use super::{Env, Evaluation, Unresolved, ast::truth_value, eval, unresolved_at};
use crate::{truth::Truth, value::Value};

/// Which question is being asked of every member.
#[derive(Debug, Clone, Copy)]
enum Quantifier {
    All,
    Any,
}

impl Quantifier {
    /// The quantifier a method name spells, if it spells one.
    fn named(method: &str) -> Option<Self> {
        match method {
            "all" => Some(Quantifier::All),
            "any" => Some(Quantifier::Any),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Quantifier::All => "all",
            Quantifier::Any => "any",
        }
    }

    /// The answer every member agreeing gives, which a member nobody asked
    /// could still overturn.
    fn unsettled(self) -> Truth {
        match self {
            Quantifier::All => Truth::True,
            Quantifier::Any => Truth::False,
        }
    }

    fn over(self, verdicts: Vec<Truth>) -> Truth {
        match self {
            Quantifier::All => Truth::all(verdicts),
            Quantifier::Any => Truth::any(verdicts),
        }
    }
}

/// `object.all(param => body)` or `object.any(param => body)`.
///
/// `None` when this is not one of those — another method, or arguments other
/// than a single lambda — and when `object` is an instance rather than a
/// collection: then `all` is a function that instance's entity may define, and
/// answering it is the call path's job, not this one's.
pub(super) fn quantified(
    whole: &Expr,
    object: &Expr,
    method: &str,
    args: &[CallArg],
    env: &Env<'_>,
) -> Option<Evaluation> {
    let quantifier = Quantifier::named(method)?;
    let [CallArg::Positional(Expr::Lambda { param, body, .. })] = args else {
        return None;
    };
    let (param, body): (&Expr, &Expr) = (param, body);
    let on = eval(object, env);
    // A collection that came back with reasons may be short of members: a
    // relationship leaves out every candidate its `with` could not decide on,
    // and says so only in a note. See `undecided_by_omission`.
    let incomplete = !on.unresolved.is_empty();
    let mut unresolved = on.unresolved;
    let items = match on.value {
        Value::Set(items) => items,
        Value::Ref(_) => return None,
        // Undecided already, and it said why — or it did not, and then this
        // says it, because the reader is otherwise told nothing.
        Value::Unknown => {
            if unresolved.is_empty() {
                unresolved.push(unresolved_at(
                    format!("`{}` was asked of something nothing settled", quantifier.word()),
                    object,
                    env.source,
                ));
            }
            return Some(Evaluation { value: Value::Unknown, unresolved });
        }
        other => {
            unresolved.push(unresolved_at(
                format!(
                    "`{}` asks about every member of a collection, and was called on {}",
                    quantifier.word(),
                    other.described()
                ),
                whole,
                env.source,
            ));
            return Some(Evaluation { value: Value::Unknown, unresolved });
        }
    };

    // `i => …`. A lambda's parameter is an expression in the tree, and the only
    // one that binds anything is a plain name.
    let Expr::Ident(name) = param else {
        unresolved.push(unresolved_at(
            "a lambda whose parameter is not a single name is not simulated",
            param,
            env.source,
        ));
        return Some(Evaluation { value: Value::Unknown, unresolved });
    };

    let mut verdicts = Vec::with_capacity(items.len());
    for item in &items {
        // Only the parameter is bound. Unlike `where`, a lambda names its
        // element, so the element's fields are not put in scope bare — that
        // would let a field called `box` shadow the rule's own `box`.
        let scope = env.scoped(&name.name, item.clone());
        let held = eval(body, &scope);
        let verdict = held.truth();
        let said_why = !held.unresolved.is_empty();
        unresolved.extend(held.unresolved);
        if verdict == Truth::Unknown {
            let reason = match (item, &held.value) {
                // The member is the thing nobody settled. Whatever the
                // predicate said about reading it, this is the cause, and it
                // belongs to the collection rather than to the predicate.
                (Value::Unknown, _) => Some((
                    format!(
                        "a member of this collection is not known, so `{}` cannot check it",
                        quantifier.word()
                    ),
                    object,
                )),
                _ if said_why => None,
                (_, Value::Unknown) => Some((
                    format!("the predicate could not be decided for `{}`", item.render()),
                    body,
                )),
                (_, other) => Some((format!("{} is not a condition", other.described()), body)),
            };
            if let Some((reason, at)) = reason {
                unresolved.push(unresolved_at(reason, at, env.source));
            }
        }
        verdicts.push(verdict);
    }

    let (answer, omitted) = undecided_by_omission(
        quantifier.over(verdicts),
        quantifier.unsettled(),
        incomplete.then_some(object),
        quantifier.word(),
        env,
    );
    unresolved.extend(omitted);
    Some(Evaluation { value: truth_value(answer), unresolved })
}

/// `answer`, unless it is the one a member the collection left out could have
/// overturned — `incomplete` is the collection, when it may have left one out.
///
/// A collection evaluated with reasons may have dropped members it could not
/// decide on — `items: Item with box = this` leaves out an item whose `box`
/// nobody stated, and keeps only a note. The members it *does* hold can still
/// settle the question: one item of the wrong kind refutes `all` whatever else
/// is in the box. What they cannot do is settle it by agreeing, because the
/// member left out was never asked. So `all` coming out true, or `any` coming
/// out false — `unsettled` — is turned into undecided, with a reason, rather
/// than reported as a conclusion nobody reached.
pub(super) fn undecided_by_omission(
    answer: Truth,
    unsettled: Truth,
    incomplete: Option<&Expr>,
    word: &str,
    env: &Env<'_>,
) -> (Truth, Option<Unresolved>) {
    match incomplete {
        Some(collection) if answer == unsettled => (
            Truth::Unknown,
            Some(unresolved_at(
                format!(
                    "this collection may be missing members it could not decide on, so `{word}` \
                     cannot rest on the ones it holds"
                ),
                collection,
                env.source,
            )),
        ),
        _ => (answer, None),
    }
}
