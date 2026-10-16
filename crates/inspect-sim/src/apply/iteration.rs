//! `for x in collection [where filter]: …` as a postcondition.
//!
//! Its own file because it is its own question — which members the clauses
//! inside are applied to, and what is said about the ones nobody could decide
//! — and because `apply` is long enough without it.

use allium_parser::ast::{Expr, ForBinding};

use super::{Application, Applied};
use crate::{
    eval::{Evaluation, Unresolved, element_scope, eval, span_of},
    truth::Truth,
    value::Value,
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
        let binding = match binding {
            ForBinding::Single(ident) => ident.name.clone(),
            ForBinding::Destructured(parts, _) => {
                parts.first().map_or_else(|| "it".to_owned(), |ident| ident.name.clone())
            }
        };
        let body = body.clone();
        let header = match span_of(collection)
            .and_then(|span| span.slice(self.against.source))
            .filter(|text| !text.is_empty())
        {
            Some(text) => format!("`for {binding} in {text}`"),
            None => format!("`for {binding} in`"),
        };

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
        let incomplete = !evaluated.unresolved.is_empty();
        let mut applied = Applied::note(evaluated.unresolved);
        if incomplete {
            applied.left_undone(Unresolved {
                reason: format!(
                    "{header} may be missing members it could not decide on, so the clauses \
                     inside it were not applied to them"
                ),
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

    /// Whether `filter` keeps `item`, read with the member's fields bare and
    /// the loop's name bound to it.
    ///
    /// A field the member declares and does not hold is no longer whatever
    /// the rule bound under the same name. `when: Raised(active)` beside
    /// `for w in Hooks where active = true` reads the *hook's* `active`, and
    /// a hook nobody gave one is undecided — reading the trigger's instead
    /// would decide the filter about the wrong thing, and apply the body to a
    /// member nobody said it kept.
    fn kept(&self, binding: &str, item: &Value, filter: &Expr) -> Evaluation {
        let mut outer = self.scope();
        for field in self.unheld(item) {
            outer.bindings.remove(&field);
        }
        let mut scope = element_scope(&outer, item);
        scope.bindings.insert(binding.to_owned(), item.clone());
        eval(filter, &scope)
    }

    /// The fields `item`'s entity declares that `item` does not hold.
    fn unheld(&self, item: &Value) -> Vec<String> {
        let Value::Ref(id) = item else { return Vec::new() };
        let Some(instance) = self.world.instance(id) else { return Vec::new() };
        let declared = super::entity_node(self.against.spec, &instance.module, &instance.entity)
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
}
