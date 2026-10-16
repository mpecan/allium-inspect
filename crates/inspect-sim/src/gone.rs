//! A state rule woken about an instance this step already removed (issue #22).
//!
//! A state rule's trigger is its entity, so the one event that fires
//! `Cleanup` — `when: d: Delivery.is_expired`, `ensures: not exists d` —
//! reaches every other rule on `Delivery` too, `TimesOut` among them. Rules in
//! a step run in order over the world as the earlier ones left it, so by the
//! time `TimesOut` reads `d.status = sending` the delivery is gone.
//!
//! Whether a rule like that applies turns on whether it was due when the
//! step began, in the world the event woke it in:
//!
//! - **Not due then** — `status = done`, so never overdue — and the instance is
//!   gone now: there is no state left for the condition to be about, and the
//!   rule does not apply to it. A decided *no*, which refuses the rule — not an
//!   unknown. Read as an ordinary expression it came back "`Delivery#1` is not
//!   in this world", which made every clean-up that a second rule watched
//!   undecided.
//! - **Due then**, or not decidable then: the two rules raced over one
//!   instance, and the specification does not say which runs first. The step
//!   runs rules in a fixed order, so a refusal here would be that order's
//!   answer and not the spec's — rename a rule and it flips. Undecided, naming
//!   both rules.
//!
//! Only an instance *this step* removed, and only the instance the condition
//! is about. A reference to something the world never held is a different
//! question — a bad id, or a world missing what the event assumed — and stays
//! the evaluator's to call undecided, with that reason.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

use crate::{
    apply::Effect,
    eval::{Evaluation, Unresolved, span_of},
    step::{ClauseVerdict, Disposition, RuleOutcome},
    truth::Truth,
    value::{EntityId, Value},
    world::{Event, World},
};

/// The instances removed so far in one step, each with the rule that removed
/// it — the first one, if two said so — and the world the step began in.
#[derive(Debug)]
pub(crate) struct Removed<'a> {
    by: BTreeMap<EntityId, String>,
    /// The world the event woke every rule in, before any of them ran.
    before: &'a World,
}

impl<'a> Removed<'a> {
    /// Nothing removed yet, in a step that began in `before`.
    pub(crate) fn new(before: &'a World) -> Self {
        Self { by: BTreeMap::new(), before }
    }

    /// Remember what `outcome` removed.
    pub(crate) fn note(&mut self, outcome: &RuleOutcome) {
        for effect in &outcome.effects {
            if let Effect::Removed { id, .. } = effect {
                self.by.entry(id.clone()).or_insert_with(|| outcome.name.clone());
            }
        }
    }

    /// The one instance of `entity` among the event's arguments that this
    /// step removed, when there is exactly one.
    ///
    /// For a rule that calls the instance by another name than the one the
    /// event carries: `Expire` says `e` and `Lapse` says `l`. The step's own
    /// lookup only takes an instance the world still holds, so without this
    /// the second rule read its condition about nothing.
    pub(crate) fn carried(&self, event: &Event, entity: &str) -> Option<EntityId> {
        let mut found =
            event.arguments.values().chain(&event.positional).filter_map(|value| match value {
                Value::Ref(id) if id.entity() == entity && self.by.contains_key(id) => Some(id),
                _ => None,
            });
        let first = found.next()?;
        found.all(|other| other == first).then(|| first.clone())
    }

