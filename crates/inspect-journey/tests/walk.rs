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

const LOSS: &str = include_str!("fixtures/loss.journey");
const FORMS: &str = include_str!("fixtures/forms.journey");

#[test]
fn the_world_settles_after_time_passes() {
    // `ReportLostCopy` waits on `Copy.status = lost` and nobody fires it. The
    // simulator reports state-condition rules as *newly enabled* rather than
    // running them, because in the browser a person picks which to follow — but
    // a journey that says a fortnight passed has already said that whatever
    // became true in it happened.
    let walk = walked(LOSS);
    assert_eq!(walk.verdict(), Verdict::Specified, "{:#?}", outcomes(&walk));
    let noticed = outcomes(&walk)
        .into_iter()
        .find(|(_, about, _)| about.contains("ReportLostCopy"))
        .expect("the journey asserts the rule fires");
    assert_eq!(noticed.0, Verdict::Specified, "{noticed:?}");
}

#[test]
fn a_rule_already_enabled_before_the_clock_moved_still_runs() {
    // The trap this walked into once: the simulator's `newly_enabled` subtracts
    // the rules that already held, which is right for a browser showing "what
    // your action just made possible" and wrong for a journey asking what the
    // world now makes true. Driven from that list, a rule enabled by step one
    // and still waiting in step two would never run at all — and the whole
    // settle would be silently dead.
    //
    // Step 1 leaves the copy lost, so `ReportLostCopy` is enabled before step
    // 2's clock ever moves. If it runs, this list is not empty.
    let walk = walked(LOSS);
    let second = &walk.steps[1];
    assert!(
        second.outcomes.iter().all(|outcome| outcome.verdict == Verdict::Specified),
        "{:#?}",
        second.outcomes
    );
}

#[test]
fn settling_stops_once_nothing_new_is_true() {
    // A rule whose effect keeps its own condition true — a copy that is lost
    // stays lost — re-enables itself every round. Without remembering what has
    // already run for which instance it would run to the bound and report a
    // world that never settled, which is a failure invented by this walker
    // rather than found in the spec.
    let walk = walked(LOSS);
    let complaints: Vec<_> = outcomes(&walk)
        .into_iter()
        .filter(|(_, _, detail)| {
            detail.as_deref().is_some_and(|text| text.contains("never settled"))
        })
        .collect();
    assert!(complaints.is_empty(), "{complaints:#?}");
}

/// Every outcome of the one-step `FORMS` journey, by the line it is about.
fn forms() -> Vec<(Verdict, String, Option<String>)> {
    outcomes(&walked(FORMS))
}

fn form(about: &str) -> (Verdict, String, Option<String>) {
    forms()
        .into_iter()
        .find(|(_, written, _)| written.contains(about))
        .unwrap_or_else(|| panic!("`{about}` is one of the lines:\n{:#?}", forms()))
}

#[test]
fn an_instance_the_journey_caught_exists_and_one_it_never_named_does_not() {
    assert_eq!(form("loan exists").0, Verdict::Specified);
    assert_eq!(form("reservation does not exist").0, Verdict::Specified);
}

#[test]
fn a_not_equal_assertion_is_not_an_equal_one() {
    // The copy is on loan by now, so `!= available` holds and `= available`
    // would not. An operator that fell through to equality would report this
    // journey passing for the opposite reason.
    let (verdict, _, detail) = form("copy.status != available");
    assert_eq!(verdict, Verdict::Specified);
    assert_eq!(detail, None, "a line that holds needs no explaining");
}

#[test]
fn a_line_that_holds_says_nothing_more_and_one_that_does_not_says_what_it_found() {
    // The report is read by somebody looking for the four lines out of forty
    // that went wrong. Every holding line carrying "and here is what I found"
    // buries them.
    // Assertions only. A sight reports what the actor saw either way, because
    // "she can see it, and it is empty" is a different fact from "she can see
    // it" and a reader chasing a gap in the spec wants both.
    for (verdict, about, detail) in
        forms().into_iter().filter(|(_, about, _)| about.starts_with("then "))
    {
        match verdict {
            Verdict::Specified => assert_eq!(detail, None, "{about}"),
            _ => assert!(detail.is_some(), "{about} says why not"),
        }
    }
}

#[test]
fn membership_is_read_against_the_instances_the_world_holds() {
    // `Loans` is every loan, which is how a journey says "and it is one of
    // them" without casting each instance by hand.
    assert_eq!(form("loan in Loans").0, Verdict::Specified);

    // And an empty collection is a definite no rather than an unknown: the
    // spec declares `Reservation`, this world holds none, so the loan is
    // certainly not among them.
    let (verdict, _, detail) = form("loan in Reservations");
    assert_eq!(verdict, Verdict::Refused);
    assert_eq!(detail.as_deref(), Some("Reservations is {}"));
}

