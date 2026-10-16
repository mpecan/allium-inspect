//! Rules woken by one trigger, and what they write (issue #24).
//!
//! Every rule a firing wakes is read in the world as it was when the trigger
//! fired, not as the rules before it left it: its preconditions, and what its
//! postconditions read and refuse. Rules woken by one event are alternatives to
//! each other, or independent reactions to it — never a sequence:
//! `CreateIfMissing` and `ReplaceIfPresent` both listen for `Put`, and if the
//! second could see the box the first had just made, it would run on it as
//! well. Likewise `lamp.shade = lamp.colour` copies the colour the trigger
//! found, not one a sibling wrote a moment earlier, and a lifecycle refusal is
//! decided from the value the trigger found, so it is the rule's own whichever
//! sibling went first.
//!
//! Each rule's postconditions are applied to its own copy of that world, and
//! what they did is then written into the one world the step builds, in the
//! step's fixed order, because an entity id is handed out once. That order is
//! the step's and not the specification's, and the only thing it can still
//! decide is which of two values written to one field stands. So two siblings
//! that disagree about a field leave it undecided, naming both — and it stays
//! undecided whatever a third writes to it, since agreeing with one of the two
//! is not settling the question between them. Writes that agree, writes to
//! different fields and removals come out the same whichever rule went first,
//! and stand.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

use crate::{
    apply::{Against, Application, Applied, Effect},
    eval::Unresolved,
    step::{Disposition, RuleOutcome},
    value::{EntityId, Value},
    world::World,
};

/// Apply one fired rule's `ensures`, read in `world` — the world the trigger
/// found — and written into `next`.
///
/// The rule sees its own writes and creations, clause after clause, and
/// nothing a sibling did: its clauses run against a copy of `world` that
/// continues `next`'s ids, so what it creates does not take a name a sibling
/// already took. What it did is then replayed into `next` effect by effect.
pub(crate) fn apply(
    against: Against<'_>,
    (world, next): (&World, &mut World),
    bindings: BTreeMap<String, Value>,
    ensures: &[Expr],
) -> Applied {
    let mut own = world.clone();
    own.next_ordinal.clone_from(&next.next_ordinal);
    let mut applied = Applied::default();
    {
        let mut application = Application::new(against, &mut own, bindings);
        for clause in ensures {
            let step = application.apply(clause);
            applied.effects.extend(step.effects);
            applied.unresolved.extend(step.unresolved);
            applied.unapplied.extend(step.unapplied);
        }
    }
    next.next_ordinal.clone_from(&own.next_ordinal);
    for effect in &applied.effects {
        match effect {
            // As the rule left it: its fields were set by the creation, and
            // perhaps by a later clause too, which replays to the same value.
            Effect::Created { id, .. } => {
                if let Some(instance) = own.instance(id) {
                    next.insert(instance.clone());
                }
            }
            Effect::Removed { id, .. } => {
                next.remove(id);
            }
            // Onto nothing, when a sibling removed it: gone either way.
            Effect::Assigned { id, field, to, .. } => {
                next.set_field(id, field, to.clone());
            }
            Effect::Emitted { .. } | Effect::Refused { .. } | Effect::Noted { .. } => {}
        }
    }
    applied
}

/// Where one field stands among the siblings that wrote it so far.
#[derive(Debug)]
enum Stands {
    /// One value, from this rule — the first of several that agreed.
    Settled(String, Value),
    /// Two rules disagreed, and which value stands is undecided for the rest
    /// of the step.
    Contested(String, String),
}

/// What each field written so far in one step was set to, and by which rule.
#[derive(Debug, Default)]
pub(crate) struct Written {
    by: BTreeMap<(EntityId, String), Stands>,
}

