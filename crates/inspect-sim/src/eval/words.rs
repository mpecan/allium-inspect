//! What a bare name inside a definition means when nothing binds it.
//!
//! Inside an entity's definition the stored fields an instance holds are in
//! scope bare, and the ones nobody set are not — so an unset field, a word
//! the spec declares as a state, and a name nothing in the spec gives any
//! meaning all used to arrive at the same "nothing is bound". A comparison
//! then read the first as a state of the same spelling: `other != status`,
//! with `other` unset, was `true` because the state `other` is not `waiting`
//! (issue #21). And a branch returning the second — `if status = waiting:
//! waiting else: running` — was undecided, because nothing beside it said it
//! was a state.
//!
//! Each is now told apart by what the spec declares rather than by what
//! happens to be bound: the fields an entity stores and the states the spec
//! set declares are both on [`inspect_model::Derivations`].

use allium_parser::ast::Ident;
use inspect_model::Span;

use super::{Env, Evaluation, Unresolved};
use crate::value::{EntityId, Instance, Value};

/// The reason given for a field nobody set: "`Member#1` has no `x` set".
///
/// Shared for the reason [`super::nothing_bound`] is: the journey walk says it
/// about the name the journey cast rather than the world's id, and has to
/// recognise it to do so.
#[must_use]
pub fn unset_field(id: &EntityId, field: &str) -> String {
    format!("`{id}` has no `{field}` set")
}

/// `name`, unbound, read inside a definition on `instance`: one of its own
/// stored fields nobody set, or a state the spec declares — or neither, and
/// the caller goes on to say nothing is bound to it.
pub(super) fn own_field_or_state(
    instance: &Instance,
    ident: &Ident,
    env: &Env<'_>,
) -> Option<Evaluation> {
    let name = &ident.name;
    // One of its own fields that nobody set. Said as the field it is: as
    // "nothing is bound" it read like a word, and a comparison against a state
    // took it for one.
    if let Some(unset) = unset_stored(instance, ident, env) {
        return Some(unset);
    }
    // A word the spec declares as a state — `agg: if status = waiting: waiting
    // else: running` returns the state. A comparison already read `waiting`
    // that way when the other side was a state; a branch's value has nothing
    // beside it to say so, and the declaration is what does. Only a declared
    // state, the test the emission path puts to a word: anything else may be a
    // construct this simulator does not model, and stays undecided, naming
    // itself.
    (!env.parameters.contains(name) && env.derived.states.contains(name))
        .then(|| Evaluation::known(Value::Enum(name.clone())))
}

/// `name`, unbound, read inside a `where` or `with` filter: a stored field of
/// the element being looked at that the element never set.
///
/// Asked before anything about `this`, because inside a definition the two
/// differ — `live: docs where status = active` on a `Shelf` asks each doc for
/// its `status` — and a doc that never set one is a doc nobody can say is
/// live, not a shelf with a word in it.
pub(super) fn unset_element_field(ident: &Ident, env: &Env<'_>) -> Option<Evaluation> {
    let element = env.world.instance(env.element.as_ref()?)?;
    unset_stored(element, ident, env)
}

/// `name`, unbound, read inside a `where` or `with` filter: a field the element
/// being looked at computes rather than stores.
///
/// Computed the way any computed field is — [`super::field_of`]'s way — in the
/// element's own scope: `this` is the element and only its fields are bare.
/// `active_watches: watches where is_watching = true`, with `is_watching: a or
/// b` on the watch, asks each watch for its `a` and `b`; when one is unset the
/// reason is that watch's, naming it and the field. Before this, nothing asked
/// the element: the name fell through to `this`, the repo, which has no such
/// field, and every member came back "nothing is bound" however fully the
/// world described it (issue #23).
///
/// Asked before `this`'s computed fields, for the reason the element's stored
/// fields shadow `this`'s: inside the filter, a bare name is the element's.
/// `this`'s *stored* fields, bound bare inside its definitions, are unbound
/// for any name the element declares by `element_scope`, so they do not
/// answer first either.
///
/// A function the element defines, named bare, is the element's too: it takes
/// an argument, and says so, as [`super::field_of`] does for `watch.f`.
pub(super) fn computed_element_field(ident: &Ident, env: &Env<'_>) -> Option<Evaluation> {
    let element = env.world.instance(env.element.as_ref()?)?;
    if let Some((definition, key)) = env.derivation(element, &ident.name) {
        return Some(super::eval(definition, &env.computing(element, key)));
    }
    let (function, _) = env.function(element, &ident.name)?;
    Some(super::calls::read_bare(&ident.name, function, env))
}

/// "`instance` has no `name` set", when `name` is a field its entity stores.
fn unset_stored(instance: &Instance, ident: &Ident, env: &Env<'_>) -> Option<Evaluation> {
    let name = &ident.name;
    let key = inspect_model::derived_key(&instance.module, &instance.entity, name);
    env.derived.stored.contains(&key).then(|| Evaluation {
        value: Value::Unknown,
        unresolved: vec![Unresolved {
            reason: unset_field(&instance.id, name),
            expression: Some(name.clone()),
            span: Some(Span::new(ident.span.start, ident.span.end)),
        }],
    })
}

impl Env<'_> {
    /// Whether an unbound `name` could only be a word — a state it spells —
    /// rather than something in scope that nobody has given a value.
    ///
    /// Three things read exactly like a word nothing binds, and reading any of
    /// them as the state of the same spelling answers a question about a
    /// value nobody knows:
    ///
    /// - a name bound to a value that is itself undecided;
    /// - a parameter of the rule that nobody passed;
    /// - a field of `this`, or of the element a filter is looking at: a stored
    ///   one nobody set, or a computed one that came back undecided. `stale:
    ///   agg != status` with `agg` undecided is undecided, not a comparison
    ///   against a state called `agg`; and in `docs where status = active` a
    ///   doc with no `status` is not one whose status is the state `status`.
    pub(crate) fn could_be_a_state(&self, name: &str) -> bool {
        if self.bindings.contains_key(name) || self.parameters.contains(name) {
            return false;
        }
        let this = match self.bindings.get("this") {
            Some(Value::Ref(id)) => Some(id),
            _ => None,
        };
        ![this, self.element.as_ref()].into_iter().flatten().any(|id| self.has(id, name))
    }

    /// Whether the entity `id` is an instance of stores, computes or defines
    /// `name` — whether or not it holds a value for it.
    pub(crate) fn has(&self, id: &EntityId, name: &str) -> bool {
        let Some(instance) = self.world.instance(id) else { return false };
        let key = inspect_model::derived_key(&instance.module, &instance.entity, name);
        self.derived.stored.contains(&key)
            || self.derived.fields.contains_key(&key)
            || self.derived.functions.contains_key(&key)
    }
}
