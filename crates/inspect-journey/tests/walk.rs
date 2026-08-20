//! Walking a journey through the step engine.
//!
//! Against the recorded `allium` output for the lending fixture and the real
//! simulator — so what passes here is a journey the specification actually
//! supports, rather than one a stub agreed with.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_journey::{Verdict, Walk, parse, walk};
use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::step::Sources;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

fn library() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();

    for module in ["catalogue", "lending"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        let text = read(&path);
        sources.insert(module.to_owned(), text.clone());
        reader = reader.with(&path, text);
        paths.push(path);
    }

    let Ingestion { graph, program } =
        ingest(&runner, &reader, &paths).expect("the fixtures ingest");
    (graph, program, sources)
}

/// Walk the one journey in `source`.
fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = library();
    walk(&journeys[0], &graph, &program, &sources)
}

/// Every outcome, flattened, for asserting about a whole walk.
fn outcomes(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

const BORROWING: &str = include_str!("fixtures/lending.journey");

#[test]
fn a_copy_goes_out_and_comes_back() {
    // The whole of it: an act the surface offers, a rule that fires, a state
    // that moves, a fortnight passing without waking anything, and the copy
    // back on the shelf.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[0], &graph, &program, &sources);

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, _, _)| !matches!(verdict, Verdict::Specified | Verdict::Undecided))
        .collect();
    assert!(bad.is_empty(), "nothing should be refused or unspecified: {bad:#?}");
    assert_eq!(result.steps.len(), 3);
}

#[test]
fn borrowing_makes_the_rule_fire_and_moves_the_copy() {
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[0], &graph, &program, &sources);
    let first = &result.steps[0];

    assert_eq!(first.number, 1);
    // the act, `BorrowCopy fires`, `loan.status = open`, `copy.status = on_loan`
    for at in 0..4 {
        assert_eq!(first.outcomes[at].verdict, Verdict::Specified, "{:?}", first.outcomes[at]);
    }
}

#[test]
fn a_step_catches_what_the_rule_created_and_later_steps_use_it() {
    // The binding, which is the whole reason a journey does not have to predict
    // that the loan it is about to talk about will be called `Loan#1`.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[0], &graph, &program, &sources);

    let returned = &result.steps[2];
    assert_eq!(returned.title, "she brings it back");
    for outcome in &returned.outcomes {
        assert_eq!(outcome.verdict, Verdict::Specified, "{outcome:?}");
    }
}

#[test]
fn the_second_journey_asks_for_something_this_spec_does_not_deliver() {
    // And the tool saying so is the design working rather than failing.
    //
    // `LoanFallsOverdue` waits on `loan.window.due_at <= now`, and `BorrowCopy`
    // creates a loan with a copy, a member and a status — and no window. So the
    // due date is unset, the condition can never be decided, and no amount of
    // waiting makes the loan overdue.
    //
    // A journey is the demand written first. This one demands a loan that falls
    // due on its own, and the fixture spec has not met it: whoever owns that
    // spec has a rule to fix, and the line number to fix it against.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[1], &graph, &program, &sources);

    let overdue = &result.steps[1];
    assert_eq!(overdue.title, "the loan period runs out on its own");
    let fires = &overdue.outcomes[1];
    assert_eq!(fires.about, "then LoanFallsOverdue fires");
    assert_eq!(fires.verdict, Verdict::Undecided, "{fires:?}");
    assert!(fires.detail.as_ref().expect("a reason").contains("never became true"), "{fires:?}");
}

#[test]
fn a_rule_the_world_never_makes_true_is_undecided_rather_than_denied() {
    // Nobody fires a state-condition rule; it becomes true or it does not. The
    // simulator lists the ones that became true and says nothing about the
    // rest, so a condition that is *false* and one that could not be *decided*
    // are identical from here — and calling the second a flat no is the failure
    // this design exists to refuse.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[1], &graph, &program, &sources);
    let fires = &result.steps[1].outcomes[1];
    assert_ne!(fires.verdict, Verdict::Refused, "a rule nobody can decide is not a refusal");
}

#[test]
fn does_not_fire_errs_toward_not_knowing_too() {
    // The same limitation seen from the other side, and deliberately not
    // special-cased. "It did not run" is observable; "it was never going to"
    // is not, because the simulator lists the state-condition rules that became
    // true and says nothing about the rest. So a journey cannot get a green out
    // of a negative claim about one either, and the direction of the error —
    // toward "I do not know" — is the one this tool errs in everywhere else.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[0], &graph, &program, &sources);

    let waiting = &result.steps[1];
    assert_eq!(waiting.outcomes[1].about, "then LoanFallsOverdue does not fire");
    assert_eq!(waiting.outcomes[1].verdict, Verdict::Undecided, "{:?}", waiting.outcomes[1]);
}

#[test]
fn a_rule_somebody_fires_is_reported_as_run_or_not_run_without_hedging() {
    // The hedge above is only for rules nobody fires. An act either ran its
    // rule or it did not, and there is nothing invisible about which.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
        then BorrowCopy fires
        then ReturnCopy does not fire
}",
    );
    assert_eq!(result.steps[0].outcomes[1].verdict, Verdict::Specified, "{:?}", result.steps[0]);
    assert_eq!(result.steps[0].outcomes[2].verdict, Verdict::Specified, "{:?}", result.steps[0]);
}