impl Written {
    /// Nothing written yet.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Weigh what `outcome` wrote against what its siblings wrote before it,
    /// and remember it.
    ///
    /// A disagreement is said on `outcome`, as something it promised that the
    /// world does not show, and the field is left unknown in `world`: the value
    /// that happened to be written last is one order's answer. A rule weighs
    /// in with the last value it wrote to a field: writing it twice is its own
    /// sequence, not a clash.
    pub(crate) fn settle(&mut self, trigger: &str, outcome: &mut RuleOutcome, world: &mut World) {
        if outcome.disposition != Disposition::Fired {
            return;
        }
        let mut last = BTreeMap::new();
        for effect in &outcome.effects {
            if let Effect::Assigned { id, field, to, .. } = effect {
                last.insert((id.clone(), field.clone()), to);
            }
        }
        let mut clashes = Vec::new();
        for (key, wrote) in last {
            let said = match self.by.get(&key) {
                None => {
                    self.by.insert(key, Stands::Settled(outcome.name.clone(), wrote.clone()));
                    continue;
                }
                Some(Stands::Settled(_, earlier)) if earlier == wrote => continue,
                Some(Stands::Settled(first, earlier)) => {
                    let said = clash(trigger, (first, &outcome.name), &key, earlier, wrote);
                    self.by.insert(
                        key.clone(),
                        Stands::Contested(first.clone(), outcome.name.clone()),
                    );
                    said
                }
                Some(Stands::Contested(first, second)) => {
                    still_contested(trigger, (first, second, &outcome.name), &key, wrote)
                }
            };
            world.set_field(&key.0, &key.1, Value::Unknown);
            clashes.push(said);
        }
        outcome.unresolved.extend(clashes.iter().cloned());
        outcome.unapplied.extend(clashes);
    }
}

/// Two siblings, one field, and no answer from the spec about which stands.
fn clash(
    trigger: &str,
    (first, second): (&str, &str),
    (id, field): &(EntityId, String),
    earlier: &Value,
    wrote: &Value,
) -> Unresolved {
    undecided(format!(
        "`{first}` and `{second}` were both woken by `{trigger}`: `{first}` set `{id}.{field}` \
         to {} and `{second}` set it to {}; the specification does not say which of the two \
         runs first, so which value stands is undecided",
        earlier.render(),
        wrote.render()
    ))
}

/// A third sibling writing a field two others already disagreed about.
fn still_contested(
    trigger: &str,
    (first, second, third): (&str, &str, &str),
    (id, field): &(EntityId, String),
    wrote: &Value,
) -> Unresolved {
    undecided(format!(
        "`{third}` was woken by `{trigger}` too and set `{id}.{field}` to {}, but `{first}` and \
         `{second}` had already disagreed about it; the specification does not say which of \
         them runs last, so which value stands is undecided",
        wrote.render()
    ))
}

