//! What a rule's `ensures` removes, as the evaluator is told it (issue #33).
//!
//! `ensures` clauses are a set of postconditions about the state the rule
//! leaves, not a sequence, and in that state a removed instance has no fields.
//! Read in line order, `not exists upload` above `Blob.created(name:
//! upload.name)` left the read undecided and the same two lines swapped let it
//! through, so the verdict turned on which line the author happened to write
//! first. Told up front what the rule removes, a value it leaves behind —
//! a creation's parameter, an emission's argument, an assigned value — reads
//! a removed instance as gone in either order, and says how to keep what it
//! held.
//!
//! What *decides* which postconditions apply is read in another world: the
//! one the rule found, with what it removes still in it. That world is the
//! application's to build (`Application::decide`); here an environment is
//! told only what to refuse.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

use super::{Env, Evaluation, span_of};
use crate::value::{EntityId, Instance};

/// Nothing removed, for an environment not reading a rule's `ensures`.
pub(super) static NOTHING_REMOVED: BTreeMap<EntityId, Removal> = BTreeMap::new();

/// An instance a rule's `ensures` removes, as [`Env::removing`] is told it.
#[derive(Debug, Clone, PartialEq)]
pub struct Removal {
    /// The rule that removes it, by name; empty when nobody said.
    pub rule: String,
    /// What the removal is about, as written: `upload`, `upload.thumb`.
    pub subject: String,
    /// The clause that removes it, as written: `not exists upload`.
    pub clause: String,
    /// The instance as the rule found it — before any of its clauses ran —
    /// for what decides which postconditions apply.
    pub found: Instance,
}

impl<'a> Env<'a> {
    /// The same environment, reading the `ensures` of a rule that removes
    /// each of `removed`: none of them has a field to read in the state the
    /// rule leaves.
    #[must_use]
    pub fn removing(mut self, removed: &'a BTreeMap<EntityId, Removal>) -> Self {
        self.removed_here = removed;
        self
    }

    /// The instance `id`, as this read sees it: `Err` with how it was removed
    /// when the rule whose `ensures` this reads removes it, otherwise as the
    /// world holds it, or `Ok(None)` when it holds none.
    pub(crate) fn instance(&self, id: &EntityId) -> Result<Option<&'a Instance>, &'a Removal> {
        match self.removed_here.get(id) {
            Some(removal) => Err(removal),
            None => Ok(self.world.instance(id)),
        }
    }
}

/// A read of `id`, which the rule whose `ensures` reads it removes.
///
/// Undecided, and the same whichever line comes first. What the author almost
/// always meant is the value it had before, and the language has a way to say
/// that: a `let` beside the rule's `when`, read before anything changes. A
/// `let` inside `ensures` is not that — it is a name for a read of the state
/// the rule leaves — so the advice says where, not only what.
pub(super) fn removed_here(
    id: &EntityId,
    removal: &Removal,
    whole: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    let written = span_of(whole).and_then(|span| span.slice(env.source));
    let read = written.map_or_else(|| "this read".to_owned(), |text| format!("`{text}`"));
    let keep = match (written, whole) {
        (Some(text), Expr::MemberAccess { field, .. }) => format!("`let {} = {text}`", field.name),
        _ => "it in a `let`".to_owned(),
    };
    let rule = if removal.rule.is_empty() {
        "this rule".to_owned()
    } else {
        format!("rule `{}`", removal.rule)
    };
    let Removal { subject, clause, .. } = removal;
    let named = if subject == id.as_str() { String::new() } else { format!(" (`{id}`)") };
    Evaluation::unknown(
        format!(
            "{rule} removes `{subject}`{named} with `{clause}`, so {read} has nothing to read \
             in the state it leaves, whichever line comes first; to keep what it held, bind \
             {keep} beside the `when` — a `let` inside `ensures` reads the state the rule \
             leaves too"
        ),
        whole,
        env.source,
    )
}