    /// The `when` verdict of state rule `rule` whose instance, bound as
    /// `about`, this step removed — or `None` when it is still there, or was
    /// never removed here, and the condition is read as usual.
    ///
    /// `due` reads the rule's condition about an instance in a given world;
    /// it is asked about the world the step began in.
    pub(crate) fn when_verdict(
        &self,
        (rule, about): (&str, Option<&Value>),
        world: &World,
        (text, when): (&str, Option<&Expr>),
        due: impl FnOnce(&World, &EntityId) -> Evaluation,
    ) -> Option<ClauseVerdict> {
        let Some(Value::Ref(id)) = about else { return None };
        if world.instance(id).is_some() {
            return None;
        }
        let by = self.by.get(id)?;
        let then = due(self.before, id);
        let (truth, reason, mut why) = match then.truth() {
            Truth::False => (
                Truth::False,
                format!(
                    "`{id}` was removed earlier in this step, by `{by}`, so it has no state left \
                     for this condition to be about"
                ),
                Vec::new(),
            ),
            Truth::True => (
                Truth::Unknown,
                format!(
                    "`{rule}` was due on `{id}` when this step began, and `{by}` removed it \
                     earlier in the same step; the specification does not say which of the two \
                     runs first"
                ),
                Vec::new(),
            ),
            Truth::Unknown => (
                Truth::Unknown,
                format!(
                    "whether `{rule}` was due on `{id}` when this step began could not be \
                     decided, and `{by}` removed it earlier in the same step; the specification \
                     does not say which of the two runs first"
                ),
                then.unresolved,
            ),
        };
        // About the whole clause, as a refusal inside a loop is: the reader
        // sees which `when` said what, and why, rather than a bare clause text
        // that reads as though the delivery's status had been checked.
        let note = Unresolved {
            reason,
            // The clause as the graph sliced it from the source, so a reader
            // matching the note to its clause matches the author's own words.
            expression: (!text.is_empty()).then(|| text.to_owned()),
            span: when.and_then(span_of),
        };
        why.insert(0, note);
        Some(ClauseVerdict { text: text.to_owned(), truth, unresolved: why })
    }
}

