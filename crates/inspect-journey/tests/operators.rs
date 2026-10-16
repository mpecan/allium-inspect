//! `??` and string `+` are simulated, and a field written with a value nothing
//! settled says why when it is read back (issue #27).
//!
//! Both operators came back unknown, and the walk said only "`b.out` is
//! unknown". The evaluator had a reason — "a `??` coalesce is not simulated" —
//! and it was lost where the rule wrote the value: a field stored a bare
//! unknown, and reading it back had nothing to say. Stipulation 1 calls that
//! shape indistinguishable from a bug, and it was not particular to these two
//! operators: the `if` spelling of the same rule said exactly as little the
//! moment `summary` went unanswered.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::OPERATORS;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/operators.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(OPERATORS);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = walk
        .steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "step {}", step.number);
    }
}

/// The line about `about`, and what it said.
fn line<'a>(walk: &'a Walk, about: &str) -> &'a inspect_journey::Outcome {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .unwrap_or_else(|| panic!("no line `{about}`: {walk:#?}"))
}

/// The `then` line is undecided, and its detail says why in the spec's terms:
/// more than "is unknown", and naming `naming`.
fn undecided_naming(walk: &Walk, about: &str, naming: &str) -> String {
    let said = line(walk, about);
    assert_eq!(said.verdict, Verdict::Undecided, "{said:#?}");
    let detail = said.detail.clone().unwrap_or_default();
    assert!(detail.contains(" — "), "a reason, not only a reading: {detail}");
    assert!(detail.contains(naming), "the reason names {naming}: {detail}");
    detail
}

/// The same, quoting `quoted` from the spec as the place it is about — sliced
/// by byte offset past the em-dashes in the fixture's comments.
fn undecided_quoting(walk: &Walk, about: &str, naming: &str, quoted: &str) {
    let detail = undecided_naming(walk, about, naming);
    assert!(detail.ends_with(&format!("(in `{quoted}`)")), "quotes `{quoted}`: {detail}");
}

/// The first reproduction from the issue, as it was filed.
#[test]
fn a_coalesce_over_a_null_falls_through_to_the_right_side() {
    all_held(&walked("CoalesceFallsThrough"));
}

/// The left side, set, is the answer, and the right is never asked.
#[test]
fn a_coalesce_over_a_set_value_keeps_it() {
    all_held(&walked("CoalesceKeepsTheLeft"));
}

/// Whether the title is null is what nobody said, so the coalesce does not
/// take either side — and the walk says it was the title. `summary` is
/// answered, so reading the unknown title as null would make this hold.
#[test]
fn a_coalesce_over_an_unset_title_is_undecided_and_says_so() {
    let walk = walked("CoalesceOverAnUnsetTitle");
    undecided_quoting(&walk, "then b.out = \"s\"", "title", "box.title");
}

/// Falling through to a call nobody answered says it was that call.
#[test]
fn a_coalesce_falling_through_to_an_unanswered_call_names_it() {
    let walk = walked("CoalesceUnanswered");
    undecided_quoting(&walk, "then b.out = \"s\"", "summary", "summary(box)");
}

/// The fall-through named by a `let`: the binding keeps its reason.
#[test]
fn a_coalesce_through_a_let_names_the_call_too() {
    let walk = walked("ThroughALetUnanswered");
    undecided_quoting(&walk, "then b.out = \"s\"", "summary", "summary(box)");
}

/// Concatenated, and not to what the line says: that is a definite no, not
/// an undecided.
#[test]
fn a_concatenation_that_disagrees_does_not_hold() {
    let walk = walked("ConcatenatesWrongly");
    let said = line(&walk, "then b.label = \"ac\"");
    assert_eq!(said.verdict, Verdict::Refused, "{said:#?}");
}

/// Answered a step after the write: the field stays undecided — a stored
/// value is not recomputed — and the reason says what was so *when it was
/// written*, not that nothing answers `summary` now, a line under the
/// stipulation that does. A rule reading the field names its own clause.
#[test]
fn a_reason_given_at_the_write_is_said_as_then_and_the_reader_names_its_own_clause() {
    let walk = walked("AnsweredAfterTheWrite");
    let detail = undecided_naming(&walk, "then b.out = \"s\"", "summary");
    assert!(detail.contains("`b.out` was set to a value nothing settled"), "{detail}");
    assert!(detail.contains("when it was written: nothing defines `summary`"), "{detail}");
    let ship = undecided_naming(&walk, "b does Ship(b) on Desk", "summary");
    assert!(ship.ends_with("— in `box.out`"), "the clause read `box.out`: {ship}");
}

/// A field the spec computes from one a rule wrote undecided says what the
/// write could not settle, for `+` and for `=` alike.
#[test]
fn a_computed_field_over_an_undecided_one_says_why() {
    let walk = walked("ComputedOverAnUndecidedField");
    for about in ["then b.shown = \"s!\"", "then b.same = true"] {
        let detail = undecided_naming(&walk, about, "summary");
        assert!(detail.ends_with("— in `out`"), "{about}: {detail}");
    }
}

/// The same gap through the `if` spelling, which already evaluated: what was
/// missing there was only the reason.
#[test]
fn an_if_falling_through_to_an_unanswered_call_names_it_too() {
    let walk = walked("SpelledOutUnanswered");
    undecided_quoting(&walk, "then b.out = \"s\"", "summary", "summary(box)");
}

/// The second reproduction from the issue, as it was filed.
#[test]
fn strings_are_concatenated() {
    all_held(&walked("Concatenates"));
}

/// A tag nobody set is not an empty string, and the walk says it was the tag.
#[test]
fn concatenating_an_unset_tag_is_undecided_and_says_so() {
    let walk = walked("ConcatenatesAnUnsetTag");
    undecided_quoting(&walk, "then b.label = \"a c\"", "tag", "box.tag");
}

/// Whether somebody is the actor a surface faces turns on a field a rule wrote
/// undecided: the filter reading it bare says what the write could not settle.
#[test]
fn an_actor_filter_over_an_undecided_field_says_why() {
    let walk = walked("FacedOverAnUndecidedField");
    let detail = undecided_naming(&walk, "b does Seal(b) on Dock", "summary");
    assert!(detail.contains("`b.out` was set to a value nothing settled"), "{detail}");
}

/// A field nothing set and nothing computes says so on the `then` line, as it
/// does when a rule reads it — not only "is unknown".
#[test]
fn a_field_nothing_set_says_so() {
    let walk = walked("NothingSetIt");
    let detail = undecided_naming(&walk, "then b.done = true", "`b` has no `done` set");
    assert_eq!(detail, "b.done is unknown — `b` has no `done` set");
}
