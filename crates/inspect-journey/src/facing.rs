//! Whether a surface faces whoever is at it (issue #13).
//!
//! `surface AdminPanel { facing admin: SiteAdmin … }` says who is on the other
//! side, and `actor SiteAdmin { identified_by: User where is_admin = true }`
//! says who that is. Read together they are a demand on the person: a user who
//! is not an admin does not have this panel, so nothing on it is theirs to do
//! or to see. The walk read neither, so `bob cannot do Flip(bob) on
//! AdminPanel` was refused with "`Flip` went through" — a surface that says it
//! faces site admins let anyone through, and nothing reported it.
//!
//! Three answers, the same three as everywhere else. The condition holds, and
//! the surface is theirs; it is definitely false, and the surface is closed to
//! them with the condition quoted; or it cannot be settled, and that is said
//! with what was missing — never read as either of the other two.
//!
//! **A type that differs is not adjudicated on its own.** A surface facing a
//! `Member` and an act by an `Identity` is either a small inconsistency or one
//! person being both, and the static check already declines to pick (see
//! `check`). So an actor that is only a type — `identified_by: User`, or a
//! surface facing an entity directly — demands nothing this walk can decide
//! about somebody of another type. An actor with a *condition* does: it is a
//! demand on a `User`'s fields, and whether somebody who is not a `User` meets
//! it is undecided, said as such.
//!
//! **Cast as the actor is being one.** `ines: Reader` in a cast is the journey
//! saying Ines is a reader, visibly, where every reader of it looks first; the
//! condition is not asked of an instance made to be one.
//!
//! **Unsettled is not the end of the question.** Whether an act goes through
//! is the facing check *and* the rules behind it, so when the first cannot be
//! settled the second is still asked: a rule that refuses whoever is at the
//! surface decides the line either way, and only an act that would otherwise
//! have gone through is left undecided.

use std::collections::BTreeMap;

use inspect_model::{NodeKind, Party, graph::EdgeKind};
use inspect_sim::{
    Value,
    eval::WITHIN,
    eval::span_of,
    value::{EntityId, Instance},
    world::World,
};

use crate::{
    check::Verdict,
    run::{Act, Outcome, Undecided, Walker},
    shapes::{CastMember, Origin},
};

/// Whether a surface faces somebody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Facing {
    /// It does, or it demands nothing this walk could hold them to.
    Faced,
    /// It definitely does not, and why: the condition, as written.
    Closed(String),
    /// Nothing could settle it, and what was missing.
    Undecided(String),
}

/// Where somebody stands, for an actor identified `within` a context: each
/// context of the surface placed so far, by entity.
pub(crate) enum At<'a> {
    /// An act, placed by the arguments its surface's `provides` passes each
    /// context in as — `provides: Archive(workspace)` puts the workspace first.
    Acting(&'a [(String, EntityId)]),
    /// A `sees`, placed by `in` and by whoever is looking.
    Looking(&'a [(String, EntityId)]),
}

impl At<'_> {
    fn placed(&self) -> &[(String, EntityId)] {
        match self {
            At::Acting(placed) | At::Looking(placed) => placed,
        }
    }
}

/// The actor a surface faces, resolved.
struct Faced<'a> {
    /// The actor's name, or the entity's when the surface faces one directly.
    name: &'a str,
    /// The module it is declared in, whose source its condition is sliced from.
    module: &'a str,
    party: Party,
}

