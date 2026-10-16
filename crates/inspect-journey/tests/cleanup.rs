//! An instance removed while the clock moves, and the state rules that were
//! watching it (issue #22).
//!
//! `Cleanup` removes a delivery a week after it finished, and the event that
//! woke it reaches every rule on `Delivery` — `TimesOut` among them, after the
//! delivery is gone. A delivery that no longer exists has no status to be
//! overdue on, so `TimesOut` does not apply to it: decided, not undecided. It
//! used to read its condition about an instance that was not there and take
//! the whole step with it.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::CLEANUP;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/cleanup.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(CLEANUP);
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

/// The reproduction from the issue: the clock expires a delivery, `Cleanup`
/// removes it, and `TimesOut` — woken by the same event — has nothing left to
/// be about.
#[test]
fn a_delivery_cleaned_up_has_no_timeout_to_check() {
    all_held(&walked("ADeliveryCleanedUpHasNoTimeoutToCheck"));
}

/// The line about `rule`, and what it said.
fn line<'a>(walk: &'a Walk, about: &str) -> &'a inspect_journey::Outcome {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .unwrap_or_else(|| panic!("no line `{about}`: {walk:#?}"))
}

/// Undecided, and saying which two rules raced over which instance — not a
/// decided answer that only the order the walk happens to run them in gave.
fn raced(walk: &Walk, about: &str, (due, remover): (&str, &str)) {
    let said = line(walk, about);
    assert_eq!(said.verdict, Verdict::Undecided, "{said:#?}");
    let detail = said.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains(&format!("`{due}` was due on"))
            && detail.contains(&format!("`{remover}` removed it"))
            && detail.contains("does not say which of the two runs first"),
        "{detail}"
    );
}

/// Both rules due at once, on the same delivery: the spec does not order them,
/// so whether `TimesOut` runs is undecided, and says why — whichever name sorts
/// first is not the specification's answer.
#[test]
fn an_overdue_delivery_due_beside_its_cleanup_is_a_race() {
    let walk = walked("AnOverdueDeliveryRacesItsCleanup");
    assert_eq!(line(&walk, "then Cleanup fires").verdict, Verdict::Specified);
    raced(&walk, "then TimesOut does not fire", ("TimesOut", "Cleanup"));
}

/// One round wider: `Close` removes a parcel from an event about its route,
/// and the event about the parcel — listed in the same round, before anything
/// fired — was due too. It is not fired about something no longer there, and
/// it is not dropped without a word either.
#[test]
fn a_parcel_removed_earlier_in_the_round_races_giving_up() {
    let walk = walked("AParcelOnAClosedRouteRacesGivingUp");
    assert_eq!(line(&walk, "then Close fires").verdict, Verdict::Specified);
    raced(&walk, "then GiveUp does not fire", ("GiveUp", "Close"));
}

/// The claim the other way about the same race is not decided either.
#[test]
fn a_parcel_given_up_on_beside_its_removal_is_not_decided_either() {
    let text = AS_FILED.replace("then GiveUp does not fire", "then GiveUp fires");
    let journeys = parse(&text).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == "AParcelOnAClosedRouteRacesGivingUp")
        .expect("the journey");
    let (graph, program, sources) = common::library(CLEANUP);
    let walk = walk(journey, &journeys, &graph, &program, &sources);
    raced(&walk, "then GiveUp fires", ("GiveUp", "Close"));
}

/// Under different names: the event that woke `Expire` carries `e`, and
/// `Lapse` calls the lease `l`. The lease is still the one it is about, and a
/// renewed lease was never lapsing, so `Lapse` is refused — decided.
#[test]
fn a_rule_naming_the_removed_instance_differently_is_still_refused() {
    all_held(&walked("ARenewedLeaseThatExpiresIsNotLapsed"));
}