#[test]
fn an_assertion_the_spec_does_not_bear_out_is_refused() {
    // The spec doing something other than what somebody said it should. From
    // the reader's side that is the same as a refusal: this journey is not what
    // this specification does.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it and it does not go out
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        then copy.status = available
}",
    );
    let last = &result.steps[0].outcomes[1];
    assert_eq!(last.verdict, Verdict::Refused, "{last:?}");
    assert!(last.detail.as_ref().expect("a reason").contains("on_loan"), "{last:?}");
    assert_eq!(result.verdict(), Verdict::Refused);
}

#[test]
fn a_precondition_the_world_does_not_meet_is_refused_in_the_specs_own_words() {
    // `BorrowCopy` requires the copy to be available. A journey that borrows a
    // lost one is not a bug in the journey — it is the specification saying no,
    // and it should say so in the words the author wrote.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = lost
    1. she tries to borrow a copy nobody can find
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    let act = &result.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Refused, "{act:?}");
    let why = act.detail.as_ref().expect("a reason");
    assert!(why.contains("BorrowCopy"), "{why}");
    assert!(why.contains("copy.status = available"), "{why}");
}

#[test]
fn a_derived_value_nobody_set_leaves_the_step_undecided() {
    // `is_at_limit` is computed by the spec and not by this simulator. Left
    // unset, the rule that reads it cannot be decided — and the journey says so
    // rather than picking a side.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it, and nobody knows whether she may
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    let act = &result.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Undecided, "{act:?}");
    assert!(act.detail.as_ref().expect("a reason").contains("is_at_limit"), "{act:?}");
}

#[test]
fn a_stipulation_gets_past_it_and_is_reported() {
    // The guardrail. An agent can make any journey pass; it cannot make one
    // pass invisibly, because what it asked to be taken on trust is at the top
    // of the result.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it, and we take her limit on trust
        stipulate ada.is_at_limit = false
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    assert_eq!(result.stipulated, ["ada.is_at_limit = false"]);
    let act = &result.steps[0].outcomes[1];
    assert_eq!(act.verdict, Verdict::Specified, "{act:?}");
}

#[test]
fn a_journey_reports_the_worst_of_what_happened() {
    // One refused step makes a journey refused, however many held.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        then loan.status = open
    2. and something that is not true
        then loan.status = returned
}",
    );
    assert_eq!(result.steps[0].verdict(), Verdict::Specified);
    assert_eq!(result.steps[1].verdict(), Verdict::Refused);
    assert_eq!(result.verdict(), Verdict::Refused);
}

#[test]
fn a_line_the_spec_cannot_support_is_not_run() {
    // Firing an act at a surface nobody specified would produce a second,
    // confusing failure about a world rather than the one that matters.
    let result = walked(
        "journey J {
    cast:
        ada: Member
    1. she acts at a screen nobody built
        ada does MemberBorrows(ada) on ReadingRoom
}",
    );
    let act = &result.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Unspecified);
    assert!(act.detail.as_ref().expect("a reason").contains("ReadingRoom"), "{act:?}");
}

#[test]
fn seeing_a_value_that_exists_is_undecided_until_the_filter_is_read() {
    // The honest half-answer. The surface carries it — the checker said so —
    // and whether *this* actor is admitted needs the `exposes` clause as an
    // expression, which is not read yet. Coming back true here would be the
    // tool claiming to have checked something it did not.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it and looks at her shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada sees loan.status on MemberShelf
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Undecided, "{seen:?}");
    assert!(seen.detail.as_ref().expect("a reason").contains("not read yet"), "{seen:?}");
}

#[test]
fn a_false_that_follows_something_undecided_is_not_a_refusal() {
    // The consequence of a world this tool could not finish computing, not the
    // specification saying no. Reporting it as a refusal would claim to have
    // checked something it did not — which is the one thing this design refuses
    // everywhere else, and the reason `undecided` exists at all.
    let journeys = parse(BORROWING).expect("parses");
    let (graph, program, sources) = library();
    let result = walk(&journeys[1], &graph, &program, &sources);

    let overdue = &result.steps[1];
    let consequence = &overdue.outcomes[2];
    assert_eq!(consequence.about, "then loan.status = overdue");
    assert_eq!(consequence.verdict, Verdict::Undecided, "{consequence:?}");
    assert!(
        consequence.detail.as_ref().expect("a reason").contains("earlier in this step"),
        "{consequence:?}"
    );
    assert_eq!(overdue.verdict(), Verdict::Undecided);
}

#[test]
fn a_false_with_nothing_undecided_before_it_stays_a_refusal() {
    // The downgrade is about consequences, not about softening every failure.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it and it does not go out
        ada does MemberBorrows(ada, copy) on MemberShelf
        then copy.status = available
}",
    );
    assert_eq!(result.steps[0].outcomes[1].verdict, Verdict::Refused);
}
