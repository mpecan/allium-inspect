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

use super::{Env, Told, Unresolved, span_of};
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

/// Whether reading `field` of `instance`, holding `value`, has something to
/// say: it is undecided, or a collection holding something undecided that its
/// write said why about. A bare `{x}` nobody explained is left as it is read.
pub(crate) fn undecided(instance: &Instance, field: &str, value: &Value) -> bool {
    value.is_unknown() || !instance.why(field).is_empty()
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

/// Keep `why` for `name` in `told` when `value` is undecided, and forget any
/// reason for it otherwise.
///
/// The one rule for every map of bound names — an environment's, an
/// application's, a rule's own `let`s — so they cannot drift apart. A reason
/// kept beside a settled value would be read as explaining whatever unknown is
/// bound under that name next; an empty one is no reason at all.
pub(crate) fn remember(told: &mut Told, name: &str, value: &Value, why: Vec<Unresolved>) {
    if value.holds_unknown() && !why.is_empty() {
        told.insert(name.to_owned(), why);
    } else {
        told.remove(name);
    }
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
        remember(&mut self.told, &name, &value, why);
        self.bindings.insert(name, value);
    }

    /// Every field `instance` stores, bound bare, each undecided one with the
    /// note reading it back gives.
    pub fn bind_stored(&mut self, instance: &Instance) {
        for (field, value) in &instance.fields {
            let why = if undecided(instance, field, value) {
                vec![stored_unknown(instance, field)]
            } else {
                Vec::new()
            };
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

#[cfg(test)]
mod tests {
    use inspect_model::Span;

    use super::{Told, Unresolved, anchored, remember, stored_unknown};
    use crate::{
        eval::Env,
        value::{EntityId, Instance, Value},
        world::World,
    };

    fn note(reason: &str, expression: Option<&str>) -> Unresolved {
        Unresolved {
            reason: reason.to_owned(),
            expression: expression.map(str::to_owned),
            span: None,
        }
    }

    fn written(why: Vec<Unresolved>) -> Instance {
        let mut instance = Instance::new(EntityId::new("Box", 1), "Box", "m");
        instance.set_explained("out", Value::Unknown, why);
        instance
    }

    #[test]
    fn a_write_reason_says_where_it_was_when_it_does_not_already() {
        let read =
            stored_unknown(&written(vec![note("nothing answers it", Some("summary(box)"))]), "out");
        assert_eq!(
            read.reason,
            "`Box#1.out` was set to a value nothing settled — when it was written: \
             nothing answers it (in `summary(box)`)"
        );
    }

    #[test]
    fn a_write_reason_that_names_its_expression_is_not_told_it_twice() {
        let read =
            stored_unknown(&written(vec![note("`box.title` is unset", Some("box.title"))]), "out");
        assert_eq!(
            read.reason,
            "`Box#1.out` was set to a value nothing settled — when it was written: \
             `box.title` is unset"
        );
    }

    #[test]
    fn a_note_with_somewhere_to_be_already_is_not_moved() {
        // Anchored by expression alone — a quoted sub-expression with no span —
        // is still anchored, and the read must not overwrite what it quotes.
        let source = "box.out";
        let kept = anchored(note("why", Some("summary(box)")), Some(Span::new(0, 7)), source);
        assert_eq!(kept.expression.as_deref(), Some("summary(box)"));
        assert_eq!(kept.span, None);

        let placed = anchored(note("why", None), Some(Span::new(0, 7)), source);
        assert_eq!(placed.expression.as_deref(), Some("box.out"));
        assert_eq!(placed.span, Some(Span::new(0, 7)));
    }

    #[test]
    fn only_an_undecided_name_with_something_to_say_is_remembered() {
        let mut told = Told::new();
        remember(&mut told, "s", &Value::Unknown, vec![note("why", None)]);
        assert_eq!(told.get("s").map(Vec::len), Some(1));

        // Settled since: the old reason no longer explains it.
        remember(&mut told, "s", &Value::Int(1), vec![note("why", None)]);
        assert!(!told.contains_key("s"));

        remember(&mut told, "s", &Value::Unknown, vec![note("why", None)]);
        remember(&mut told, "s", &Value::Unknown, Vec::new());
        assert!(!told.contains_key("s"), "an empty reason is no reason, and replaces the old one");
    }

    #[test]
    fn an_environment_binds_by_the_same_rule() {
        let world = World::new();
        let mut env = Env::new(&world, "m", "s");
        env.set_explained("s", Value::Unknown, vec![note("why", None)]);
        assert_eq!(env.told_of("s", Span::new(0, 1)).len(), 1);
        env.set_explained("s", Value::Str("x".to_owned()), vec![note("why", None)]);
        assert!(env.told.is_empty());
    }
}
