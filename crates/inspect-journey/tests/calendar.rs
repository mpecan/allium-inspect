//! A duration in calendar units (issue #41).
//!
//! `keep: Duration = 13.months` was "a default this simulator cannot read",
//! and every rule woken by the clock that read it came back undecided — even
//! where its deadline was `null`, which is no deadline whatever a month is.
//! Now the `null` deadline is decided, and the real question says what it
//! could not answer.
//!
//! Against the recorded `allium` output and the real simulator.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::RETENTION;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/retention.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(RETENTION);
    walk(journey, &journeys, &graph, &program, &sources)
}

fn details(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

fn all_held(walk: &Walk) {
    for step in &walk.steps {
        assert_eq!(
            step.verdict(),
            Verdict::Specified,
            "step {}: {:#?}",
            step.number,
            details(walk)
        );
    }
}

/// The reproduction as filed: a `null` deadline moved by a config default in
/// months has not passed.
#[test]
fn a_null_deadline_moved_by_a_calendar_default_is_not_due() {
    all_held(&walked("ArchivesAfterKeep"));
}

/// The same with the period written in the rule, in years.
#[test]
fn a_null_deadline_moved_by_a_calendar_literal_is_not_due() {
    all_held(&walked("ALedgerNobodyOpenedIsNeverPurged"));
}

/// With the order closed, whether thirteen months have passed depends on which
/// months, which this simulator does not know. Undecided — and the reason is
/// the calendar, not an unreadable default.
#[test]
fn a_real_calendar_deadline_is_undecided_and_says_it_is_the_calendar() {
    let walk = walked("AClosedOrderMeetsACalendarPeriod");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{:#?}", details(&walk));
    let said = details(&walk);
    assert!(
        said.iter().any(|(_, _, detail)| detail.as_deref().is_some_and(|it| {
            it.contains("`config.keep` defaults to `13.months`")
                && it.contains("calendar units")
                && !it.contains("no default this simulator can read")
        })),
        "{said:#?}"
    );
}

/// A journey that states the period in days gets an answer either side of it.
#[test]
fn a_period_restated_in_days_is_decided() {
    all_held(&walked("APeriodRestatedInDaysIsDecided"));
}
