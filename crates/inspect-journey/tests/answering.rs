//! A black box asked about an instance the act itself makes (issue #18).
//!
//! `where <call> = <value>` under `creating` answers a call for the instance
//! the act is about to make, in force from the moment it is made. Against the
//! recorded `allium` output and the real simulator, so what holds here is a
//! journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::AUDIENCE;
use inspect_journey::{Verdict, Walk, parse, render, walk};

const JOURNEYS: &str = include_str!("fixtures/audience.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(JOURNEYS).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(AUDIENCE);
    walk(journey, &journeys, &graph, &program, &sources)
}

fn said(walk: &Walk) -> Vec<&str> {
    walk.stipulated.iter().map(|told| told.said.as_str()).collect()
}

/// The reproduction from the issue, with the answer written where the
/// reaction can read it: the step holds, and the answer is in the ledger.
#[test]
fn an_answer_under_creating_is_in_force_for_what_the_act_sets_off() {
    let walk = walked("NobodyIsInTheAudience");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Specified, "{}", render(std::slice::from_ref(&walk)));
    assert_eq!(said(&walk), ["audience(issue) = {}"]);
    let text = render(std::slice::from_ref(&walk));
    assert!(text.contains("1 of 1 steps hold"), "{text}");
    assert!(text.contains("    stipulated  audience(issue) = {}\n"), "{text}");
}

/// The answer is read, not just recorded: an audience of one notifies.
#[test]
fn the_reaction_reads_the_answer_it_was_given() {
    let walk = walked("TheRepoIsTheAudience");
    assert_eq!(
        walk.steps[0].verdict(),
        Verdict::Specified,
        "{}",
        render(std::slice::from_ref(&walk))
    );
    // In the journey's words, as `given` and `stipulate` list an answer.
    assert_eq!(said(&walk), ["audience(issue) = {repo}"]);
}

/// Said after the act, the answer still lands — a later act might ask — but
/// the reaction already asked, so the step stays undecided and the walk says
/// why, and where the answer belongs.
#[test]
fn a_stipulation_after_the_act_is_too_late_and_the_walk_says_so() {
    let walk = walked("TheAnswerComesTooLate");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided);
    assert_eq!(said(&walk), ["audience(issue) = {}"]);
    let aside = step
        .asides
        .iter()
        .find(|aside| aside.about == "stipulate audience(issue) = {}")
        .unwrap_or_else(|| panic!("a remark on the stipulation: {:#?}", step.asides));
    assert_eq!(aside.verdict, Verdict::Remark);
    let detail = aside.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("said after the act that made `issue`"), "{detail}");
    assert!(detail.contains("write `where` under `creating issue: Issue`"), "{detail}");
    // An aside, not a verdict: the line itself held.
    assert!(
        step.outcomes
            .iter()
            .filter(|outcome| outcome.about.starts_with("stipulate"))
            .all(|outcome| outcome.verdict == Verdict::Specified && outcome.detail.is_none())
    );
}

/// One act, two issues. The answer is about the one caught; the other's
/// audience is still nobody's answer, and the step says so rather than
/// reading the first answer as covering both.
#[test]
fn an_answer_is_about_the_instance_caught_and_no_other() {
    let walk = walked("OnlyTheCaughtIsAnswered");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Undecided, "{}", render(std::slice::from_ref(&walk)));
    let act = &step.outcomes[0];
    let detail = act.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`audience(issue)` was never settled"), "{detail}");
    assert_eq!(said(&walk), ["audience(issue) = {}"]);
}

/// Nothing of the kind was made, so no call could have read the answer and
/// the ledger does not claim one.
#[test]
fn an_answer_about_nothing_made_is_not_listed() {
    let walk = walked("NothingToAnswerFor");
    assert!(said(&walk).is_empty(), "{:?}", said(&walk));
    let act = &walk.steps[0].outcomes[0];
    let detail = act.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("nothing of kind `Repo` was created for `other`"), "{detail}");
}

fn remarks<'a>(walk: &'a Walk, about: &str) -> Vec<&'a str> {
    walk.steps
        .iter()
        .flat_map(|step| &step.asides)
        .filter(|aside| aside.verdict == Verdict::Remark && aside.about == about)
        .filter_map(|aside| aside.detail.as_deref())
        .collect()
}

/// Something else left the step unsettled. A late answer to a call nothing
/// asked is not why, and is not told it is; a late answer to the call that was
/// asked is.
#[test]
fn a_late_stipulation_is_blamed_only_for_the_call_that_was_asked() {
    let walk = walked("SomethingElseIsUnsettled");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(remarks(&walk, "stipulate colour(issue) = \"red\"").is_empty(), "{text}");
    let quiet = remarks(&walk, "stipulate quiet(issue) = true");
    assert_eq!(quiet.len(), 1, "{text}");
    assert!(quiet[0].contains("to answer `quiet(issue)`"), "{text}");
}

/// Nothing was left unsettled, so nothing came too late.
#[test]
fn a_stipulation_after_a_settled_act_has_nothing_to_remark() {
    let walk = walked("AnsweredAndSaidAgain");
    let step = &walk.steps[0];
    assert_eq!(step.verdict(), Verdict::Specified, "{}", render(std::slice::from_ref(&walk)));
    assert!(step.asides.is_empty(), "{:#?}", step.asides);
}

