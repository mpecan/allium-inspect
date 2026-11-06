//! `x?.field` short-circuits to `null` when `x` is null (issue #40).
//!
//! The language says so, and the evaluator read `?.` exactly as `.`: a read
//! from a null, undecided. `.` on a null is still that, and still says so.
//!
//! Against the recorded `allium` output and the real simulator.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::BADGES;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/badges.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(BADGES);
    walk(journey, &journeys, &graph, &program, &sources)
}

fn verdicts(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

/// The reproduction as filed: no badge, so no colour, and the step holds.
#[test]
fn optional_navigation_on_nothing_is_null() {
    let walk = walked("NoBadgeMeansNoColour");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{:#?}", verdicts(&walk));
}

/// With a badge, `?.` reads through it like `.`.
#[test]
fn optional_navigation_on_something_reads_its_field() {
    let walk = walked("ABadgeGivesItsColour");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{:#?}", verdicts(&walk));
}

/// `.` has no short-circuit: reading a field from a null is an error in the
/// spec, and the simulator still declines to answer it — with the reason.
#[test]
fn plain_navigation_on_nothing_is_still_undecided() {
    let walk = walked("PlainNavigationOnNullIsStillUndecided");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{:#?}", verdicts(&walk));
    assert!(
        verdicts(&walk)
            .iter()
            .any(|(_, _, detail)| detail.as_deref().is_some_and(|it| it.contains("from a null"))),
        "{:#?}",
        verdicts(&walk)
    );
}
