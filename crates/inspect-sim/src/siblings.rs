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
    eval::{Told, Unresolved},
    step::{Disposition, RuleOutcome},
    truth::Truth,
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
    (against, rule): (Against<'_>, &str),
    (world, next): (&World, &mut World),
    (bindings, told): (BTreeMap<String, Value>, Told),
    ensures: &[Expr],
) -> Applied {
    let mut own = world.clone();
    own.next_ordinal.clone_from(&next.next_ordinal);
    let (removed, unsettled) =
        crate::removals::removed_by((against, rule), &own, (&bindings, &told), ensures);
    let mut applied = Applied::default();
    {
        let mut application =
            Application::new(against, &mut own, bindings).telling(told).removing(removed);
        for clause in ensures {
            applied.merge(application.apply(clause));
        }
    }
    if let Some(why) = unsettled {
        applied.unresolved.insert(0, why.clone());
        applied.unapplied.push(why);
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
            // With the reason the rule's own copy kept for an undecided
            // value, or replaying it would leave the unknown and drop why.
            Effect::Assigned { id, field, to, .. } => {
                let why = own.instance(id).map(|it| it.why(field).to_vec()).unwrap_or_default();
                next.set_field_explained(id, field, to.clone(), why);
            }
            Effect::Emitted { .. } | Effect::Refused { .. } | Effect::Noted { .. } => {}
        }
    }
    applied
}

/// Where one field stands among the siblings that wrote it so far.
#[derive(Debug)]
enum Stands {
    /// One value, from this rule — the first of several that agreed — and
    /// whether any of them read the field to compute it.
    Settled(String, Value, bool),
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
    ///
    /// Two values agree when they are equal as the spec compares them — `{a,
    /// b}` is `{b, a}` — and neither was computed from the field itself.
    /// `reads` says whether `outcome` read a field: `c.n = c.n + 1` from two
    /// siblings is the same `1` over the same stale `0`, and run one after the
    /// other they would have made `2` (see `reads`).
    pub(crate) fn settle(
        &mut self,
        trigger: &str,
        outcome: &mut RuleOutcome,
        world: &mut World,
        reads: impl Fn(&str) -> bool,
    ) {
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
            let read = reads(&key.1);
            let said = match self.by.get(&key) {
                None => {
                    self.by.insert(key, Stands::Settled(outcome.name.clone(), wrote.clone(), read));
                    continue;
                }
                Some(Stands::Settled(_, earlier, read_before))
                    if !read && !read_before && earlier.equals(wrote) == Truth::True =>
                {
                    continue;
                }
                Some(Stands::Settled(first, earlier, read_before)) => {
                    let said = if earlier.equals(wrote) == Truth::True {
                        stale(trigger, (first, &outcome.name), &key, (wrote, *read_before, read))
                    } else {
                        clash(trigger, (first, &outcome.name), &key, earlier, wrote)
                    };
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
            world.set_field_explained(&key.0, &key.1, Value::Unknown, vec![said.clone()]);
            clashes.push(said);
        }
        outcome.unresolved.extend(clashes.iter().cloned());
        outcome.unapplied.extend(clashes);
    }
}

/// Two siblings that wrote one field the same value, at least one of them
/// computed from the value the trigger found.
fn stale(
    trigger: &str,
    (first, second): (&str, &str),
    (id, field): &(EntityId, String),
    (wrote, first_read, second_read): (&Value, bool, bool),
) -> Unresolved {
    let who = match (first_read, second_read) {
        (true, true) => format!("both computed it from `{id}.{field}` as the trigger found it"),
        (true, false) => {
            format!("`{first}` computed it from `{id}.{field}` as the trigger found it")
        }
        _ => format!("`{second}` computed it from `{id}.{field}` as the trigger found it"),
    };
    undecided(format!(
        "`{first}` and `{second}` were both woken by `{trigger}` and both set `{id}.{field}` to \
         {}, but {who}; run one after the other, the second would read what the first wrote, \
         and the specification does not say which of the two runs first, so which value stands \
         is undecided",
        wrote.render()
    ))
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
        written.settle("Paint", &mut blue, &mut world, |_| false);
        assert!(blue.unapplied.is_empty(), "the first write is not a clash: {blue:#?}");
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut red = writing("PaintRed", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut red, &mut world, |_| false);
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
        written.settle("Paint", &mut first, &mut world, |_| false);
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut same = writing(
            "B",
            Disposition::Fired,
            vec![set(&lamp, "colour", "red"), set(&lamp, "shade", "dark")],
        );
        written.settle("Paint", &mut same, &mut world, |_| false);
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
        written.settle("Paint", &mut twice, &mut world, |_| false);
        assert!(twice.unapplied.is_empty(), "{twice:#?}");
        // And the value a later sibling is weighed against is its last.
        let mut later = writing("B", Disposition::Fired, vec![set(&lamp, "colour", "blue")]);
        written.settle("Paint", &mut later, &mut world, |_| false);
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
            written.settle("Paint", &mut outcome, &mut world, |_| false);
        }
        assert_eq!(world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
        // What the third rule's own postcondition wrote into the world.
        world.set_field(&lamp, "colour", Value::Str("red".to_owned()));
        let mut third = writing("C", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut third, &mut world, |_| false);
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
        written.settle("Go", &mut open, &mut world, |_| false);
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
        written.settle("Go", &mut lose, &mut world, |_| false);
        assert!(lose.unapplied.is_empty(), "{lose:#?}");
        assert_eq!(world.instance(&lamp).unwrap().field("status"), Value::Str("open".to_owned()));
    }