#[test]
fn a_rule_that_ran_and_one_that_did_not_are_both_reported() {
    assert_eq!(form("BorrowCopy fires").0, Verdict::Specified);
    assert_eq!(form("ReturnCopy does not fire").0, Verdict::Specified);
}

#[test]
fn a_negated_sight_reports_the_observation_rather_than_the_static_note() {
    // `cannot see` is checked twice: once against what the surface exposes,
    // which agrees, and once against the world. The line a reader gets should
    // be the second one — the first only says the check was allowed to run.
    let (verdict, _, detail) = form("cannot see copy.shelfmark");
    assert_eq!(verdict, Verdict::Specified);
    assert_eq!(detail.as_deref(), Some("copy.shelfmark has no value here"));
}

const REACHING_PAST: &str = r#"
journey SheReachesPastTheDesk {
    goal: One line the spec cannot support, beside one it can.

    cast:
        ada:  Member
        copy: catalogue/Copy

    given:
        copy.status = available

    1. she looks, then reaches past the desk
        then copy.status = available
        ada does MemberBorrows(ada, copy) on CatalogueDesk

    ends: One of these two lines is the library's problem.
}
"#;

#[test]
fn a_line_the_spec_cannot_support_blocks_only_itself() {
    // The note that stops a line running is matched by the line it is about.
    // A filter that took any note anywhere would report the whole step against
    // one unspecified act, and the reader would go looking for a second fault
    // that is not there.
    let walk = walked(REACHING_PAST);
    let outcomes = outcomes(&walk);
    assert_eq!(outcomes[0].0, Verdict::Specified, "{outcomes:#?}");
    assert_eq!(outcomes[0].1, "then copy.status = available");
    assert_eq!(outcomes[0].2, None);

    assert_eq!(outcomes[1].0, Verdict::Unspecified, "{outcomes:#?}");
    assert!(outcomes[1].2.is_some(), "and it says why: {outcomes:#?}");
}

#[test]
fn a_walk_reports_its_worst_step() {
    // What the summary line says and what `--strict` exits on. A journey with
    // one unsupported line among many is not a journey that passes.
    assert_eq!(walked(FORMS).verdict(), Verdict::Refused);
    assert_eq!(walked(LOSS).verdict(), Verdict::Specified);
    assert_eq!(walked(REACHING_PAST).verdict(), Verdict::Unspecified);
}

const RESERVATIONS: &str = include_str!("fixtures/reservations.journey");
const UNDECIDED: &str = include_str!("fixtures/undecided.journey");

#[test]
fn a_rule_that_holds_for_two_instances_runs_for_both() {
    // Two readers, one book, one withdrawal. The rule that calls a reservation
    // off holds for each of them separately, and a fixpoint that remembered
    // only which *rules* had run would cancel the first reader's and leave the
    // second waiting on a book that no longer exists — silently, because
    // nothing else in the walk would notice.
    let walk = walked(RESERVATIONS);
    assert_eq!(walk.verdict(), Verdict::Specified, "{:#?}", outcomes(&walk));
    for who in ["hers.status = cancelled", "his.status = cancelled"] {
        let (verdict, _, _) = outcomes(&walk)
            .into_iter()
            .find(|(_, about, _)| about.contains(who))
            .unwrap_or_else(|| panic!("`{who}` is one of the lines"));
        assert_eq!(verdict, Verdict::Specified, "{who}");
    }
}

#[test]
fn a_rule_whose_effect_keeps_its_own_condition_true_runs_once_per_instance() {
    // The other half. `CancelReservationOnWithdrawal` watches the *book*, and
    // cancelling a reservation does not un-withdraw it — so every round it is
    // enabled again for the same two reservations. Remembering only the rule
    // would drop one reader; remembering nothing would run to the bound and
    // report a world that never settled, which is a failure this walker
    // invented rather than found.
    let walk = walked(RESERVATIONS);
    let complaints: Vec<_> = outcomes(&walk)
        .into_iter()
        .filter(|(_, _, detail)| {
            detail.as_deref().is_some_and(|text| text.contains("never settled"))
        })
        .collect();
    assert!(complaints.is_empty(), "{complaints:#?}");
}

#[test]
fn a_rule_nobody_could_decide_is_not_a_rule_that_did_not_run() {
    // The two answers a reader must be able to tell apart. One is the spec
    // saying no; the other is this tool saying it does not know, and a journey
    // reporting the second as the first would send somebody to change a spec
    // that was never consulted.
    let walk = walked(UNDECIDED);
    let lines = outcomes(&walk);

    let (verdict, _, detail) = lines
        .iter()
        .find(|(_, about, _)| about.contains("BorrowCopy fires"))
        .cloned()
        .expect("the journey asserts it fires");
    assert_eq!(verdict, Verdict::Undecided);
    assert_eq!(detail.as_deref(), Some("`BorrowCopy` could not be decided"));

    let (_, _, detail) = lines
        .iter()
        .find(|(_, about, _)| about.contains("ReturnCopy fires"))
        .cloned()
        .expect("the journey asserts that one too");
    // Nobody fired it and nothing was waiting on it, so this half is flat fact
    // — and it is what tells the reader the step is about the borrow rather
    // than about the return.
    assert!(
        detail.as_deref().is_some_and(|text| text.contains("`ReturnCopy` did not run")),
        "{detail:?}"
    );
}

