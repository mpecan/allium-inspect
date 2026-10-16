//! Walking a journey through an emission into another module (issue #17).
//!
//! `migrations` hands its failure to `failures` by emitting that module's
//! trigger through the alias it imported it under:
//! `admin/FailureReported(…)`. The simulator read the qualified call as neither
//! a creation nor a trigger, so the rule in `failures` waiting on
//! `FailureReported` never ran, and the step was undecided with a reason that
//! did not say what was wrong.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::FAILURES;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(FAILURES);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

const HANDING: &str = include_str!("fixtures/failures.journey");

/// The reproduction from the issue, end to end: the step holds, `Record`
/// fires, and `creating n: failures/Notice` binds the notice it made.
#[test]
fn a_trigger_emitted_into_another_module_wakes_its_rule() {
    let walk = walked(HANDING);
    let bad: Vec<_> = walk
        .steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .map(|outcome| (outcome.about.clone(), outcome.detail.clone()))
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified);
    assert!(walk.steps[0].asides.is_empty(), "{:#?}", walk.steps[0].asides);
}

/// `migrations` walked without `failures`: its `use "./failures.allium" as
/// admin` leads to no module in the set, so `admin/FailureReported(…)` is
/// nobody's trigger. The step is undecided, saying which `use` and which
/// emission, rather than holding or waking every rule of that name.
#[test]
fn a_trigger_emitted_through_an_import_that_is_not_in_the_set_is_undecided() {
    let source = "journey Alone {
    cast:
        m: migrations/Migration
    given:
        m.failed = false
    1. fail
        m does Fail(m) on MigrationPage
        then m.failed = true
}
";
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(&["migrations"]);
    let walk = walk(&journeys[0], &journeys, &graph, &program, &sources);
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{step:#?}");
    let said: Vec<&str> =
        step.outcomes.iter().filter_map(|outcome| outcome.detail.as_deref()).collect();
    assert!(
        said.iter().any(|detail| detail.contains(
            "`migrations` imports `admin` from \"./failures.allium\", which is not in \
             this spec set, so `admin/FailureReported` is no known module's trigger"
        )),
        "{said:#?}"
    );
}
