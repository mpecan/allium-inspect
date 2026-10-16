//! A listener that discards a parameter with `_` (issue #16).
//!
//! `_` is a discard: it keeps its place among the listener's parameters and
//! names nothing, so nothing reads it and nothing has to bind it. An argument
//! handed to it by position is dropped, and so is one passed by a name no
//! other parameter has — `Published(doc: doc, by: doc)` against `when:
//! Published(doc, _)` binds `doc` and leaves nothing undecided.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::DISCARDS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(DISCARDS);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
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

/// The reproduction from the issue: `by` is passed by name, the listener
/// discards it with `_`, and the step holds — in the emitting module and
/// across the boundary.
#[test]
fn a_discard_takes_an_argument_passed_by_name() {
    let walk = walked(
        "journey DiscardANamedArgument {
    cast:
        d: Doc
    given:
        d.seen = false
        d.hushed = false
    1. publish
        d does Publish(d) on Desk
        then d.seen = true
        then d.hushed = true
}",
    );
    all_held(&walk);
}

/// By position, `_` takes its place and the next parameter the next one:
/// `Moved(doc, doc.next)` lands `b`, not `a`, so a `_` dropped rather than
/// kept in its place would land the wrong doc.
#[test]
fn a_discard_takes_an_argument_passed_by_position() {
    let walk = walked(
        "journey J {
    cast:
        a: Doc
        b: Doc
    given:
        a.next = b
        a.landed = false
        b.landed = false
    1. move
        a does Move(a) on Desk
        then b.landed = true
        then a.landed = false
}",
    );
    all_held(&walk);
}

/// `_?` is a discard as much as `_` is: `by` goes to no one and `Glance`
/// fires.
#[test]
fn an_optional_discard_takes_an_argument_passed_by_name() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
    given:
        d.glanced = false
    1. publish
        d does Publish(d) on Desk
        then d.glanced = true
}",
    );
    all_held(&walk);
}

/// The limit, on purpose: `reason` against `when: Noted(doc, _, cause?)` may
/// be meant for the `_` or be `cause` misnamed, so `cause` is not made
/// `null` and the step is undecided — over `cause`, never over `_`.
#[test]
fn a_discard_does_not_decide_an_optional_beside_an_unmatched_name() {
    let walk = walked(
        "journey J {
    cast:
        d: Doc
    given:
        d.noted = false
    1. annotate
        d does Annotate(d) on Desk
        then d.noted = true
}",
    );
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{step:#?}");
    let details: Vec<&str> = step.outcomes.iter().filter_map(|it| it.detail.as_deref()).collect();
    let said = details.join("\n");
    assert!(said.contains("binds `cause`") && said.contains("`reason`"), "{said}");
    assert!(!said.contains("binds `_`"), "{said}");
}

/// `check` has nothing to say before the walk either: no emission leaves a
/// `_` unbound, because a `_` is never bound.
#[test]
fn check_does_not_remark_on_a_discarded_argument() {
    let source = "journey J {
    cast:
        d: Doc
    1. publish and move
        d does Publish(d) on Desk
        d does Move(d) on Desk
}";
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, _) = common::library(DISCARDS);
    let notes = inspect_journey::check(&journeys[0], &journeys, &graph, &program);
    assert!(notes.is_empty(), "{notes:#?}");
}
