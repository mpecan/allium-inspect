//! A relationship with a singular name is one entity or `null` (issue #39).
//!
//! `allowance: Allowance with plan = this` read as a set, so `high in
//! allowance.levels` asked whether `high` was in a set of sets — `false`, and a
//! correct specification was refused. The language settles cardinality by the
//! name, and a plural name keeps the collection.
//!
//! Against the recorded `allium` output and the real simulator.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::ALLOWANCES;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/allowances.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(ALLOWANCES);
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
}

/// The reproduction as filed: the allowance is the allowance, and a derived
/// value navigating through it reads its levels.
#[test]
fn a_singular_relationship_is_the_one_entity() {
    all_held(&walked("TheAllowanceFits"));
}

/// Nothing matched: the relationship is `null`, which `= null` decides.
#[test]
fn a_singular_relationship_with_no_match_is_null() {
    all_held(&walked("NoAllowanceIsNull"));
}

/// The same declaration under a plural name is still a collection.
#[test]
fn a_plural_relationship_stays_a_collection() {
    all_held(&walked("APluralNameStaysACollection"));
}

/// Two matches for a singular name is the specification in error. Neither
/// allowance is the answer, so the step is undecided — not refused, which
/// would blame the derived value — and it says why.
#[test]
fn two_matches_for_a_singular_name_are_undecided_and_say_why() {
    let walk = walked("TwoForASingularNameIsAnError");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{walk:#?}");
    let read = &step.outcomes[0];
    assert!(
        read.detail
            .as_deref()
            .is_some_and(|it| it.contains("`allowance` is singular") && it.contains("2 match")),
        "{read:#?}"
    );
}

/// No allowance definitely matched, but one has no `plan` set and may be the
/// one. `null` would be a guess, so the read is undecided and says so.
#[test]
fn an_undecided_candidate_keeps_a_singular_relationship_undecided() {
    let walk = walked("ACandidateNobodyPlacedMayBeTheOne");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{walk:#?}");
    assert!(
        step.outcomes[0].detail.as_deref().is_some_and(|it| it.contains("may be it")),
        "{:#?}",
        step.outcomes[0]
    );
}
