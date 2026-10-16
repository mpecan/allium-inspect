//! `for x in collection [where filter]: …` as a postcondition.
//!
//! Its own file because it is its own question — which members the clauses
//! inside are applied to, and what is said about the ones nobody could decide
//! — and because `apply` is long enough without it.

use allium_parser::ast::{Expr, ForBinding};
use inspect_model::SpecGraph;

use super::{Application, Applied};
use crate::{
    eval::{Env, Evaluation, Unresolved, element_scope, eval, span_of},
    truth::Truth,
    value::Value,
    world::World,
};

impl Application<'_> {
    /// `for x in collection [where filter]: …`, as a postcondition.
    ///
    /// The body is applied once per member the filter keeps, and not at all
    /// for a collection with no members: `for w in Hooks` over a world with no
    /// hooks emits nothing, and that is decided rather than undecided.
    ///
    /// The filter is read the way a `where` is anywhere else — the member's
    /// own fields bare, and the loop's name bound to it. It used to be
    /// dropped, so every member was iterated whatever the filter said.
    ///
    /// A member the filter cannot decide about is skipped, and the reason says
    /// which member and which iteration: applying the body would be treating
    /// the unknown as true, and dropping it silently as false.
    pub(super) fn iteration(
        &mut self,
        binding: &ForBinding,
        collection: &Expr,
        filter: Option<&Expr>,
        body: &Expr,
    ) -> Applied {
        if let Some(why) = destructured(
            binding,
            collection,
            self.against.source,
            "the clauses inside it were not applied",
        ) {
            let mut applied = Applied::default();
            applied.left_undone(why);
            return applied;
        }
        let binding = bound_name(binding);
        let body = body.clone();
        let header = header(&binding, collection, self.against.source);

        let evaluated = self.evaluate(collection);
        let items = match evaluated.value {
            Value::Set(items) => items,
            other => {
                // Something that is not a collection ranges over nothing the
                // simulator can name, and the clauses inside were not applied.
                // Said first, even when the collection gave its own reason:
                // that names what could not be read, not what went unapplied.
                let quoted = span_of(collection)
                    .and_then(|span| span.slice(self.against.source))
                    .map(str::to_owned);
                let reason = match (&other, &quoted) {
                    (Value::Unknown, Some(text)) => format!(
                        "{header}: `{text}` was never settled, so the clauses inside it were not \
                         applied"
                    ),
                    _ => format!(
                        "{header} ranges over {}, so the clauses inside it were not applied",
                        other.described()
                    ),
                };
                let mut applied = Applied::note(evaluated.unresolved);
                applied.left_undone(Unresolved {
                    reason,
                    expression: quoted,
                    span: span_of(collection),
                });
                return applied;
            }
        };
        // A collection that came back with reasons may be short of members: a
        // relationship leaves out every candidate its `with` could not decide
        // on, and says so only in a note. Iterating the rest as if they were
        // all would be reading "undecided" as "not a member" — the clauses
        // were not applied to whatever it left out, and that is said.
        let incomplete = (!evaluated.unresolved.is_empty()).then(|| {
            crate::per_member::missing_members(
                &header,
                &evaluated.unresolved,
                "the clauses inside it were not applied to them",
            )
        });
        let mut applied = Applied::note(evaluated.unresolved);
        if let Some(reason) = incomplete {
            applied.left_undone(Unresolved {
                reason,
                expression: span_of(collection)
                    .and_then(|span| span.slice(self.against.source))
                    .map(str::to_owned),
                span: span_of(collection),
            });
        }

        for item in items {
            if let Some(filter) = filter {
                let kept = self.kept(&binding, &item, filter);
                match kept.truth() {
                    // Decided, so whatever the filter could not read did not
                    // matter — `or` settled by its left side — and a decided
                    // filter leaves no note about it.
                    Truth::True => {}
                    Truth::False => continue,
                    // The iteration's reason first, then the filter's own:
                    // "nothing is bound to `active`" alone does not say a
                    // member went unapplied.
                    Truth::Unknown => {
                        let mut skipped = Applied::note(kept.unresolved);
                        skipped.left_undone(Unresolved {
                            reason: format!(
                                "{header} could not tell whether its filter keeps {}, so the \
                                 clauses inside it were not applied to it",
                                item.render()
                            ),
                            expression: span_of(filter)
                                .and_then(|span| span.slice(self.against.source))
                                .map(str::to_owned),
                            span: span_of(filter),
                        });
                        applied.merge(skipped);
                        continue;
                    }
                }
            }
            let restore = self.bindings.insert(binding.clone(), item);
            let step = self.apply(&body);
            applied.merge(step);
            match restore {
                Some(previous) => self.bindings.insert(binding.clone(), previous),
                None => self.bindings.remove(&binding),
            };
        }
        applied
    }

    /// Whether `filter` keeps `item`; see [`keeps`].
    fn kept(&self, binding: &str, item: &Value, filter: &Expr) -> Evaluation {
        keeps(self.scope(), self.against.spec, binding, item, filter)
    }
}