fn undecided(reason: String) -> Unresolved {
    Unresolved { reason, expression: None, span: None }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::Written;
    use crate::{
        apply::Effect,
        step::{Disposition, RuleOutcome},
        value::{EntityId, Value},
        world::World,
    };

    fn writing(rule: &str, disposition: Disposition, effects: Vec<Effect>) -> RuleOutcome {
        RuleOutcome {
            rule: format!("m.{rule}"),
            name: rule.to_owned(),
            module: "m".to_owned(),
            disposition,
            requires: Vec::new(),
            effects,
            unresolved: Vec::new(),
            unapplied: Vec::new(),
        }
    }

    fn set(id: &EntityId, field: &str, to: &str) -> Effect {
        Effect::Assigned {
            id: id.clone(),
            field: field.to_owned(),
            from: Value::Unknown,
            to: Value::Str(to.to_owned()),
        }
    }

    fn lamp() -> (World, EntityId) {
        let mut world = World::new();
        let lamp = world.create("Lamp", "m");
        (world, lamp)
    }

    #[test]
    fn two_siblings_that_disagree_about_a_field_leave_it_undecided_naming_both() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut blue = writing("PaintBlue", Disposition::Fired, vec![set(&lamp, "colour", "blue")]);
        written.settle("Paint", &mut blue, &mut world);
        assert!(blue.unapplied.is_empty(), "the first write is not a clash: {blue:#?}");
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut red = writing("PaintRed", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut red, &mut world);
        assert_eq!(red.unapplied.len(), 1, "{red:#?}");
        assert_eq!(red.unresolved, red.unapplied);
        let reason = &red.unapplied[0].reason;
        assert!(
            reason.contains("`PaintBlue` and `PaintRed` were both woken by `Paint`")
                && reason.contains("`Lamp#1.colour`")
                && reason.contains("\"blue\"")
                && reason.contains("\"red\"")
                && reason.contains("does not say which of the two runs first"),
            "{reason}"
        );
        assert_eq!(world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
    }

    #[test]
    fn siblings_that_agree_or_write_different_fields_both_stand() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut first = writing("A", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut first, &mut world);
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut same = writing(
            "B",
            Disposition::Fired,
            vec![set(&lamp, "colour", "red"), set(&lamp, "shade", "dark")],
        );
        written.settle("Paint", &mut same, &mut world);
        assert!(same.unapplied.is_empty() && same.unresolved.is_empty(), "{same:#?}");
        assert_eq!(world.instance(&lamp).unwrap().field("colour"), Value::Str("red".to_owned()));
    }

    /// One rule writing a field twice is its own sequence, not a clash.
    #[test]
    fn a_rule_overwriting_its_own_write_is_not_a_clash() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut twice = writing(
            "A",
            Disposition::Fired,
            vec![set(&lamp, "colour", "red"), set(&lamp, "colour", "blue")],
        );
        written.settle("Paint", &mut twice, &mut world);
        assert!(twice.unapplied.is_empty(), "{twice:#?}");
        // And the value a later sibling is weighed against is its last.
        let mut later = writing("B", Disposition::Fired, vec![set(&lamp, "colour", "blue")]);
        written.settle("Paint", &mut later, &mut world);
        assert!(later.unapplied.is_empty(), "{later:#?}");
    }

    /// A third sibling agreeing with the first does not settle what the first
    /// two disagreed about: in the order red, blue, red the field would
    /// otherwise come out red, and blue, red, red would leave it unknown.
    #[test]
    fn a_field_two_siblings_disagreed_about_stays_undecided_whatever_a_third_writes() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        for (rule, colour) in [("A", "red"), ("B", "blue")] {
            world.set_field(&lamp, "colour", Value::Str(colour.to_owned()));
            let mut outcome = writing(rule, Disposition::Fired, vec![set(&lamp, "colour", colour)]);
            written.settle("Paint", &mut outcome, &mut world);
        }
        assert_eq!(world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
        // What the third rule's own postcondition wrote into the world.
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut third = writing("C", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut third, &mut world);
        assert_eq!(world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
        assert_eq!(third.unapplied.len(), 1, "{third:#?}");
        let reason = &third.unapplied[0].reason;
        assert!(
            reason.contains("`C` was woken by `Paint` too")
                && reason.contains("`A` and `B` had already disagreed"),
            "{reason}"
        );
    }

    /// A refusal is decided from the value the trigger found, so it is the
    /// rule's own whichever sibling went first: no clash, and the sibling's
    /// write stands.
    #[test]
    fn a_refusal_is_not_weighed_against_a_siblings_write() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut open = writing("Open", Disposition::Fired, vec![set(&lamp, "status", "open")]);
        written.settle("Go", &mut open, &mut world);
        world.set_field(&lamp, "status", Value::Str("open".to_owned()));
        let mut lose = writing(
            "Lose",
            Disposition::Fired,
            vec![Effect::Refused {
                id: lamp.clone(),
                field: "status".to_owned(),
                from: "draft".to_owned(),
                to: "lost".to_owned(),
                reason: "no".to_owned(),
            }],
        );
        written.settle("Go", &mut lose, &mut world);
        assert!(lose.unapplied.is_empty(), "{lose:#?}");
        assert_eq!(world.instance(&lamp).unwrap().field("status"), Value::Str("open".to_owned()));
    }

    /// A rule that did not fire wrote nothing, whatever its effects list holds.
    #[test]
    fn a_rule_that_did_not_fire_is_not_weighed() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut first = writing("A", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut first, &mut world);
        let mut refused = writing("B", Disposition::Refused, vec![set(&lamp, "colour", "blue")]);
        written.settle("Paint", &mut refused, &mut world);
        assert!(refused.unapplied.is_empty(), "{refused:#?}");
        // And it left nothing for a third to clash with.
        let mut third = writing("C", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut third, &mut world);
        assert!(third.unapplied.is_empty(), "{third:#?}");
    }
}