    /// A rule that did not fire wrote nothing, whatever its effects list holds.
    #[test]
    fn a_rule_that_did_not_fire_is_not_weighed() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut first = writing("A", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut first, &mut world, |_| false);
        let mut refused = writing("B", Disposition::Refused, vec![set(&lamp, "colour", "blue")]);
        written.settle("Paint", &mut refused, &mut world, |_| false);
        assert!(refused.unapplied.is_empty(), "{refused:#?}");
        // And it left nothing for a third to clash with.
        let mut third = writing("C", Disposition::Fired, vec![set(&lamp, "colour", "red")]);
        written.settle("Paint", &mut third, &mut world, |_| false);
        assert!(third.unapplied.is_empty(), "{third:#?}");
    }

    fn assign(id: &EntityId, field: &str, to: Value) -> Effect {
        Effect::Assigned { id: id.clone(), field: field.to_owned(), from: Value::Unknown, to }
    }

    fn hues(names: &[&str]) -> Value {
        Value::Set(names.iter().map(|it| Value::Enum((*it).to_owned())).collect())
    }

    /// Equal as the spec compares them: a set written in another order, and
    /// a whole number beside the same number with a point.
    #[test]
    fn siblings_writing_equal_values_spelled_differently_agree() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut one = writing(
            "A",
            Disposition::Fired,
            vec![
                assign(&lamp, "hues", hues(&["red", "blue"])),
                assign(&lamp, "level", Value::Int(1)),
            ],
        );
        written.settle("Tag", &mut one, &mut world, |_| false);
        world.set_field(&lamp, "hues", hues(&["blue", "red"]));
        let mut two = writing(
            "B",
            Disposition::Fired,
            vec![
                assign(&lamp, "hues", hues(&["blue", "red"])),
                assign(&lamp, "level", Value::Float(1.0)),
            ],
        );
        written.settle("Tag", &mut two, &mut world, |_| false);
        assert!(two.unapplied.is_empty(), "{two:#?}");
        assert_eq!(world.instance(&lamp).unwrap().field("hues"), hues(&["blue", "red"]));
    }

    /// The same value from a stale read is a lost update, not an agreement:
    /// in either order the second would have read the first one's write.
    #[test]
    fn the_same_value_computed_from_the_field_itself_is_contested() {
        for (first_reads, second_reads, says) in [
            (true, true, "both computed it from `Lamp#1.n`"),
            (true, false, "`A` computed it from `Lamp#1.n`"),
            (false, true, "`B` computed it from `Lamp#1.n`"),
        ] {
            let (mut world, lamp) = lamp();
            let mut written = Written::new();
            let mut one = writing("A", Disposition::Fired, vec![assign(&lamp, "n", Value::Int(1))]);
            written.settle("Bump", &mut one, &mut world, |field| first_reads && field == "n");
            world.set_field(&lamp, "n", Value::Int(1));
            let mut two = writing("B", Disposition::Fired, vec![assign(&lamp, "n", Value::Int(1))]);
            written.settle("Bump", &mut two, &mut world, |field| second_reads && field == "n");
            assert_eq!(world.instance(&lamp).unwrap().field("n"), Value::Unknown);
            assert_eq!(two.unapplied.len(), 1, "{two:#?}");
            let reason = &two.unapplied[0].reason;
            assert!(
                reason.contains("both set `Lamp#1.n` to 1")
                    && reason.contains(says)
                    && reason.contains("the second would read what the first wrote"),
                "{reason}"
            );
            assert_eq!(world.instance(&lamp).unwrap().why("n"), two.unapplied.as_slice());
        }
    }

    /// Two undecided values are not thereby the same value.
    #[test]
    fn two_unknown_writes_do_not_agree() {
        let (mut world, lamp) = lamp();
        let mut written = Written::new();
        let mut one = writing("A", Disposition::Fired, vec![assign(&lamp, "n", Value::Unknown)]);
        written.settle("Bump", &mut one, &mut world, |_| false);
        let mut two = writing("B", Disposition::Fired, vec![assign(&lamp, "n", Value::Unknown)]);
        written.settle("Bump", &mut two, &mut world, |_| false);
        assert_eq!(two.unapplied.len(), 1, "{two:#?}");
    }
}