/// The name a `for` binds each member to: the first, when it destructures —
/// though a destructured loop is refused before this is asked; see
/// [`destructured`].
pub(crate) fn bound_name(binding: &ForBinding) -> String {
    match binding {
        ForBinding::Single(ident) => ident.name.clone(),
        ForBinding::Destructured(parts, _) => {
            parts.first().map_or_else(|| "it".to_owned(), |ident| ident.name.clone())
        }
    }
}

/// Why a loop that destructures its members was not read: `for (a, b) in …`.
///
/// The simulator binds a member to one name. Binding it to the first of the
/// written names and leaving the rest unbound would let a trigger argument or
/// a rule-level `let` spelled like the second answer for it, and decide the
/// clause about the wrong thing — so a destructured loop is not read at all,
/// and this says so, quoting the binding as written. `None` for a loop that
/// names its member once.
pub(crate) fn destructured(
    binding: &ForBinding,
    collection: &Expr,
    source: &str,
    consequence: &str,
) -> Option<Unresolved> {
    let ForBinding::Destructured(parts, span) = binding else { return None };
    let written = source.get(span.start..span.end).filter(|text| !text.is_empty()).map_or_else(
        || {
            let names: Vec<&str> = parts.iter().map(|part| part.name.as_str()).collect();
            format!("({})", names.join(", "))
        },
        str::to_owned,
    );
    let header = header(&written, collection, source);
    Some(Unresolved {
        reason: format!(
            "{header} takes each member apart into names, which this simulator does not do, so \
             {consequence}"
        ),
        expression: Some(written),
        span: Some(inspect_model::Span::new(span.start, span.end)),
    })
}

/// `` `for h in r.hooks` ``, quoting the collection as the spec wrote it.
pub(crate) fn header(binding: &str, collection: &Expr, source: &str) -> String {
    match span_of(collection).and_then(|span| span.slice(source)).filter(|text| !text.is_empty()) {
        Some(text) => format!("`for {binding} in {text}`"),
        None => format!("`for {binding} in`"),
    }
}

/// Whether `filter` keeps `item`, read in `outer` with the member's fields bare
/// and the loop's name bound to it.
///
/// A field the member declares and does not hold is no longer whatever the
/// rule bound under the same name. `when: Raised(active)` beside
/// `for w in Hooks where active = true` reads the *hook's* `active`, and a hook
/// nobody gave one is undecided — reading the trigger's instead would decide
/// the filter about the wrong thing, and apply the body to a member nobody said
/// it kept.
///
/// Shared by the fan-out of a loop's postconditions and the reading of its
/// preconditions, so the two cannot disagree about which members a loop has.
pub(crate) fn keeps(
    mut outer: Env<'_>,
    spec: &SpecGraph,
    binding: &str,
    item: &Value,
    filter: &Expr,
) -> Evaluation {
    for field in unheld(spec, outer.world, item) {
        outer.bindings.remove(&field);
    }
    let mut scope = element_scope(&outer, item);
    scope.bindings.insert(binding.to_owned(), item.clone());
    eval(filter, &scope)
}

/// The fields `item`'s entity declares that `item` does not hold.
fn unheld(spec: &SpecGraph, world: &World, item: &Value) -> Vec<String> {
    let Value::Ref(id) = item else { return Vec::new() };
    let Some(instance) = world.instance(id) else { return Vec::new() };
    let declared = super::entity_node(spec, &instance.module, &instance.entity)
        .and_then(|node| node.detail.as_entity());
    declared
        .map(|entity| {
            entity
                .fields
                .iter()
                .filter(|field| !instance.fields.contains_key(&field.name))
                .map(|field| field.name.clone())
                .collect()
        })
        .unwrap_or_default()
}