#[test]
fn the_cast_lists_everybody_the_journey_named() {
    // Three ways a name gets bound — declared, described, and caught by a step
    // — and a panel that listed only the first would drop the loan, which is
    // the thing half the assertions are about.
    let walk = walked(BORROWING);
    let named: Vec<(&str, &str)> =
        walk.cast.iter().map(|member| (member.name.as_str(), member.type_expr.as_str())).collect();
    assert_eq!(named, vec![("ada", "Member"), ("copy", "catalogue/Copy"), ("loan", "Loan")]);
}

#[test]
fn the_cast_says_where_each_name_came_from() {
    use inspect_journey::Origin;
    let walk = walked(BORROWING);
    let origins: Vec<Origin> = walk.cast.iter().map(|member| member.origin).collect();
    assert_eq!(origins, vec![Origin::Cast, Origin::Cast, Origin::Caught]);
}

#[test]
fn a_cast_type_keeps_the_module_the_journey_wrote() {
    // `catalogue/Copy` read back as `Copy` would send a reader looking for the
    // type in the wrong file.
    let walk = walked(BORROWING);
    let copy = walk.cast.iter().find(|member| member.name == "copy").expect("the copy");
    assert_eq!(copy.type_expr, "catalogue/Copy");
    assert_eq!(copy.entity.as_deref(), Some("Copy#1"));
}

#[test]
fn two_of_a_kind_are_two_separate_instances() {
    // The reason a cast is instances rather than roles. Ada and Bob are both
    // members, and the journey is about them being different ones.
    let walk = walked(RESERVATIONS);
    let ada = walk.cast.iter().find(|member| member.name == "ada").expect("ada");
    let bob = walk.cast.iter().find(|member| member.name == "bob").expect("bob");
    assert_eq!(ada.type_expr, bob.type_expr);
    assert_ne!(ada.entity, bob.entity);
    assert!(ada.entity.is_some() && bob.entity.is_some());
}

#[test]
fn a_name_a_step_failed_to_catch_is_listed_with_nothing_behind_it() {
    // The name is used by every line after it, so leaving it out of the cast
    // hides the cause of all of them. Listing it empty says which name went
    // nowhere.
    let walk = walked(
        r#"
journey NothingIsCaught {
    cast:
        ada: Member

    1. she does something that creates nothing
        ada does MemberReturns(ada) on MemberShelf
            creating ghost: Reservation
}
"#,
    );
    let ghost = walk.cast.iter().find(|member| member.name == "ghost").expect("the ghost");
    assert_eq!(ghost.entity, None);
}

#[test]
fn each_step_keeps_the_world_it_left_behind() {
    // A single final state answers "what is the loan now" while hiding the step
    // that made it so — and *when* a value changed is most of what a journey is
    // written to show.
    let walk = walked(BORROWING);
    let copy = "Copy#1".to_owned();
    let status = |step: &inspect_journey::Walked| {
        step.world
            .entities
            .get(&inspect_sim::value::EntityId(copy.clone()))
            .map(|instance| instance.field("status"))
    };
    assert_eq!(status(&walk.steps[0]), Some(inspect_sim::Value::Enum("on_loan".to_owned())));
    assert_eq!(status(&walk.steps[2]), Some(inspect_sim::Value::Enum("available".to_owned())));
}

#[test]
fn every_step_carries_the_configuration_in_force() {
    // Seeded from the spec's own defaults, and the reason half the refusals in
    // a real journey happen. A panel that showed instances but not the config
    // would leave `loan_limit` invisible while it decided the outcome.
    let walk = walked(BORROWING);
    for step in &walk.steps {
        assert_eq!(step.world.config("lending", "loan_limit"), inspect_sim::Value::Int(5));
    }
}

#[test]
fn an_undecided_rule_names_the_sub_expression_that_could_not_be_settled() {
    // Two halves of one answer, and only one of them is about the world.
    // "`Member#1` has no `is_at_limit` set" says what is missing; it does not
    // say which clause asked, and `BorrowCopy` has two preconditions. The quote
    // is sliced out of the spec text, which is why the walker is handed it —
    // without it the reader is told half of what happened.
    let walk = walked(UNDECIDED);
    let act = outcomes(&walk)
        .into_iter()
        .find(|(_, about, _)| about.contains("does MemberBorrows"))
        .expect("the act");
    let detail = act.2.expect("a detail");
    assert!(detail.contains("has no `is_at_limit` set"), "{detail}");
    assert!(detail.contains("in `member.is_at_limit`"), "and which clause asked: {detail}");
}