/// The repo was not made by the act, so `where` could not have said it, and
/// the walk does not suggest it.
#[test]
fn a_stipulation_about_something_the_act_did_not_make_draws_no_remark() {
    let walk = walked("AboutSomethingTheActDidNotMake");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(remarks(&walk, "stipulate audience(repo) = {}").is_empty(), "{text}");
}

/// Two issues: the remark beside the act says the one left unanswered is not
/// the one the answer is about, so the ledger above does not read as wrong.
#[test]
fn an_answer_for_one_instance_says_so_when_another_was_asked() {
    let walk = walked("OnlyTheCaughtIsAnswered");
    let text = render(std::slice::from_ref(&walk));
    let said = remarks(&walk, "repo does Split(repo, \"crash\") on Tracker");
    assert_eq!(said.len(), 1, "{text}");
    assert!(said[0].contains("`Notify` asked `audience(issue)` of something"), "{text}");
    assert!(said[0].contains("answers for `issue`, Issue#1, and no other"), "{text}");
}

/// The rule that makes the issue asks about it in the same turn, before the
/// answer is in force; the remark says so rather than that another was asked.
#[test]
fn the_rule_that_makes_the_instance_is_told_it_asked_too_soon() {
    let walk = walked("AskedAsItWasMade");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    let said = remarks(&walk, "repo does Report(repo, \"crash\") on Tracker");
    assert_eq!(said.len(), 1, "{text}");
    assert!(said[0].contains("`Report` asked `audience(issue)` as it made `issue`"), "{text}");
    assert!(said[0].contains("not the rule that makes it"), "{text}");
}

/// Refused at the door, or unsure whether it is open: the act is undone, and
/// an answer that landed inside it answered nothing, so the ledger is empty.
#[test]
fn an_answer_in_an_act_the_surface_does_not_let_through_is_not_listed() {
    for (name, verdict) in
        [("TheDoorIsClosed", Verdict::Refused), ("TheDoorMightBeClosed", Verdict::Undecided)]
    {
        let walk = walked(name);
        let text = render(std::slice::from_ref(&walk));
        assert_eq!(walk.steps[0].verdict(), verdict, "{text}");
        assert!(said(&walk).is_empty(), "{name}: {:?}", said(&walk));
        assert!(!text.contains("stipulated"), "{text}");
    }
    // And through an open door the same act is answered and listed.
    let open = walked("TheDoorIsOpen");
    assert_eq!(open.steps[0].verdict(), Verdict::Specified);
    assert_eq!(said(&open), ["audience(issue) = {}"]);
}

/// Undone at an unsettled door, what the act woke is still named: a `then
/// … fires` about it is told that rule could not be decided, not that it
/// did not run.
#[test]
fn what_an_unsettled_door_woke_is_named_when_asked_about() {
    let walk = walked("WhatTheDoorMightHaveWoken");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    let fires = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about == "then Open fires")
        .unwrap_or_else(|| panic!("the fires line: {text}"));
    assert_eq!(fires.verdict, Verdict::Undecided, "{text}");
    let detail = fires.detail.as_deref().unwrap_or_default();
    assert!(detail.starts_with("`Open` could not be decided: "), "{text}");
}

/// Caught, answered, then removed by what the act set off: the name names
/// nothing afterwards, as it would have without the `where`.
#[test]
fn a_name_bound_for_an_answer_is_let_go_when_its_instance_is_removed() {
    let walk = walked("CaughtAndDiscarded");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(said(&walk), ["spam(issue) = true"]);
    let after = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about == "stipulate issue.title = \"ham\"")
        .unwrap_or_else(|| panic!("the stipulation: {text}"));
    let detail = after.detail.as_deref().unwrap_or_default();
    assert!(detail.contains("`issue` is not a name this journey bound"), "{text}");
}

/// Answered with something a `for` cannot range over: the call was read, and
/// the simulator's own reason says what went wrong with it. Saying it was
/// asked of another instance would be a reason the walk never had.
#[test]
fn an_answer_that_was_read_draws_no_remark_about_who_asked() {
    let walk = walked("AnsweredWithANumber");
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided, "{text}");
    assert!(walk.steps[0].asides.is_empty(), "{text}");
    assert!(text.contains("ranges over an integer"), "{text}");
}

/// A `where` whose answer has no value once the instance is made is refused
/// as `given` refuses one, on the act's line, and not listed.
#[test]
fn an_answer_under_creating_with_no_value_is_refused_and_not_listed() {
    let walk = walked("AnAnswerWithNoValue");
    let step = &walk.steps[0];
    assert!(said(&walk).is_empty(), "{:?}", said(&walk));
    let aside = step
        .asides
        .iter()
        .find(|aside| aside.detail.as_deref().is_some_and(|it| it.contains("has no value")))
        .unwrap_or_else(|| panic!("said on the act's line: {:#?}", step.asides));
    assert_eq!(aside.verdict, Verdict::Undecided);
    let detail = aside.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("`repo.name` has no value once `issue` is made")
            && detail.contains("`where audience(issue)` gives the call no answer"),
        "{detail}"
    );
    assert_eq!(step.verdict(), Verdict::Undecided);
}
