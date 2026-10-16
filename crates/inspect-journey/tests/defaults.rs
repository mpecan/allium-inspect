//! A config parameter's default is in force from the start whatever its shape
//! (issue #25).
//!
//! `apps: Set<App> = {a, b}` used to be left unset, so a rule iterating it
//! created nothing and every journey had to restate the default with `given:
//! config.apps = {a, b}` — a restatement that goes stale the day the spec's
//! default changes. Scalars were already read; collections and bare
//! enumeration values were not.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::DEFAULTS;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/defaults.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(DEFAULTS);
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

/// The reproduction from the issue, as it was filed.
#[test]
fn a_set_of_enumeration_values_is_in_force_without_restating_it() {
    all_held(&walked("SetDefault"));
}

/// Every default reads back as the spec wrote it: a set of enumeration values,
/// of strings, a list of integers, a bare enumeration value, the empty set.
#[test]
fn every_default_reads_back_as_written() {
    all_held(&walked("DefaultsAsWritten"));
}

/// Iterating a default of strings and of integers makes one of each.
#[test]
fn strings_and_integers_from_a_default_are_iterated() {
    all_held(&walked("StringsFromADefault"));
    all_held(&walked("IntegersFromADefault"));
}

/// `{}` is a default, and the rule iterating it is decided: it makes nothing.
/// Unread, it was "never settled" and the step undecided.
#[test]
fn an_empty_default_is_decided() {
    all_held(&walked("NothingFromAnEmptyDefault"));
}

/// A journey that does set the parameter still wins over the default.
#[test]
fn a_journey_may_still_override_a_default() {
    all_held(&walked("ADefaultOverridden"));
}

/// `{a, favourite}` names another parameter, which the seed does not evaluate,
/// so the default stays unset — and both the act and the read-back say which
/// parameter and why, rather than "`t.app` is unknown" with nothing behind it.
#[test]
fn a_default_built_from_another_parameter_is_undecided_and_says_why() {
    let walk = walked("ADefaultBuiltFromAnother");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
    let act = line(&walk, "bx does Choose(bx) on Panel");
    assert_eq!(act.verdict, Verdict::Undecided, "{act:#?}");
    assert!(
        act.detail.as_deref().is_some_and(|it| it.contains("nothing of kind `Thing` was created")
            && it.contains("`for x in config.chosen`")),
        "{act:#?}"
    );
    let read = line(&walk, "then config.chosen = {a, c}");
    assert_eq!(read.verdict, Verdict::Undecided, "{read:#?}");
    assert!(
        read.detail.as_deref().is_some_and(|it| it.contains("`config.chosen` has no value")
            && it.contains("no default this simulator can read")),
        "{read:#?}"
    );
}

/// `twice: Set<App> = {a, b, a}` and `requires: config.twice = {a, b, a}`:
/// the default and the rule are the same words, so the rule goes through. The
/// seed held the set once and the rule's set three times, and the rule was
/// refused by a precondition that holds — `DefaultsAsWritten` reads it back
/// both ways too, as does a default written a member to a line with notes.
#[test]
fn a_repeated_default_agrees_with_the_rule_written_the_same_way() {
    all_held(&walked("ARepeatedDefaultAsTheRuleWritesIt"));
}

/// The reason travels into a membership, not only a comparison.
#[test]
fn a_membership_in_a_default_nothing_read_says_why() {
    let walk = walked("AMembershipInADefaultNothingRead");
    let read = line(&walk, "then a in config.chosen");
    assert_eq!(read.verdict, Verdict::Undecided, "{read:#?}");
    assert!(
        read.detail.as_deref().is_some_and(|it| it.contains("`config.chosen` has no value")),
        "{read:#?}"
    );
}

/// A parameter the journey did set, to something undecided, is not one with
/// "no default" that "nothing set": both halves of that would be false.
#[test]
fn a_default_set_to_something_undecided_says_that_and_not_the_other() {
    let walk = walked("ADefaultSetToSomethingUndecided");
    let read = line(&walk, "then config.favourite = a");
    assert_eq!(read.verdict, Verdict::Undecided, "{read:#?}");
    let detail = read.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("was set to a value that is itself undecided"), "{read:#?}");
    assert!(!detail.contains("nothing set it") && !detail.contains("no default"), "{read:#?}");
}

/// A `creating` the act was refused before it could make is a refusal, not
/// an undecided step: the undecided reading is for a rule that fired and left
/// something unsettled.
#[test]
fn a_refused_act_that_created_nothing_stays_refused() {
    let walk = walked("ARefusalThatCreatesNothing");
    let act = line(&walk, "bx does Shut(bx) on Panel");
    assert_eq!(act.verdict, Verdict::Refused, "{act:#?}");
    assert!(
        act.detail.as_deref().is_some_and(|it| it.contains("nothing of kind `Thing` was created")
            && it.contains("`Shut` refused")),
        "{act:#?}"
    );
}