impl Walker<'_> {
    /// Whether `surface` faces `who`, standing `at` whatever the line says.
    pub(crate) fn faces(&self, surface: &str, who: &str, at: &At<'_>) -> Facing {
        let faced = match self.faced_by(surface) {
            None => return Facing::Faced,
            Some(Err(why)) => return Facing::Undecided(why),
            Some(Ok(faced)) => faced,
        };
        let Faced { name: actor, module, party } = faced;
        let Some(condition) = &party.condition else { return Facing::Faced };

        let Some(id) = self.bound.get(who) else {
            return Facing::Undecided(format!("`{who}` is nobody in this journey"));
        };
        let Some(instance) = self.world.instance(id) else {
            return Facing::Undecided(format!("`{who}` is not in this world"));
        };
        // Cast as the actor itself — `ines: Reader` — which is the journey
        // saying she is one, in the one place a reader looks to see who is who.
        // Asking her fields whether she meets the condition would be asking an
        // instance that has none, about a claim the cast already made. Only
        // when no entity shares the actor's name: instances carry bare names,
        // and one of those would be a different somebody.
        if instance.entity == actor
            && !self.spec.nodes_of(NodeKind::Entity).any(|node| node.name == actor)
        {
            return Facing::Faced;
        }
        let source = self.sources.get(module).map_or("", String::as_str);
        let written = span_of(condition)
            .and_then(|span| span.slice(source))
            .map_or_else(|| "what it is identified by".to_owned(), str::to_owned);

        if let Some(entity) = &party.entity
            && *entity != instance.entity
        {
            return Facing::Undecided(format!(
                "`{surface}` faces `{actor}`, a `{entity}` where `{written}`, and `{who}` is a \
                 `{}` — whether they are one is not something the specification says",
                instance.entity
            ));
        }

        // `within`, from wherever the line placed it. Two contexts of its type
        // and no way to say which one the actor is identified within is not a
        // choice this walk makes by taking the first.
        let mut scope = BTreeMap::new();
        let mut unplaced_in = None;
        if let Some(context) = &party.within {
            let matching: Vec<&EntityId> = at
                .placed()
                .iter()
                .filter(|(entity, _)| entity == context)
                .map(|(_, id)| id)
                .collect();
            match matching.as_slice() {
                [standing] => {
                    scope.insert(WITHIN.to_owned(), Value::Ref((*standing).clone()));
                }
                [] => unplaced_in = Some(context.as_str()),
                _ => {
                    return Facing::Undecided(format!(
                        "`{surface}` is at more than one `{context}` here, and which one \
                         `{actor}` is identified within is not said"
                    ));
                }
            }
        }

        match self.filter_admits(condition, id, scope, module) {
            Ok(true) => Facing::Faced,
            Ok(false) => Facing::Closed(format!(
                "`{surface}` faces `{actor}`, and `{who}` is not one: not `{written}`"
            )),
            Err(why) => Facing::Undecided(match unplaced_in {
                Some(context) => unplaced(&Unplaced {
                    surface,
                    actor,
                    who,
                    context,
                    declared: self.declares_context(surface, context),
                    acting: matches!(at, At::Acting(_)),
                }),
                None => format!(
                    "whether `{who}` is the `{actor}` `{surface}` faces could not be settled — \
                     `{written}`: {}",
                    self.unset(&why, who, instance)
                ),
            }),
        }
    }

    /// The evaluator's "nothing is bound to `is_admin`", or its "`User#1` has
    /// no `is_admin` set", said about the journey when it is a field of
    /// whoever is at the surface that nothing set: what to write in `given` is
    /// then the remedy, and neither "bound" nor the world's id says so.
    fn unset(&self, why: &str, who: &str, instance: &Instance) -> String {
        self.spec
            .nodes_of(NodeKind::Entity)
            .filter(|node| node.name == instance.entity)
            .filter_map(|node| node.detail.as_entity())
            .flat_map(|detail| detail.fields.iter())
            .find(|field| {
                !instance.fields.contains_key(&field.name)
                    && (why == inspect_sim::eval::nothing_bound(&field.name)
                        || why == inspect_sim::eval::unset_field(&instance.id, &field.name))
            })
            .map_or_else(
                // A field it holds undecided is named the way the journey
                // names its owner: "`b.out` was set to …", not `Box#1.out`.
                || why.replace(&format!("`{}.", instance.id), &format!("`{who}.")),
                |field| format!("`{who}` has no `{}` set", field.name),
            )
    }

    /// The actor `surface` faces, when it faces one this walk can read.
    ///
    /// By the edge the linker resolved rather than by the name the surface
    /// wrote, because `facing admin: admins/SiteAdmin` names another module's
    /// actor and the edge is what knows which.
    ///
    /// `None` when the surface faces nobody, an entity directly (`facing c:
    /// Clock`, which demands a type and nothing more — see the module note on
    /// types), or something the spec does not declare. There is no condition
    /// there to hold anybody to. An actor that resolved and was never read
    /// for who it is, though, is ingestion missing something — not a pass.
    fn faced_by(&self, surface: &str) -> Option<Result<Faced<'_>, String>> {
        let node = self.spec.nodes_of(NodeKind::Surface).find(|node| node.name == surface)?;
        let edge = self.spec.edges_from(&node.id).find(|edge| edge.kind == EdgeKind::Facing)?;
        let party = self.spec.node(&edge.to).filter(|party| party.kind == NodeKind::Actor)?;
        Some(match self.program.actor(party.id.as_str()) {
            Some(read) => {
                Ok(Faced { name: &party.name, module: &party.module, party: read.clone() })
            }
            None => Err(format!(
                "`{surface}` faces `{}`, and who that is was not read from the specification",
                party.name
            )),
        })
    }

    /// The binding name of `surface`'s context of entity `context`, if it
    /// declares one.
    fn declares_context(&self, surface: &str, context: &str) -> Option<String> {
        let node = self.spec.nodes_of(NodeKind::Surface).find(|node| node.name == surface)?;
        let boundary = self.program.boundary(node.id.as_str())?;
        boundary.contexts.iter().find(|it| it.entity == context).map(|it| it.name.clone())
    }

    /// Where an act stands: each context of its surface that the surface's
    /// `provides` passes to this trigger, bound to the argument at that place.
    ///
    /// `provides: Archive(workspace)` and `seat does Archive(desk) on
    /// WorkspacePanel` say together that the seat is at `desk`. That is the
    /// surface's own sentence, not a guess about which argument is which.
    fn placed_by(&self, act: &Act<'_>) -> Vec<(String, EntityId)> {
        let bare = |name: &str| name.rsplit('/').next().unwrap_or(name).to_owned();
        let Some(node) =
            self.spec.nodes_of(NodeKind::Surface).find(|node| node.name == act.surface)
        else {
            return Vec::new();
        };
        let (Some(boundary), Some(detail)) =
            (self.program.boundary(node.id.as_str()), node.detail.as_surface())
        else {
            return Vec::new();
        };
        let Some(operation) =
            detail.provides.iter().find(|op| bare(&op.trigger) == bare(act.trigger))
        else {
            return Vec::new();
        };
        let mut placed = Vec::new();
        for context in &boundary.contexts {
            let Some(at) = operation.parameters.iter().position(|it| *it == context.name) else {
                continue;
            };
            if let Some(argument) = act.arguments.get(at)
                && let Value::Ref(id) = self.value_of(argument)
                && self.world.instance(&id).is_some_and(|it| it.entity == context.entity)
            {
                placed.push((context.entity.clone(), id));
            }
        }
        placed
    }
}

