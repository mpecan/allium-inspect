//! A set literal of words, written in a rule, is iterated (issue #26).
//!
//! `for k in {a, b}` with `enum Kind { a | b }` used to read each word as a
//! name nothing binds, so the loop "may be missing members it could not
//! decide on" and its body was never decided. The same words already resolved
//! as a comparison value (`kind = a`) and in an emission; a member of a
//! literal set now resolves the same way — and only when the spec declares it.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::KINDS;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/kinds.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(KINDS);
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

/// The reproduction from the issue, as it was filed.
#[test]
fn a_set_literal_of_declared_words_is_iterated() {
    all_held(&walked("FillBothKinds"));
}

/// Twice: one slot per word, not one slot of an unknown kind.
#[test]
fn a_set_literal_of_two_words_makes_two() {
    all_held(&walked("OneSlotOfEachKind"));
}

/// `[b, a]` is read the same way, in the order written: the first slot made
/// is the `b` one.
#[test]
fn a_list_literal_of_words_is_iterated_in_order() {
    all_held(&walked("AListOfWordsInOrder"));
}

/// `{a, c}` with `c` declared nowhere: still undecided, and the reason names
/// `c` — not `a`, which is now read.
#[test]
fn a_word_nothing_declares_stays_undecided_and_says_which() {
    let walk = walked("AWordNothingDeclares");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
    let act = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about == "bx does Stray(bx) on Shelf")
        .unwrap_or_else(|| panic!("no act line: {walk:#?}"));
    assert_eq!(act.verdict, Verdict::Undecided, "{act:#?}");
    let detail = act.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`for k in {a, c}`"), "{act:#?}");
    assert!(detail.contains("nothing is bound to `c`"), "{act:#?}");
    assert!(!detail.contains("nothing is bound to `a`"), "{act:#?}");
}

/// A word passed to a precondition's set literal.
#[test]
fn a_word_in_a_requires_set_literal_is_a_member() {
    all_held(&walked("AWordInARequires"));
}

/// `Shadowed(box, a)` passed `b`: inside the rule `a` is the parameter, so
/// `{a, b}` is `{b}` and one slot is made. Read as the state, it would be two.
#[test]
fn a_parameter_spelled_like_a_state_is_what_was_passed() {
    all_held(&walked("AParameterSpelledLikeAState"));
}

/// `let x = box.preferred`, unset, then `{x, b}`. The set is not two things
/// it knows: if `x` is `b` it is one. The step stays undecided and says why
/// `x` is undecided — the `let` keeps its reason (issue #27) — rather than
/// holding on a count it never established.
#[test]
fn an_unknown_beside_a_word_leaves_the_loop_undecided_and_says_which() {
    let walk = walked("AnUnknownBesideAWord");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
    let act = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about == "bx does Preferred(bx) on Shelf")
        .unwrap_or_else(|| panic!("no act line: {walk:#?}"));
    assert_eq!(act.verdict, Verdict::Undecided, "{act:#?}");
    let detail = act.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`bx` has no `preferred` set"), "{act:#?}");
    assert!(detail.contains("in `{x, b}`"), "{act:#?}");
}

/// `Tray` stores `a` and never sets it, so `options: {a, b}` holds a value
/// nobody knows — the field, not the state `a` — and its count is undecided.
#[test]
fn a_field_spelled_like_a_state_is_the_field_in_a_set_literal() {
    let walk = walked("AFieldSpelledLikeAState");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{walk:#?}");
}
