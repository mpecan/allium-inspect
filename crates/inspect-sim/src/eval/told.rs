//! Why a bound name holds an unknown, kept beside the binding (issue #27).
//!
//! A binding is a bare [`Value`], so a name bound to something undecided
//! arrived at its reader saying nothing: a stored field `out` read bare inside
//! `shown: out + "!"`, or `let s = summary(box)` read as `s` a line later. The
//! reader then either blamed itself ("`+` was given a side nothing settled") or
//! said nothing at all, which stipulation 1 calls indistinguishable from a bug.
//!
//! Two kinds of note are told, and they are anchored differently:
//!
//! - A `let`'s own notes are about its value, in this rule, and keep the span
//!   they were given.
//! - A stored field's note is about the *read*, so it has no span until the
//!   read gives it one. What the write could not settle is said inside it, in
//!   the past tense: it named a sub-expression of another rule, at another
//!   time, and a stipulation since may answer what it asked.

use allium_parser::ast::Expr;
use inspect_model::Span;

use super::{Env, Unresolved, span_of};
use crate::value::{Instance, Value};

/// The note for reading `field` of `instance` while it holds a stored unknown.
///
/// Not anchored: whoever reads it says where (see [`anchored`]). The reasons
/// the write gave come along as context, said as what was so *then*.
pub(crate) fn stored_unknown(instance: &Instance, field: &str) -> Unresolved {
    let mut reason = format!("`{}.{field}` was set to a value nothing settled", instance.id);
    let then: Vec<String> = instance
        .why(field)
        .iter()
        .map(|note| match &note.expression {
            Some(expression) if !note.reason.contains(&format!("`{expression}`")) => {
                format!("{} (in `{expression}`)", note.reason)
            }
            _ => note.reason.clone(),
        })
        .collect();
    if !then.is_empty() {
        reason.push_str(" — when it was written: ");
        reason.push_str(&then.join("; "));
    }
    Unresolved { reason, expression: None, span: None }
}

/// `note`, anchored at `span` when it was not anchored already.
pub(crate) fn anchored(mut note: Unresolved, span: Option<Span>, source: &str) -> Unresolved {
    if note.span.is_none() && note.expression.is_none() {
        note.span = span;
        note.expression = span.and_then(|span| span.slice(source)).map(str::to_owned);
    }
    note
}

/// The same, anchored at an expression.
pub(crate) fn anchored_at(note: Unresolved, at: &Expr, source: &str) -> Unresolved {
    anchored(note, span_of(at), source)
}

impl Env<'_> {
    /// The same environment with `name` bound to `value`, and — when that is
    /// undecided — `why`, for whatever reads the name to give.
    #[must_use]
    pub fn bind_explained(
        mut self,
        name: impl Into<String>,
        value: Value,
        why: Vec<Unresolved>,
    ) -> Self {
        self.set_explained(name, value, why);
        self
    }

    /// Bind `name` to `value` in place, forgetting why it was undecided before.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.set_explained(name, value, Vec::new());
    }

    /// Bind `name` to `value` in place, with `why` when that is undecided.
    pub fn set_explained(&mut self, name: impl Into<String>, value: Value, why: Vec<Unresolved>) {
        let name = name.into();
        if value.is_unknown() && !why.is_empty() {
            self.told.insert(name.clone(), why);
        } else {
            self.told.remove(&name);
        }
        self.bindings.insert(name, value);
    }

    /// Every field `instance` stores, bound bare, each undecided one with the
    /// note reading it back gives.
    pub fn bind_stored(&mut self, instance: &Instance) {
        for (field, value) in &instance.fields {
            let why =
                if value.is_unknown() { vec![stored_unknown(instance, field)] } else { Vec::new() };
            self.set_explained(field.clone(), value.clone(), why);
        }
    }

    /// Why the bound name `name` is undecided, anchored at the read.
    pub(super) fn told_of(&self, name: &str, span: Span) -> Vec<Unresolved> {
        self.told.get(name).map_or_else(Vec::new, |notes| {
            notes.iter().map(|note| anchored(note.clone(), Some(span), self.source)).collect()
        })
    }
}
