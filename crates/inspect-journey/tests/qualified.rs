//! A call answered for the module that asks it (issue #32).
//!
//! `parcels` and `tally` each ask a black box called `watchers` about the tag
//! `Pack` makes. `where tally/watchers(tag) = …` answers `tally`'s call and no
//! other; `where watchers(tag) = …` answers every module's. A qualifier naming
//! a module the specification does not have answers nothing, and is refused
//! rather than listed as if it took effect.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{GRANTS_AND_PARCELS, QUALIFIED};
use inspect_journey::{Verdict, Walk, parse, render, walk};

const JOURNEYS: &str = include_str!("fixtures/qualified.journey");
const WORLD: &str = include_str!("fixtures/qualified_world.journey");
const GRANTS: &str = include_str!("fixtures/qualified_grants.journey");

fn walked(name: &str) -> Walk {
    walked_in(JOURNEYS, QUALIFIED, name)
}

fn walked_in(file: &str, modules: &[&str], name: &str) -> Walk {
    let journeys = parse(file).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(modules);
    walk(journey, &journeys, &graph, &program, &sources)
}

fn said(walk: &Walk) -> Vec<&str> {
    walk.stipulated.iter().map(|told| told.said.as_str()).collect()
}

fn text(walk: &Walk) -> String {
    render(std::slice::from_ref(walk))
}

/// The reproduction from the issue: the qualified answer settles `tally`'s
/// `Count`, and the walk says plainly that `parcels`'s `Stamp` asked a call
/// of the same name that the answer was not about.
#[test]
fn a_qualified_answer_settles_the_call_in_the_module_it_names() {
    let walk = walked("OneModuleIsAnswered");
    let text = text(&walk);
    assert_eq!(said(&walk), ["tally/watchers(tag) = {}"]);
    assert!(!text.contains("`Count` fired, but"), "{text}");
    // `Stamp` is still unsettled — the answer was not about it — and says why.
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(text.contains("`Stamp` fired, but"), "{text}");
    assert!(
        text.contains(
            "`Stamp` asked `watchers(t)`, which `where tally/watchers(tag)` does not answer — it \
             answers `tally`'s `watchers` and no other module's"
        ),
        "{text}"
    );
}

/// Answering each module by name settles both, each with its own answer.
#[test]
fn each_module_reads_its_own_answer() {
    let walk = walked("EachModuleIsAnswered");
    let text = text(&walk);
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{text}");
    assert_eq!(said(&walk), ["tally/watchers(tag) = {}", "parcels/watchers(tag) = {ada}"]);
}

/// Unqualified, as before: every module that asks reads it.
#[test]
fn an_unqualified_answer_answers_every_module() {
    let walk = walked("EveryModuleIsAnswered");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{}", text(&walk));
}

/// Both said: the qualified one is the answer in its own module.
#[test]
fn a_qualified_answer_wins_over_an_unqualified_one_in_its_module() {
    let walk = walked("TheModuleSaysWhich");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{}", text(&walk));
}

/// A qualifier naming no module is refused, and kept out of the ledger.
#[test]
fn a_module_the_specification_does_not_have_is_refused() {
    let walk = walked("NoSuchModule");
    let text = text(&walk);
    assert_eq!(said(&walk), ["watchers(tag) = {}"], "{text}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(
        text.contains(
            "the specification has no module `ghost`, so `where ghost/watchers(tag)` answers no \
             call the specification can ask"
        ),
        "{text}"
    );
}

/// `given` and `stipulate` refuse an unknown module the same way, and land a
/// known one.
#[test]
fn given_and_stipulate_refuse_a_module_the_specification_does_not_have() {
    let walk = walked("QualifiedFromTheStart");
    let text = text(&walk);
    assert_eq!(said(&walk), ["tally/watchers(ada) = {}"], "{text}");
    // Twice, each where it was said: the `given` and step 1's `stipulate`.
    assert_eq!(
        text.matches("the specification has no module `ghost`, so this answers no call").count(),
        2,
        "{text}"
    );
    assert!(text.contains("given ghost/watchers(ada)"), "{text}");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{text}");
    let refused = step
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("ghost/watchers"))
        .unwrap_or_else(|| panic!("the stipulate about `ghost` has an outcome: {text}"));
    assert_eq!(refused.verdict, Verdict::Undecided, "{text}");
}

