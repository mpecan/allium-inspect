//! A call stipulated in `given` or in the file's `world` (issue #19).
//!
//! `stipulate f(x) = v` worked inside a step, and the same line written in
//! `given:` or a file's `world` was read as a name and reported unbound — so
//! a spec set that guards its rules with a black box for policy repeated the
//! same `stipulate` in the first step of every journey that reached one.
//!
//! Against the recorded `allium` output and the real simulator, so what holds
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::GRANTS;
use inspect_journey::{Verdict, Walk, parse, render, walk};

const JOURNEYS: &str = include_str!("fixtures/given_answers.journey");
const MOVED: &str = include_str!("fixtures/given_answers_moved.journey");

fn walked(name: &str) -> Walk {
    walked_in(JOURNEYS, name)
}

fn walked_in(source: &str, name: &str) -> Walk {
    let journeys = parse(source).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(GRANTS);
    walk(journey, &journeys, &graph, &program, &sources)
}

fn said(walk: &Walk) -> Vec<&str> {
    walk.stipulated.iter().map(|told| told.said.as_str()).collect()
}

fn text(walk: &Walk) -> String {
    render(std::slice::from_ref(walk))
}

/// The issue's reproduction: the world answers, the rule reads it, and the
/// line is listed `from the file` — not again as the journey's own.
#[test]
fn an_answer_in_the_world_is_in_force_for_every_journey_that_takes_it() {
    let walk = walked("TheWorldAnswers");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert!(walk.notes.is_empty(), "{:#?}", walk.notes);
    assert!(out.contains("1 of 1 steps hold"), "{out}");
    assert!(out.contains("    from the file  can(r, code) = true\n"), "{out}");
    assert!(!out.contains("is not a name this journey bound"), "{out}");
    assert!(said(&walk).is_empty(), "the world's lines are its own to list: {:?}", said(&walk));
}

/// The journey's own answer about the same call is the one asked, it is
/// listed as the journey's stipulation, and the world's line is marked as
/// overridden.
#[test]
fn an_answer_in_the_journey_overrides_the_worlds() {
    let walk = walked("TheJourneyOverrules");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert_eq!(said(&walk), ["can(r, code) = false"]);
    let inherited = walk
        .inherited
        .iter()
        .find(|line| line.said == "can(r, code) = true")
        .unwrap_or_else(|| panic!("the world's answer is listed: {:#?}", walk.inherited));
    assert!(inherited.overridden, "{out}");
    assert!(out.contains("    stipulated  can(r, code) = false\n"), "{out}");
}

#[test]
fn stipulate_may_lead_a_given_line_and_means_the_same() {
    let walk = walked("StipulateInGiven");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert!(walk.notes.is_empty(), "{:#?}", walk.notes);
    assert_eq!(said(&walk), ["can(r, code) = false"]);
}

/// A later answer about one call replaces the earlier: kept beside it, the
/// lookup found the world's `true` while the ledger listed the step's `false`.
#[test]
fn a_stipulation_in_a_step_replaces_the_starting_answer() {
    let walk = walked("AStepChangesItsMind");
    assert_eq!(walk.verdict(), Verdict::Specified, "{}", text(&walk));
}

/// Stipulation 1: an answer about a value nobody knows answers nothing, so
/// it is reported undecided with the argument named, and not listed.
#[test]
fn an_answer_whose_argument_has_no_value_is_refused_and_not_listed() {
    let walk = walked("AnArgumentWithNoValue");
    let out = text(&walk);
    let note = walk
        .notes
        .iter()
        .find(|note| note.about == "given can(r.level, code)")
        .unwrap_or_else(|| panic!("a note on the line: {out}"));
    assert_eq!(note.verdict, Verdict::Undecided);
    let detail = note.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`r.level` has no value"), "{detail}");
    assert!(said(&walk).is_empty(), "{:?}", said(&walk));
    // The world's own answer is untouched by the refused line.
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{out}");
    // And the journey does not read as fully specified past a line it could
    // not put in force.
    assert_eq!(walk.verdict(), Verdict::Undecided, "{out}");
}

/// A step's `stipulate` is refused the same way, on its own line, and is not
/// listed: two lines above a rule saying nobody answered, it was a receipt
/// for an answer no call could read.
#[test]
fn a_step_answer_whose_argument_has_no_value_is_refused_and_not_listed() {
    let walk = walked("AStepAnswerWithNoValue");
    let out = text(&walk);
    let line = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.starts_with("stipulate can(r.level, code)"))
        .unwrap_or_else(|| panic!("the line: {out}"));
    assert_eq!(line.verdict, Verdict::Undecided, "{out}");
    let detail = line.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("`r.level` has no value here")
            && detail.contains("answers no call the specification can ask"),
        "{detail}"
    );
    assert!(said(&walk).is_empty(), "{:?}", said(&walk));
    assert!(!out.contains("stipulated  can(r.level"), "{out}");
}