/// What an act changed besides the world, put back when what it did is not
/// what happened.
struct Before {
    world: World,
    bound: BTreeMap<String, EntityId>,
    cast: Vec<CastMember>,
    notes: usize,
    stipulated: usize,
}

impl Walker<'_> {
    /// An act, at a surface that may or may not face whoever does it.
    ///
    /// Faced, and it fires. Closed, and nothing fires: a surface closed to Bob
    /// is not one he can act on, so the world is the one he walked up to, and
    /// `cannot do` holds with the condition that closed it — the sentence the
    /// block is made of.
    ///
    /// Unsettled, and the rules are still asked, because an act goes through
    /// only if the surface faces him *and* its rule lets him. A rule that
    /// refuses him decides it — `cannot do` holds, `does` is refused — and
    /// only an act that would otherwise have gone through, or that is itself
    /// undecided, is undecided: the world put back, and every rule it would
    /// have woken recorded as undecided, so a `then … does not fire` after it
    /// is not answered by a rule nobody ran.
    pub(crate) fn at_the_surface(&mut self, act: &Act<'_>, line: usize, about: String) -> Outcome {
        let placed = self.placed_by(act);
        match self.faces(act.surface, act.actor, &At::Acting(&placed)) {
            Facing::Faced => self.fire_act(act, line, about),
            Facing::Closed(why) => {
                let (_, woken, before) = self.trial(act, line, &about);
                self.put_back(before);
                self.refused.extend(woken.into_iter().map(|rule| Undecided {
                    rule,
                    why: why.clone(),
                    ..Undecided::default()
                }));
                let verdict = if act.negated { Verdict::Specified } else { Verdict::Refused };
                self.unmade(act);
                Outcome { line, verdict, about, detail: Some(why) }
            }
            Facing::Undecided(why) => {
                let (mut outcome, woken, before) = self.trial(act, line, &about);
                let settled = if act.negated {
                    outcome.verdict == Verdict::Specified
                } else {
                    outcome.verdict == Verdict::Refused
                };
                if settled {
                    // Refused whichever way the surface's question goes, so
                    // nothing it did is undone by keeping it, and what refused
                    // is what a `then … fires` after it is told.
                    let said =
                        format!("whether the surface faces them was not settled either — {why}");
                    outcome.detail = Some(match outcome.detail {
                        Some(detail) => format!("{detail}; {said}"),
                        None => said,
                    });
                    return outcome;
                }
                self.put_back(before);
                self.undecided.extend(woken.into_iter().map(|rule| Undecided {
                    rule,
                    why: why.clone(),
                    ..Undecided::default()
                }));
                self.unmade(act);
                Outcome { line, verdict: Verdict::Undecided, about, detail: Some(why) }
            }
        }
    }

    /// The act fired, for what it came to and which rules it woke, with what
    /// to put back if it does not count.
    fn trial(&mut self, act: &Act<'_>, line: usize, about: &str) -> (Outcome, Vec<String>, Before) {
        let before = Before {
            world: self.world.clone(),
            bound: self.bound.clone(),
            cast: self.cast.clone(),
            notes: self.notes.len(),
            stipulated: self.stipulated.len(),
        };
        let outcome = self.fire_act(act, line, about.to_owned());
        let mut woken: Vec<String> = Vec::new();
        let rest = [&self.undecided, &self.unapplied, &self.refused];
        let named = rest.into_iter().flatten().map(|it| &it.rule);
        for rule in self.fired.iter().chain(named) {
            if !woken.contains(rule) {
                woken.push(rule.clone());
            }
        }
        (outcome, woken, before)
    }

    /// The walk as it was before a trial, and nothing remembered of it.
    fn put_back(&mut self, before: Before) {
        self.world = before.world;
        self.bound = before.bound;
        self.cast = before.cast;
        self.notes.truncate(before.notes);
        // A `where` that landed in an act now undone answered nothing.
        self.stipulated.truncate(before.stipulated);
        self.forget();
    }

    /// Listed with nothing behind it, as an act that made nothing lists it: a
    /// later line naming it should be told why it is nobody.
    fn unmade(&mut self, act: &Act<'_>) {
        for caught in act.creating {
            self.bind(&caught.cast, None, Origin::Caught);
        }
    }
}