/// A call inside a definition is the module's that wrote it, wherever it is
/// read from: `Crate` computes `open` and `ships_to` in `parcels`, and
/// `tally`'s `Ship` reads them. Answered under `parcels`, `Ship` fires.
#[test]
fn a_call_in_a_definition_is_answered_by_the_module_that_wrote_it() {
    let walk = walked("AnsweredWhereWritten");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{}", text(&walk));
}

/// Answered under `tally`, the module that reads the field, it is not — and
/// the reason says whose call it was, quoting it from `parcels`'s text rather
/// than slicing `tally`'s at `parcels`'s offsets.
#[test]
fn a_call_in_a_definition_is_not_answered_by_the_module_reading_it() {
    let walk = walked("AnsweredWhereRead");
    let text = text(&walk);
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(
        text.contains(
            "`tally/is_open` answers `tally`'s `is_open`, and this one is `parcels`'s — in \
             `is_open(this)`"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "`Ship` asked `is_open(this)`, which `where tally/is_open(c)` does not answer — it \
             answers `tally`'s `is_open` and no other module's"
        ),
        "{text}"
    );
}

/// A late `stipulate` about `parcels`'s call is not too late for `tally`'s
/// `Count`, which asked its own; one about `tally`'s is.
#[test]
fn too_late_is_said_only_of_the_module_that_asked() {
    let too_late = "said after the act that made `tag`";
    let other = walked("LateOtherModule");
    let other_text = text(&other);
    assert_eq!(other.steps[1].verdict(), Verdict::Specified, "{other_text}");
    assert!(!other_text.contains(too_late), "{other_text}");
    let same = walked("LateSameModule");
    let same_text = text(&same);
    assert!(same_text.contains(too_late), "{same_text}");
}

/// A bare answer after a qualified one about the same call is listed, and
/// says that the qualified one still answers in its own module.
#[test]
fn a_bare_answer_says_where_a_qualified_one_still_answers() {
    let walk = walked("BareAfterQualified");
    let text = text(&walk);
    assert_eq!(said(&walk), ["parcels/watchers(ada) = {}", "watchers(ada) = {ada}"], "{text}");
    assert!(
        text.contains(
            "`parcels/watchers` still answers this call in its own module, so this answers \
             `watchers` everywhere else"
        ),
        "{text}"
    );
}

/// A file's shared world refuses an unknown module as `given` does: told in
/// every journey inheriting it, and listed in none.
#[test]
fn a_shared_world_refuses_a_module_the_specification_does_not_have() {
    let walk = walked_in(WORLD, QUALIFIED, "SharedGhost");
    let text = text(&walk);
    assert!(said(&walk).iter().all(|line| !line.contains("ghost/")), "{text}");
    assert!(!text.contains("from the file  ghost/"), "{text}");
    assert!(
        text.contains(
            "the specification has no module `ghost`, so this answers no call the specification \
             can ask"
        ),
        "{text}"
    );
}

/// A `use … as` alias is not a module, and the refusal says what to write.
#[test]
fn an_alias_is_refused_with_the_module_it_stands_for() {
    let walk = walked_in(WORLD, QUALIFIED, "AliasNotModule");
    let text = text(&walk);
    assert!(
        text.contains(
            "the specification has no module `p` — `p` is what `tally` calls `parcels`, and a \
             journey names it `parcels/watchers`"
        ),
        "{text}"
    );
}

/// A call spelled with a bare word (`can(repo, code)`) is answered by a
/// qualified answer naming its own module.
#[test]
fn a_qualified_answer_settles_a_call_spelled_with_a_word() {
    let walk = walked_in(GRANTS, GRANTS_AND_PARCELS, "QualifiedSpelling");
    assert_eq!(walk.steps[0].verdict(), Verdict::Specified, "{}", text(&walk));
}

/// Named for another module, it does not, and the reason says whose call
/// the answer was about — a `given` gets the same word a `where` does.
#[test]
fn a_given_naming_another_module_says_so() {
    let walk = walked_in(GRANTS, GRANTS_AND_PARCELS, "AnotherModulesSpelling");
    let text = text(&walk);
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(
        text.contains("`parcels/can` answers `parcels`'s `can`, and this one is `grants`'s"),
        "{text}"
    );
}