/// The outcome of a rule whose instance this step removed: `gone` as its
/// `when`, after whatever was already said, and nothing else read about an
/// absent instance. Its disposition is reckoned from every clause, as any
/// rule's is, so an unbound name beside a decided no still refuses.
pub(crate) fn outcome(
    (rule, name, module): (&str, &str, &str),
    mut requires: Vec<ClauseVerdict>,
    mut unresolved: Vec<Unresolved>,
    gone: ClauseVerdict,
) -> RuleOutcome {
    // An unknown says why on the rule as well; a decided no carries none.
    if gone.truth == Truth::Unknown {
        unresolved.extend(gone.unresolved.iter().cloned());
    }
    requires.push(gone);
    // Never true: `gone` is a no or an unknown, and either decides the rule
    // does not fire.
    let disposition = match Truth::all(requires.iter().map(|clause| clause.truth)) {
        Truth::False => Disposition::Refused,
        Truth::True | Truth::Unknown => Disposition::Undecided,
    };
    RuleOutcome {
        rule: rule.to_owned(),
        name: name.to_owned(),
        module: module.to_owned(),
        disposition,
        requires,
        effects: Vec::new(),
        unresolved,
        unapplied: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{Removed, outcome};
    use crate::{
        apply::Effect,
        eval::{Evaluation, Unresolved},
        step::{ClauseVerdict, Disposition, RuleOutcome},
        truth::Truth,
        value::{EntityId, Value},
        world::{Event, World},
    };

    const WHEN: &str = "d: Delivery.is_overdue";
    const RULE: &str = "TimesOut";

    fn removing(rule: &str, ids: &[&EntityId]) -> RuleOutcome {
        RuleOutcome {
            rule: format!("m.{rule}"),
            name: rule.to_owned(),
            module: "m".to_owned(),
            disposition: Disposition::Fired,
            requires: Vec::new(),
            effects: ids
                .iter()
                .map(|id| Effect::Removed { id: (*id).clone(), entity: "Delivery".to_owned() })
                .collect(),
            unresolved: Vec::new(),
            unapplied: Vec::new(),
        }
    }

    /// The world a step began in, holding two deliveries; the world it got to,
    /// holding only `kept`; and the ids of both.
    fn worlds() -> (World, World, EntityId, EntityId) {
        let mut before = World::new();
        let gone = before.create("Delivery", "m");
        let kept = before.create("Delivery", "m");
        let mut after = before.clone();
        after.remove(&gone);
        (before, after, gone, kept)
    }

    /// A condition that read `value` when the step began, and asserts it was
    /// asked about the world the step began in.
    fn was(value: Value) -> impl FnOnce(&World, &EntityId) -> Evaluation {
        move |world, id| {
            assert!(world.instance(id).is_some(), "asked about the world the step began in");
            Evaluation::known(value)
        }
    }

    #[test]
    fn not_due_when_the_step_began_and_removed_since_refuses_naming_who_removed_it() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        let about = Value::Ref(gone);
        let verdict = removed
            .when_verdict((RULE, Some(&about)), &after, (WHEN, None), was(Value::Bool(false)))
            .expect("decided");
        assert_eq!(verdict.truth, Truth::False);
        assert_eq!(verdict.text, WHEN);
        assert_eq!(verdict.unresolved.len(), 1);
        let note = &verdict.unresolved[0];
        assert_eq!(note.expression.as_deref(), Some(WHEN));
        assert!(note.reason.contains("`Delivery#1`") && note.reason.contains("`Cleanup`"));
    }

    /// Due beside the removal: a race the spec does not settle.
    #[test]
    fn due_when_the_step_began_is_a_race_and_undecided() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        let about = Value::Ref(gone);
        let verdict = removed
            .when_verdict((RULE, Some(&about)), &after, (WHEN, None), was(Value::Bool(true)))
            .expect("answered");
        assert_eq!(verdict.truth, Truth::Unknown);
        let reason = &verdict.unresolved[0].reason;
        assert!(
            reason.contains("`TimesOut` was due on `Delivery#1`")
                && reason.contains("`Cleanup` removed it")
                && reason.contains("does not say which of the two runs first"),
            "{reason}"
        );
    }

    /// Not decidable when the step began: still a race, and the reason the
    /// condition could not be read comes along after it.
    #[test]
    fn undecidable_when_the_step_began_is_undecided_with_both_reasons() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        let about = Value::Ref(gone);
        let why = Unresolved { reason: "no status".to_owned(), expression: None, span: None };
        let unknown = move |_: &World, _: &EntityId| Evaluation {
            value: Value::Unknown,
            unresolved: vec![why],
        };
        let verdict = removed
            .when_verdict((RULE, Some(&about)), &after, (WHEN, None), unknown)
            .expect("answered");
        assert_eq!(verdict.truth, Truth::Unknown);
        assert_eq!(verdict.unresolved.len(), 2, "{verdict:#?}");
        assert!(verdict.unresolved[0].reason.contains("could not be decided"));
        assert_eq!(verdict.unresolved[1].reason, "no status");
    }

    /// The first rule to remove it is the one named, whatever came after.
    #[test]
    fn the_first_rule_to_remove_it_is_the_one_named() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        removed.note(&removing("Sweep", &[&gone]));
        let about = Value::Ref(gone);
        let verdict = removed
            .when_verdict((RULE, Some(&about)), &after, (WHEN, None), was(Value::Bool(false)))
            .unwrap();
        let reason = &verdict.unresolved[0].reason;
        assert!(reason.contains("`Cleanup`") && !reason.contains("`Sweep`"), "{reason}");
    }

    /// An instance the world still holds is read as usual, whatever a
    /// rule's effects said about it: only absence decides anything here.
    #[test]
    fn an_instance_still_in_the_world_is_read_as_usual() {
        let (before, after, gone, kept) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone, &kept]));
        let about = Value::Ref(kept);
        let never = |_: &World, _: &EntityId| -> Evaluation { panic!("not asked") };
        assert!(removed.when_verdict((RULE, Some(&about)), &after, (WHEN, None), never).is_none());
    }

    /// Absent, but not removed here: the evaluator's to call undecided.
    #[test]
    fn an_instance_this_step_did_not_remove_is_left_to_the_evaluator() {
        let (before, after, gone, _) = worlds();
        let removed = Removed::new(&before);
        let about = Value::Ref(gone);
        let never = |_: &World, _: &EntityId| -> Evaluation { panic!("not asked") };
        assert!(removed.when_verdict((RULE, Some(&about)), &after, (WHEN, None), never).is_none());
    }

    #[test]
    fn nothing_bound_or_a_value_that_is_no_instance_is_left_to_the_evaluator() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        let never = |_: &World, _: &EntityId| -> Evaluation { panic!("not asked") };
        assert!(removed.when_verdict((RULE, None), &after, (WHEN, None), never).is_none());
        let flag = Value::Bool(true);
        assert!(removed.when_verdict((RULE, Some(&flag)), &after, (WHEN, None), never).is_none());
    }

    /// Only removals: a rule that created or assigned something adds nothing.
    #[test]
    fn effects_other_than_removal_are_not_remembered() {
        let (before, after, gone, _) = worlds();
        let mut removed = Removed::new(&before);
        let mut outcome = removing("Cleanup", &[]);
        outcome.effects.push(Effect::Created { id: gone.clone(), entity: "Delivery".to_owned() });
        removed.note(&outcome);
        let about = Value::Ref(gone);
        let never = |_: &World, _: &EntityId| -> Evaluation { panic!("not asked") };
        assert!(removed.when_verdict((RULE, Some(&about)), &after, (WHEN, None), never).is_none());
    }

    /// Carried under another name: the one removed instance of the entity the
    /// event carries, and only that.
    #[test]
    fn a_removed_instance_is_carried_whatever_the_event_called_it() {
        let (before, _, gone, kept) = worlds();
        let mut removed = Removed::new(&before);
        removed.note(&removing("Cleanup", &[&gone]));
        let event = Event::new("Delivery", "m")
            .with("d", Value::Ref(gone.clone()))
            .with("other", Value::Ref(kept.clone()));
        assert_eq!(removed.carried(&event, "Delivery"), Some(gone.clone()));
        // Another entity's name is not this one's.
        assert_eq!(removed.carried(&event, "Parcel"), None);
        // Nothing removed, nothing carried.
        assert_eq!(Removed::new(&before).carried(&event, "Delivery"), None);
        // Two distinct removed ones is not a question this answers.
        removed.note(&removing("Cleanup", &[&kept]));
        assert_eq!(removed.carried(&event, "Delivery"), None);
        // The same one twice is still one.
        let twice = Event::new("Delivery", "m")
            .with("d", Value::Ref(gone.clone()))
            .with("again", Value::Ref(gone.clone()));
        assert_eq!(removed.carried(&twice, "Delivery"), Some(gone));
    }

    fn clause(truth: Truth) -> ClauseVerdict {
        ClauseVerdict { text: WHEN.to_owned(), truth, unresolved: Vec::new() }
    }

    /// The disposition is reckoned from every clause: a decided no refuses
    /// even beside an unknown, and an unknown alone is undecided and says so
    /// on the rule.
    #[test]
    fn the_outcome_is_reckoned_from_every_clause() {
        let note = Unresolved { reason: "raced".to_owned(), expression: None, span: None };
        let mut unknown = clause(Truth::Unknown);
        unknown.unresolved.push(note);

        let refused = outcome(
            ("m.T", RULE, "m"),
            vec![clause(Truth::Unknown)],
            Vec::new(),
            clause(Truth::False),
        );
        assert_eq!(refused.disposition, Disposition::Refused);
        assert_eq!(refused.requires.len(), 2);
        assert!(refused.unresolved.is_empty());
        assert!(refused.effects.is_empty());

        let undecided = outcome(("m.T", RULE, "m"), Vec::new(), Vec::new(), unknown);
        assert_eq!(undecided.disposition, Disposition::Undecided);
        assert_eq!(undecided.unresolved.len(), 1);
        assert_eq!(undecided.unresolved[0].reason, "raced");
        assert_eq!((undecided.rule.as_str(), undecided.name.as_str()), ("m.T", RULE));
        assert_eq!(undecided.module, "m");
    }
}
