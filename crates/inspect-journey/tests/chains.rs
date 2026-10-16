//! Walking a trigger one rule emits into the rules listening for it (issue #12).
//!
//! What an emission carries reaches a listener by *position* when it was passed
//! by position — under that listener's own names, whatever any other listener
//! calls them — and by *name* when it was named. A named argument the listener
//! has no parameter for binds nothing, and the listener is then undecided and
//! says which name it was waiting for and which it was handed, rather than
//! running against nothing and leaving the step to read as the spec saying no.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::CHAINS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(CHAINS);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
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

const AS_FILED: &str = include_str!("fixtures/chains.journey");

/// The reproduction from the issue, end to end: not `refused`, which is the
/// spec saying no, but undecided — and saying which binding never happened.
#[test]
fn a_listener_handed_a_name_it_does_not_have_is_undecided_and_says_which() {
    let walk = walked(AS_FILED);
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    assert!(
        outcomes(&walk).iter().all(|(verdict, _, _)| *verdict != Verdict::Refused),
        "nothing here is the spec saying no: {:#?}",
        outcomes(&walk)
    );

    let act = &step.outcomes[0];
    let detail = act.detail.as_deref().expect("the act says why");
    assert!(detail.contains("`MarkSeen` could not be decided"), "{detail}");
    // What it waited for, what it was handed, and the clause that says so.
    assert!(detail.contains("`doc`"), "{detail}");
    assert!(detail.contains("`item`"), "{detail}");
    assert!(detail.contains("Published(doc)"), "{detail}");
}

/// Across a module boundary the same mismatch is the same answer, about the
/// listener in the other module.
#[test]
fn the_same_mismatch_across_a_module_boundary_names_that_listener() {
    let walk = walked(AS_FILED);
    let detail = walk.steps[0].outcomes[0].detail.clone().expect("the act says why");
    assert!(detail.contains("`Echo` could not be decided"), "{detail}");
}

/// The control: the same trigger emitted naming `doc` reaches both listeners,
/// the one in its own module and the one across the boundary.
#[test]
fn a_named_argument_binds_the_parameter_it_names() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
    given:
        d.seen = false
        d.echoed = false
    1. republish
        d does Republish(d) on Desk
        then MarkSeen fires
        then Echo fires
        then d.seen = true
        then d.echoed = true
}",
    );
    all_held(&walk);
}

/// `Announced(doc)` by position, to three listeners calling it `thing`, `doc`
/// and — in another module — `story`. Each is handed it under its own name;
/// bound under the first listener's name only, two of the three ran against
/// nothing.
#[test]
fn a_positional_argument_reaches_every_listener_under_its_own_name() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
    given:
        d.told = false
        d.filed = false
        d.relayed = false
    1. announce
        d does Announce(d) on Desk
        then Tell fires
        then File fires
        then Relay fires
        then d.told = true
        then d.filed = true
        then d.relayed = true
}",
    );
    all_held(&walk);
}

/// `Shared(doc.owner)` and `Paired(doc, doc.owner)`: an expression by
/// position, and two by position under names no other rule uses. `Ping`
/// requires `d.owner = u`, which holds only if the two arrive in order.
#[test]
fn positional_expressions_reach_their_listeners_in_order() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
        u: User
    given:
        d.owner = u
        u.notified = false
        u.pinged = false
    1. share
        d does Share(d) on Desk
        then Notify fires
        then u.notified = true
    2. pair
        d does Pair(d) on Desk
        then Ping fires
        then u.pinged = true
}",
    );
    all_held(&walk);
}

/// `Noted(doc: doc, reason: doc)` against `when: Noted(doc, cause?)`. `cause`
/// is not quietly `null`, which fired `Mark` on `requires: cause = null` and
/// read as the spec saying `d.marked` becomes true; `Mark` is undecided and
/// names `reason`.
#[test]
fn an_optional_parameter_beside_a_misnamed_argument_is_undecided_not_null() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
    given:
        d.marked = false
    1. note
        d does Note(d) on Desk
        then d.marked = false
}",
    );
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{:#?}", outcomes(&walk));
    let detail = step.outcomes[0].detail.as_deref().expect("the act says why");
    assert!(detail.contains("`Mark` could not be decided"), "{detail}");
    assert!(detail.contains("binds `cause`: it was handed `reason` by name"), "{detail}");
}
