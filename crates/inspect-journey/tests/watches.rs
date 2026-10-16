//! A loop over a relationship filtered on a computed field, walked (issue #23).
//!
//! `active_watches: watches where is_watching = true`, with `is_watching: a or
//! b` on the watch. The filter has to ask each watch for its own
//! `is_watching`, computed in the watch's scope. It used to ask nobody: the
//! name was read as if it were the repo's, came back bound to nothing, and the
//! loop over the projection "may be missing members" however fully the
//! journey had described every watch.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::WATCHES;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/watches.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(WATCHES);
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

/// The reproduction from the issue: `w.is_watching` is `true or false`, so
/// the loop runs once, and the entry it makes is there.
#[test]
fn a_loop_over_a_projection_filtered_on_a_computed_field_runs_for_its_members() {
    all_held(&walked("T"));
}

/// Two watches, one computed in and one computed out: the loop runs for the
/// one it keeps and not for the other.
#[test]
fn a_projection_filtered_on_a_computed_field_leaves_out_the_members_it_computes_out() {
    all_held(&walked("OnlyTheWatchingWatchGetsAnEntry"));
}

/// `false or ?` is not known. Still undecided — but the reason is the watch's,
/// by the name the journey cast it as, and the field it is missing.
#[test]
fn a_member_whose_computed_field_is_undecided_is_named_with_what_it_lacks() {
    let walk = walked("AWatchNobodySaidAllOf");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`for w in r.active_watches` may be missing members"), "{detail}");
    assert!(detail.contains("`w` has no `b` set"), "names the member and the field: {detail}");
}

/// `true or ?` is `true`: the watch is in, decided, and the `b` nobody needed
/// is not a member the loop may be missing. It read as one, and the step was
/// undecided over a field that could not change the answer.
#[test]
fn a_member_decided_by_half_its_computed_field_leaves_the_loop_decided() {
    all_held(&walked("AWatchHalfSaidIsStillWatching"));
}

/// The repo stores an `is_watching` of its own: inside the filter the bare
/// name is still the watch's, and the watch computes `false`.
#[test]
fn a_field_the_repo_stores_does_not_answer_for_the_watch_s_computed_one() {
    all_held(&walked("ARepoSaysItIsWatchingAndItsWatchSaysNot"));
}

/// A rule-level loop over the projection: its precondition is asked of the
/// watch computed in, and the watch computed out is not asked at all.
#[test]
fn a_rule_level_loop_over_a_computed_projection_runs_for_its_members() {
    all_held(&walked("HangsOnlyTheWatchingWatch"));
}

/// The rule-level loop over a member nobody can decide: undecided, and the
/// member named as the journey cast it, the same as the `ensures` loop says.
#[test]
fn a_rule_level_loop_names_an_undecided_member_by_its_cast_name() {
    let walk = walked("AHangNobodySaidAllOf");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.starts_with("`Hang` could not be decided: "), "{detail}");
    assert!(detail.contains("`for w in r.active_watches` may be missing members"), "{detail}");
    assert!(detail.contains("(`w` has no `b` set)"), "the cast name, not `Watch#1`: {detail}");
}

/// Of a member decided in and a member nobody can decide, the reason given is
/// the undecided one's: the decided one's note about the `b` it did not need
/// is not the reason anything is missing.
#[test]
fn the_member_named_is_the_one_that_could_not_be_decided() {
    let walk = walked("OfTwoWatchesTheUndecidedOneIsNamed");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("(`off` has no `b` set)"), "{detail}");
    assert!(!detail.contains("`on`"), "{detail}");
}