/// Answers are put in force after every field line, so an argument may read
/// a field set on a later line.
#[test]
fn an_argument_may_read_a_field_a_later_line_sets() {
    let walk = walked("AnArgumentReadsALaterLine");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert!(walk.notes.is_empty(), "{:#?}", walk.notes);
    assert_eq!(said(&walk), ["can(r, r.level) = true"]);
}

/// An answer about other arguments is another answer, and the world's line is
/// not marked overridden by it: it is still the one in force.
#[test]
fn an_answer_about_other_arguments_does_not_override_the_worlds() {
    let walk = walked("AnotherRepoIsRefused");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    let inherited = walk
        .inherited
        .iter()
        .find(|line| line.said == "can(r, code) = true")
        .unwrap_or_else(|| panic!("the world's answer is listed: {:#?}", walk.inherited));
    assert!(!inherited.overridden, "{out}");
    assert!(!out.contains("overridden can(r, code)"), "{out}");
    assert_eq!(said(&walk), ["can(s, code) = false"]);
}

/// Stipulation 1, the other half: an answer whose value nobody knows answers
/// nothing, so it is refused with the value named, not told, and not listed.
#[test]
fn an_answer_whose_value_has_no_value_is_refused_and_not_listed() {
    let walk = walked("AValueWithNoValue");
    let out = text(&walk);
    let note = walk
        .notes
        .iter()
        .find(|note| note.about == "given can(r, code)")
        .unwrap_or_else(|| panic!("a note on the line: {out}"));
    assert_eq!(note.verdict, Verdict::Undecided);
    let detail = note.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`r.level` has no value"), "{detail}");
    assert!(said(&walk).is_empty(), "{:?}", said(&walk));
    let inherited = walk
        .inherited
        .iter()
        .find(|line| line.said == "can(r, code) = true")
        .unwrap_or_else(|| panic!("the world's answer is listed: {:#?}", walk.inherited));
    assert!(!inherited.overridden, "nothing replaced it: {out}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{out}");
}

/// The later of two answers in one block is in force, and the earlier is not
/// left listed beside it as a receipt for an answer no call was given.
#[test]
fn the_later_of_two_answers_in_given_is_the_one_listed() {
    let walk = walked("TwiceInGiven");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert_eq!(said(&walk), ["can(r, code) = false"]);
}

/// The world answered `can(r, 1)`; the journey moved `level` and answered
/// `can(r, 2)`. Same words, another call: the world's answer is still in
/// force and is not marked overridden.
#[test]
fn the_same_words_about_another_value_do_not_override_the_worlds_answer() {
    let walk = walked_in(MOVED, "TheFieldMoves");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Undecided, "the world's refused line: {out}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{out}");
    let inherited = walk
        .inherited
        .iter()
        .find(|line| line.said == "can(r, r.level) = true")
        .unwrap_or_else(|| panic!("the world's answer is listed: {:#?}", walk.inherited));
    assert!(!inherited.overridden, "{out}");
    assert_eq!(said(&walk), ["can(r, r.level) = false"]);
}

/// A `stipulate` in the world is in force, and a world line refused for an
/// argument with no value is reported and not listed `from the file` — in the
/// journey that laid the world out, and in one continuing from it.
#[test]
fn a_world_line_refused_is_reported_and_not_listed() {
    for name in ["TheWorldStands", "AfterTheWorldStands"] {
        let walk = walked_in(MOVED, name);
        let out = text(&walk);
        for step in &walk.steps {
            assert_eq!(step.verdict(), Verdict::Specified, "{name}: {out}");
        }
        assert!(
            walk.inherited.iter().any(|line| line.said == "can(r, r.level) = true"),
            "{name}: {out}"
        );
        assert!(
            !walk.inherited.iter().any(|line| line.said.starts_with("can(r.tags")),
            "{name}: {out}"
        );
        assert!(!out.contains("can(r.tags, code) = true"), "{name}: {out}");
    }
    let walk = walked_in(MOVED, "TheWorldStands");
    let note = walk
        .notes
        .iter()
        .find(|note| note.about == "given can(r.tags, code)")
        .unwrap_or_else(|| panic!("a note on the world's line: {}", text(&walk)));
    assert_eq!(note.verdict, Verdict::Undecided);
}

/// Replacing the earlier answer to `can(r, code)` takes that one line out of
/// the ledger — not the first line this journey listed, which is about `s`.
#[test]
fn replacing_an_answer_leaves_the_journeys_other_answers_listed() {
    let walk = walked("TwiceInGivenBesideAnother");
    let out = text(&walk);
    assert_eq!(walk.verdict(), Verdict::Specified, "{out}");
    assert_eq!(said(&walk), ["can(s, code) = true", "can(r, code) = false"]);
}
