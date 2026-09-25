//! Where somebody is standing when they look at a surface.
//!
//! A surface's `exposes` clause is written about its contexts, its `let`s and
//! whoever it faces, and before any item of it can be asked about, each of
//! those has to be *something*. This is where they become something: a context
//! from the journey's `in` or from the actor, a filter on it checked, a `let`
//! evaluated over the lot.
//!
//! Split from `assert` because it is the half of a `sees` line that is about
//! the surface rather than about the thing being looked at, and because it is
//! the half that three shapes of real spec went wrong in at once: a context
//! with a `where` was not a context, a second context replaced the first, and a
//! `let` was never read.

use std::collections::BTreeMap;

use inspect_model::Boundary;
use inspect_sim::{Value, eval::span_of, value::EntityId};

use crate::{assert::Sight, run::Walker};

/// What an `exposes` clause can refer to, from where this actor stands.
pub(crate) struct Standing {
    /// Every name bound: contexts, the facing binding, and each `let` that
    /// settled.
    pub(crate) bindings: BTreeMap<String, Value>,
    /// Names nothing could bind, with the reason — which is the remedy.
    ///
    /// Not an error up front. `DeviceManagement` is at an identity and at a
    /// link, and a question about the identity's own name is answered whether
    /// or not anybody said which link; refusing it for the link would be this
    /// tool being unable to answer a question that does not involve one.
    pub(crate) missing: BTreeMap<String, String>,
    /// A `let` that dropped what it could not decide, so a *no* reached over
    /// it is not a no.
    pub(crate) unsettled: Option<String>,
    /// A context with a filter that nothing bound. Whether this surface is
    /// open at all is then not known, and a *yes* is not a yes either.
    pub(crate) gated: Option<String>,
}

/// A name `in` gave, and what it names.
struct Named<'a> {
    name: &'a str,
    id: &'a EntityId,
    entity: &'a str,
}

/// One context, the instance standing in it, and the name the journey knows
/// that instance by — for saying which one a filter refused.
struct At<'a> {
    context: &'a inspect_model::Context,
    id: EntityId,
    written: String,
}

/// Why nothing about this surface can be asked from here, when that is known
/// before any item is read.
pub(crate) enum Unstood {
    /// It is not open here: a context's filter said no.
    Closed(String),
    /// Something the journey said cannot be made sense of.
    Undecided(String),
}