/// An actor identified `within` a context nothing on this line placed.
struct Unplaced<'a> {
    surface: &'a str,
    actor: &'a str,
    who: &'a str,
    context: &'a str,
    /// The surface's own context of that type, by binding name, if it has one.
    declared: Option<String>,
    acting: bool,
}

/// Why `within` is unbound, as the remedy rather than the evaluator's sentence
/// — and only a remedy that works: `in` names a context the surface declares,
/// so a surface with none of that type is told so, with the one thing that
/// does settle it.
fn unplaced(unplaced: &Unplaced<'_>) -> String {
    let Unplaced { surface, actor, who, context, acting, .. } = *unplaced;
    let lead = format!("`{actor}` is identified within a `{context}`");
    match (&unplaced.declared, acting) {
        (None, _) => format!(
            "{lead}, and `{surface}` is not at any `{context}`, so nothing can say which one \
             `{who}` is at — whether `{who}` is the `{actor}` it faces is settled only by casting \
             `{who}` as a `{actor}`"
        ),
        (Some(name), true) => format!(
            "{lead}, and no argument of this act is the `{context}` `{surface}` passes as \
             `{name}`, so whether `{who}` is the `{actor}` it faces is not settled"
        ),
        (Some(_), false) => format!(
            "{lead}, and nothing said which `{context}` `{who}` is at — say which one with `… on \
             {surface} in <the {context}>`"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(declared: Option<&str>, acting: bool) -> String {
        unplaced(&Unplaced {
            surface: "WorkspacePanel",
            actor: "WorkspaceOwner",
            who: "sam",
            context: "Workspace",
            declared: declared.map(str::to_owned),
            acting,
        })
    }

    /// An act on a surface at a workspace is told which argument would have
    /// said which one.
    #[test]
    fn an_act_is_told_which_argument_would_say_which_context() {
        assert_eq!(
            said(Some("workspace"), true),
            "`WorkspaceOwner` is identified within a `Workspace`, and no argument of this act \
             is the `Workspace` `WorkspacePanel` passes as `workspace`, so whether `sam` is the \
             `WorkspaceOwner` it faces is not settled"
        );
    }

    #[test]
    fn a_look_is_told_how_to_say_which_context() {
        let said = said(Some("workspace"), false);
        assert!(said.ends_with("say which one with `… on WorkspacePanel in <the Workspace>`"));
        assert!(said.starts_with("`WorkspaceOwner` is identified within a `Workspace`"));
    }

    /// A surface at no workspace offers no `in`, so neither a look nor an act
    /// is told to write one — only the cast settles it.
    #[test]
    fn a_surface_at_no_such_context_offers_no_in() {
        for acting in [true, false] {
            let said = said(None, acting);
            assert!(!said.contains("say which one with"), "{said}");
            assert!(said.contains("`WorkspacePanel` is not at any `Workspace`"), "{said}");
            assert!(said.ends_with("casting `sam` as a `WorkspaceOwner`"), "{said}");
        }
    }
}
