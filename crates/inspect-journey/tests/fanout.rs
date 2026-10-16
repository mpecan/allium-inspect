//! A rule that iterates an entity collection, walked (issue #6).
//!
//! `for w in Hooks where active = true:` around a rule's `ensures` runs those
//! clauses once per member the filter keeps. With no hook in the world it
//! keeps none, so nothing is emitted and the rule chained from the emission
//! never runs — the same answer whether the fan-out was started by the clock
//! or by an act. It used to be neither: the iteration was recorded and never
//! applied, the clauses inside it ran once with the loop's name bound to
//! nothing, and the chained rule came back undecided on every timed step that
//! reached it.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::FANOUT;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/fanout.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(FANOUT);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// Every outcome, flattened, with the text it was written as.
fn outcomes(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = outcomes(walk)
        .into_iter()
        .filter(|(verdict, _, _)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "step {}", step.number);
    }
}

/// The reproduction from the issue: the clock moves, `Tick` raises, `Fan`
/// ranges over no hooks, and `Queue` is never woken.
#[test]
fn a_fan_out_over_no_instances_holds_while_the_clock_moves() {
    all_held(&walked("NoHooksExist"));
}

/// Its control from the issue: the same chain started by an act.
#[test]
fn the_same_fan_out_started_by_an_act_holds() {
    all_held(&walked("SameFanOutFromAnAct"));
}

/// The filter is applied, not just the iteration: the active hook is queued
/// and fired, and the idle one is left alone.
#[test]
fn only_the_members_the_filter_keeps_are_fanned_out_to() {
    all_held(&walked("OnlyTheActiveHookFires"));
}

/// A rule-level `for` that writes rather than emits writes each member the
/// filter keeps — and the clause after the loop still runs.
#[test]
fn an_iteration_that_writes_writes_each_kept_member() {
    all_held(&walked("SweepWritesEachMember"));
}

/// A hook whose `active` nobody set is a member the filter cannot decide
/// about. That is undecided — not "matched nothing", which would be the
/// simulator guessing `false` — and it names the filter it could not settle.
#[test]
fn a_member_the_filter_cannot_decide_about_leaves_the_step_undecided_and_says_why() {
    let walk = walked("AHookNobodySaidWasActive");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`Fan` fired, but"), "{detail}");
    assert!(detail.contains("active"), "names the field it could not read: {detail}");
    assert!(detail.contains("for w in Hooks"), "names the iteration: {detail}");
}

/// An iteration over a collection nobody settled — a bell whose hooks the
/// journey never named — applies nothing, and says it was the iteration that
/// went unapplied rather than only which field was missing.
#[test]
fn an_iteration_over_a_collection_nobody_settled_is_undecided_and_names_it() {
    let walk = walked("ABellNobodyHungHooksOn");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`Ring` fired, but"), "{detail}");
    assert!(detail.contains("`for h in b.hooks`"), "names the iteration: {detail}");
    assert!(detail.contains("`b.hooks` was never settled"), "{detail}");
}

/// The rule fired, and is said to have: only what it left unapplied is
/// undecided. And a rule the missing emission might have woken is not
/// reported as definitely not running.
#[test]
fn a_rule_that_fired_with_a_member_unapplied_is_said_to_have_fired_and_unsettles_what_did_not_run()
{
    let walk = walked("AHookNobodySaidWasActiveAndWhatItWouldHaveSetOff");
    let all = outcomes(&walk);
    let act = &all[0];
    assert_eq!(act.0, Verdict::Undecided, "{all:#?}");
    let detail = act.2.clone().expect("the act says why");
    assert!(
        detail.contains("`Fan` fired, but part of what it ensures was not applied"),
        "{detail}"
    );
    assert!(!detail.contains("`Fan` could not be decided"), "{detail}");
    let fires = all.iter().find(|(_, about, _)| about.contains("Fan fires")).expect("the line");
    assert_eq!(fires.0, Verdict::Specified, "{all:#?}");
    let not = all.iter().find(|(_, about, _)| about.contains("Queue does not fire")).expect("line");
    assert_eq!(not.0, Verdict::Undecided, "{all:#?}");
}

/// The issue's own path: the clock moves, and a member the filter cannot
/// decide about leaves the timed step undecided, naming the rule.
#[test]
fn a_member_the_filter_cannot_decide_about_leaves_a_timed_step_undecided() {
    let walk = walked("AHookNobodySaidWasActiveWhileTheClockMoves");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let after = step
        .outcomes
        .iter()
        .find(|outcome| outcome.about.starts_with("after"))
        .expect("the after line");
    assert_eq!(after.verdict, Verdict::Undecided);
    let detail = after.detail.clone().expect("says why");
    assert!(detail.contains("`Fan` fired"), "{detail}");
    assert!(detail.contains("Hook#1"), "names the member: {detail}");
}

/// `when: Flagged(active)` beside `for w in Hooks where active = true`: the
/// filter asks about each hook's `active`, and a hook nobody gave one is not
/// decided by the argument that happens to share its name.
#[test]
fn a_trigger_argument_named_like_a_field_the_member_has_not_set_does_not_decide_the_filter() {
    let walk = walked("AnArgumentNamedLikeAFieldTheHookHasNotSet");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`Flag` fired"), "{detail}");
    assert!(detail.contains("Hook#1"), "{detail}");
}

/// A rule that decided and applied everything it ensures holds, whatever
/// notes it carries about what it did not need: an `any` settled by one hook,
/// an `if` whose `or` its left side settled.
#[test]
fn notes_about_what_a_decided_postcondition_did_not_need_leave_the_step_holding() {
    all_held(&walked("WhatMarkDidNotNeedDoesNotUnsettleIt"));
}

/// A `requires` inside a rule's `for` is not read as the rule's own, even when
/// the trigger's argument shares the loop's name.
#[test]
fn a_precondition_inside_a_loop_is_not_read_about_an_argument_sharing_its_name() {
    let walk = walked("APreconditionInsideTheLoopIsNotTheArgument");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`Check` could not be decided"), "{detail}");
    assert!(detail.contains("inside `for w in"), "{detail}");
}

/// What the clock set off and could not decide is said the way an act's is:
/// each rule once, with the reason it gave — not a bare list of names.
#[test]
fn a_rule_the_clock_could_not_decide_is_named_once_with_its_reason() {
    let source = "journey J {
    cast:
        c: Clock
    1. time passes over a clock nobody said was done or not
        stipulate c.is_due = true
        after 1.second
}";
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(FANOUT);
    let walk = walk(&journeys[0], &journeys, &graph, &program, &sources);
    let all = outcomes(&walk);
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "after 1.second").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert!(
        detail.starts_with("while the clock moved, what it set off: `Tick` could not be decided: "),
        "{detail}"
    );
    assert!(detail.contains("`done`"), "{detail}");
    assert_eq!(detail.matches("`Tick`").count(), 1, "{detail}");
}