impl Walker<'_> {
    /// Bind everything the surface's `exposes` clause may read.
    ///
    /// Each context on its own, and the journey's word first. `in` names
    /// instances, and each goes to the first context of its type not already
    /// taken; a context none of them fits is the actor when the actor is one
    /// of its type, which is the ordinary case and not a guess — `surface
    /// DeviceManagement` is at an `Identity`, Ada is looking, and Ada is an
    /// Identity. Anything else is left unbound, with the remedy.
    pub(crate) fn stand(
        &self,
        boundary: &Boundary,
        sight: &Sight<'_>,
        looking: &EntityId,
        module: &str,
    ) -> Result<Standing, Unstood> {
        let Some(actor) = self.world.instance(looking) else {
            return Err(Unstood::Undecided("whoever is looking is not in this world".to_owned()));
        };

        // Every name the journey gave, resolved. A context takes one by
        // removing it, so what is left afterwards is what no context took.
        let mut named: Vec<Named<'_>> = Vec::new();
        for name in sight.contexts {
            let Some(id) = self.bound.get(name) else {
                return Err(Unstood::Undecided(format!("`{name}` is nobody in this journey")));
            };
            let Some(instance) = self.world.instance(id) else {
                return Err(Unstood::Undecided(format!("`{name}` is not in this world")));
            };
            named.push(Named { name, id, entity: &instance.entity });
        }

        let mut standing = Standing {
            bindings: BTreeMap::new(),
            missing: BTreeMap::new(),
            unsettled: None,
            gated: None,
        };
        let mut at: Vec<At<'_>> = Vec::new();
        for context in &boundary.contexts {
            let taken = named.iter().position(|named| named.entity == context.entity);
            let (id, written) = match taken.map(|at| named.remove(at)) {
                Some(Named { name, id, .. }) => (id.clone(), name.to_owned()),
                None if actor.entity == context.entity => (looking.clone(), sight.actor.to_owned()),
                None => {
                    let remedy = format!(
                        "it is scoped to `{}`, and `{}` is `{}` — say which one with `… on {} in \
                         <the {}>`",
                        context.entity, sight.actor, actor.entity, sight.surface, context.entity
                    );
                    if context.filter.is_some() {
                        standing.gated.get_or_insert_with(|| remedy.clone());
                    }
                    standing.missing.insert(context.name.clone(), remedy);
                    continue;
                }
            };
            standing.bindings.insert(context.name.clone(), Value::Ref(id.clone()));
            at.push(At { context, id, written });
        }

        // A name no context took is a sentence about a room the surface is not
        // in, and dropping it would answer about a different question.
        if let Some(Named { name, entity, .. }) = named.first() {
            return Err(Unstood::Undecided(match boundary.contexts.as_slice() {
                [only] => format!("it is scoped to `{}`, and `{name}` is `{entity}`", only.entity),
                all => format!(
                    "`{name}` is `{entity}`, and no context of `{}` is left for one — it is at {}",
                    sight.surface,
                    all.iter()
                        .map(|context| format!("`{}`", context.entity))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }));
        }

        // Whoever the surface faces, under the name it gave them. `facing
        // owner: Identity` and `exposes: announces_reads(owner)` are one
        // sentence: the clause refers to the person looking, and this is who
        // that is.
        if let Some(binding) = crate::check::surface_named(self.spec, sight.surface)
            .and_then(|surface| surface.actor_binding.clone())
        {
            standing.bindings.entry(binding).or_insert_with(|| Value::Ref(looking.clone()));
        }

        for stood in &at {
            self.open_at(stood, sight.surface, module, &standing.bindings)?;
        }

        // In declaration order, because a later one may read an earlier one.
        // One that cannot be settled is left unbound with its reason, which is
        // what an item reading it is then told.
        for (name, value) in &boundary.lets {
            match self.evaluate(value, &standing.bindings, module) {
                Ok((value, unsettled)) => {
                    standing.bindings.insert(name.clone(), value);
                    if standing.unsettled.is_none() {
                        standing.unsettled = unsettled;
                    }
                }
                Err(why) => {
                    standing.missing.insert(name.clone(), why);
                }
            }
        }

        Ok(standing)
    }

    /// Whether a context's `where` admits the instance standing in it.
    ///
    /// `context request: ContactRequest where status = pending` says which
    /// requests have this screen at all, so an accepted one does not: nothing
    /// on it is shown, to anybody, and that is a settled answer rather than an
    /// undecided one.
    fn open_at(
        &self,
        at: &At<'_>,
        surface: &str,
        module: &str,
        bindings: &BTreeMap<String, Value>,
    ) -> Result<(), Unstood> {
        let At { context, id, written } = at;
        let Some(filter) = &context.filter else { return Ok(()) };
        match self.filter_admits(filter, id, bindings.clone(), module) {
            Ok(true) => Ok(()),
            Ok(false) => {
                let source = self.sources.get(module).map_or("", String::as_str);
                let text = span_of(filter)
                    .and_then(|span| span.slice(source))
                    .map_or_else(|| "what its context asks".to_owned(), str::to_owned);
                Err(Unstood::Closed(format!(
                    "`{surface}` is not open at `{written}`, which is not `{text}`"
                )))
            }
            Err(why) => Err(Unstood::Undecided(why)),
        }
    }
}

impl Standing {
    /// A reason about a name nothing bound, replaced by why nothing did.
    ///
    /// "nothing is bound to `link`" is the evaluator's sentence, and true; "say
    /// which one with `… in <the DeviceLink>`" is the one a reader can act on.
    /// Followed more than once, because a `let` that failed failed for a reason
    /// that may itself be a context nobody named.
    pub(crate) fn explain(&self, mut why: String) -> String {
        for _ in 0..=self.missing.len() {
            let unbound = why
                .strip_prefix("nothing is bound to `")
                .and_then(|rest| rest.strip_suffix('`'))
                .and_then(|name| self.missing.get(name));
            match unbound {
                Some(reason) => why = reason.clone(),
                None => break,
            }
        }
        why
    }
}
