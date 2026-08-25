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
fn a_field_nobody_stated_leaves_the_step_undecided() {
    // The case that stays undecided however good the simulator gets: somebody
    // has to say what `Copy.status` is, and this journey does not. The rule
    // that reads it cannot be decided, and the journey says so rather than
    // picking a side.
    //
    // This used to turn on `is_at_limit` — until the simulator learned to
    // compute derived values, at which point the case stopped being one.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she borrows it, and nobody knows whether she may
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    let act = &result.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Undecided, "{act:?}");
    assert!(act.detail.as_ref().expect("a reason").contains("status"), "{act:?}");
}

/// The other half, and the reason the case above had to move: a value the spec
/// *computes* is now computed, all the way down the chain.
///
///     loans:           Loan with member = this
///     open_loans:      loans where status = open
///     open_loan_count: open_loans.count
///     is_at_limit:     open_loan_count >= config.loan_limit
///
/// Nobody sets any of them, and the rule guarded by `not member.is_at_limit`
/// still decides — then the count moves when the borrow creates the loan,
/// which is the difference between computing a value and reading a constant.
#[test]
fn a_value_the_spec_computes_is_computed() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she has nothing out
        then ada.open_loan_count = 0
        then ada.is_at_limit = false
    2. and borrows one
        ada does MemberBorrows(ada, copy) on MemberShelf
        then BorrowCopy fires
        then ada.open_loan_count = 1
}",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And the other direction, so an empty collection passing for a computed
/// answer is not what the test above is really asserting.
#[test]
fn a_computed_value_reflects_what_the_world_holds() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she is already at the cap
        stipulate ada.open_loan_count = 5
        then ada.is_at_limit = true
    2. so she cannot take another
        ada does MemberBorrows(ada, copy) on MemberShelf
        then BorrowCopy does not fire
}",
    );

    let first = &result.steps[0];
    assert_eq!(first.verdict(), Verdict::Specified, "{:#?}", outcomes(&result));

    // The rule refuses, in the spec's own words, rather than coming back
    // undecided — which is what it did before any of this.
    let act = &result.steps[1].outcomes[0];
    assert_eq!(act.verdict, Verdict::Refused, "{act:?}");
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
fn seeing_a_value_the_surface_shows_this_actor_holds() {
    // The `exposes` clause is walked now, not merely matched: `MemberShelf`
    // exposes `Member.open_loan_count` and the loans on this member's shelf,
    // and Ada is the member. This was the honest half-answer for a while —
    // undecided, because whether the filter admitted *her* was unread — and a
    // half-answer is what it stopped being.
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it and looks at her shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada sees ada.open_loan_count on MemberShelf
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// The pair the fixture exists for. `MyLoans` is scoped —
///
/// ```text
/// context borrower: Member
/// exposes:
///     for loan in borrower.open_loans:
///         loan.status
/// ```
///
/// — so it shows a member the loans on *their* shelf and nobody else's. Ada
/// can see her own; the same clause is what stops her seeing Bob's.
#[test]
fn a_scoped_surface_shows_a_reader_their_own_row() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it and looks at her shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada sees loan.status on MyLoans
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// A field of the context itself — `borrower.name` — rather than of something
/// in a collection. It is *this* reader's name, and the same clause is what
/// makes it not somebody else's.
#[test]
fn a_scoped_surface_shows_the_reader_their_own_field() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
        bob: Member
    1. she looks at her own shelf
        ada sees ada.name on MyLoans
        ada cannot see bob.name on MyLoans
}",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And the direction a filter exists for. Answering "yes, that field is
/// exposed" here would be a privacy claim about somebody else's data.
#[test]
fn a_scoped_surface_does_not_show_a_reader_somebody_else_s_row() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. bob borrows it and ada looks at her own shelf
        bob does MemberBorrows(bob, copy) on MemberShelf creating loan: Loan
        ada cannot see loan.status on MyLoans
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
    // And it says so about *ada* — the surface carries loans, and this walk
    // does not reach one from where she stands.
    assert!(
        seen.detail
            .as_deref()
            .is_some_and(|why| why == "nothing `MyLoans` shows to ada is `loan.status`"),
        "{seen:?}"
    );
}

/// The same journey against the *unscoped* surface, which does show it —
/// so the pair of tests is about the two clauses and not about the two
/// journeys.
#[test]
fn the_unscoped_surface_shows_the_same_row_to_the_same_reader() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. bob borrows it and ada looks at the shelf
        bob does MemberBorrows(bob, copy) on MemberShelf creating loan: Loan
        ada sees loan.status on MemberShelf
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// A surface scoped to something the reader is not an instance of. The tool
/// declines rather than walking from the actor to a plausible context, because
/// which one a person is at is what a `context` declares.
#[test]
fn a_surface_scoped_to_something_else_is_undecided_and_says_so() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. the copy looks at a member's shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        loan sees loan.status on MyLoans
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Undecided, "{seen:?}");
    assert!(
        seen.detail.as_deref().is_some_and(|why| why.contains("scoped to `Member`")),
        "{seen:?}"
    );
    // And the reason carries the remedy. Before this it ended "a journey
    // cannot yet say which one it is looking at", which was true and left the
    // reader nowhere to go.
    assert!(seen.detail.as_deref().is_some_and(|why| why.contains("in <the Member>")), "{seen:?}");
}

/// A field typed by an enumeration declared elsewhere, set by a rule.
///
/// `wanted_as: catalogue/Medium` carries no states of its own — they are on
/// the enumeration it names — so `Reservation.created(wanted_as: print)` was
/// storing an unknown. In `friend-mesh` that field was `Receipt.kind`, and
/// the cost was `not exists Receipt{…, kind: read}`: the rule that checks
/// whether it has already recorded a read could not tell, so a person reading
/// the same message twice was undecidable from the second time on.
#[test]
fn a_field_typed_by_a_named_enum_is_set_to_the_state_the_rule_names() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    given:
        book.status = listed
    1. she reserves it
        ada does MemberReserves(ada, book) on MemberShelf creating held: Reservation
        then held.wanted_as = print
        then held.status = waiting
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And still checked rather than accepted on sight. A state the enumeration
/// does not have must stay unknown and be reported, or a misspelling becomes
/// a state nothing in the spec mentions.
#[test]
fn a_state_no_enumeration_declares_is_still_unknown() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    given:
        book.status = listed
    1. she reserves it, and it is not a state anything declares
        ada does MemberReserves(ada, book) on MemberShelf creating held: Reservation
        then held.wanted_as = papyrus
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Refused, "{seen:?}");
}

/// A rule's own `let`, which its preconditions then read.
///
/// `let standing = Reservation{member: member, book: book}` is computed from
/// the arguments the act supplied, so there is nothing unknowable about it —
/// but the binding was dropped between the parser and the program, so
/// `requires: exists standing` evaluated a name nothing had bound and the act
/// came back undecided. Three of `friend-mesh`'s four contact-naming rules are
/// this shape.
#[test]
fn a_rules_own_let_is_bound_before_its_preconditions_read_it() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    given:
        book.status = listed
    1. she reserves it
        ada does MemberReserves(ada, book) on MemberShelf creating held: Reservation
    2. and changes her mind
        ada does MemberWithdrawsReservation(ada, book) on MemberShelf
        then WithdrawReservation fires
        then held.status = cancelled
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And a `let` the simulator cannot settle stays undecided, saying what
/// defeated it rather than "nothing is bound to `standing`" — which would
/// point at the `let` instead of at the thing it could not work out.
#[test]
fn a_let_that_could_not_be_settled_says_what_defeated_it() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    1. she withdraws a reservation over a book nobody described
        ada does MemberWithdrawsReservation(ada, book) on MemberShelf
}",
    );
    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Refused, "{seen:?}");
    assert!(seen.detail.as_deref().is_some_and(|why| why.contains("exists standing")), "{seen:?}");
}

/// A journey saying an act happened means everything the act set off.
///
/// `BorrowCopy` emits `CopyBorrowed`; `NoteTheBorrowing` waits on it and has no
/// preconditions of its own. The simulator's step is one step — it offers the
/// emitted trigger to the reader, because in the browser a person picks which
/// to follow — and the walk stopped there too, so the second rule was reported
/// as never having run. In `friend-mesh` that rule was `QueueOnSend`, and the
/// whole outbox hung off the one line it could not reach.
#[test]
fn an_act_runs_the_rules_its_emissions_wake() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it, and the desk is told
        ada does MemberBorrows(ada, copy) on MemberShelf
        then BorrowCopy fires
        then NoteTheBorrowing fires
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And what that rule made can be caught, because now it exists.
///
/// This is the half a journey could not write at all: the thing a chained rule
/// creates is the thing a guarantee about it is *about*, and it had no name.
#[test]
fn what_a_chained_rule_creates_can_be_caught_and_read() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it and the notice is raised
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating notice: Notice
        then notice exists
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The emission carries what the rule handed it, under the name the trigger
/// declares. Without that the woken rule fires against nothing and creates a
/// notice pointing at no loan — which is worse than not running at all,
/// because it looks like it worked.
#[test]
fn a_woken_rule_gets_the_arguments_the_emission_carried() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it, and the notice names the loan she took
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating notice: Notice
        then notice.loan exists
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// An exposure that walks further than one hop, which is the ordinary shape
/// once a spec has more than one noun.
///
/// ```text
/// for loan in borrower.open_loans:
///     loan.copy.shelfmark
/// ```
///
/// The reader asking is asking about the *copy* — that is the thing they were
/// handed and the thing they named — and the surface reaches it through the
/// loan it hangs off. Matching only the last field made these two different
/// questions, and the tool answered "this surface exposes nothing like
/// `copy.shelfmark`" about a clause that exposes exactly that.
#[test]
fn an_exposure_that_walks_further_than_one_hop_is_still_this_walk() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it and looks for it on her shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada sees copy.shelfmark on MyLoans
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// And the privacy direction over the same clause. Bob's shelf reaches Bob's
/// copies, and a walk that passes through an element of somebody else's
/// collection arrives nowhere.
#[test]
fn a_longer_walk_still_only_reaches_what_this_readers_shelf_holds() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. bob borrows it and ada looks for it on her own shelf
        bob does MemberBorrows(bob, copy) on MemberShelf creating loan: Loan
        ada cannot see copy.shelfmark on MyLoans
        bob sees copy.shelfmark on MyLoans
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The failure the old rule had pointed the other way. `loan.copy.shelfmark`
/// is not an answer to a question about a loan's shelfmark, and matching on
/// the last field alone made every clause ending in one an answer to every
/// question about anybody's.
#[test]
fn a_walk_that_ends_the_same_way_off_something_else_is_not_this_walk() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she looks for a shelfmark on the loan rather than on the copy
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada cannot see loan.shelfmark on MyLoans
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// The remedy, used. A surface scoped to something the actor is not an
/// instance of is the ordinary case in a real spec set — `GroupMembers` is
/// scoped to a `Group` and faces a person, and a person is in several groups —
/// so which one they have open is a fact about them that only the journey has.
#[test]
fn a_journey_can_say_which_one_it_is_looking_at() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. the copy is looked at on a member's shelf
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        loan sees loan.status on MyLoans in ada
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// Named, and the wrong kind of thing. Reported against the name the journey
/// wrote rather than against the actor, because that is the word to change.
#[test]
fn naming_something_that_is_not_the_context_says_which_it_is() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she looks at a shelf scoped to a copy
        ada sees ada.name on MyLoans in copy
}",
    );
    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Undecided, "{seen:?}");
    assert!(seen.detail.as_deref().is_some_and(|why| why.contains("`copy` is `Copy`")), "{seen:?}");
}

/// The privacy direction, with the context said out loud. Bob's loan is on
/// Bob's shelf and not on Ada's, and naming the shelf is what makes the two
/// claims different claims rather than the same one twice.
#[test]
fn the_named_context_is_whose_row_is_shown() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. bob borrows it, and the two shelves are looked at
        bob does MemberBorrows(bob, copy) on MemberShelf creating loan: Loan
        ada sees loan.status on MyLoans in bob
        ada cannot see loan.status on MyLoans in ada
}",
    );
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A name nothing in the journey bound. The static pass answers it before the
/// walk gets there, which is the better answer: a name nobody wrote is a
/// requirement on the journey rather than a question about the spec.
#[test]
fn naming_a_context_the_journey_never_bound_is_reported_as_missing() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
    1. she looks at somewhere nobody named
        ada sees ada.name on MyLoans in elsewhere
}",
    );
    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Unspecified, "{seen:?}");
    assert!(
        seen.detail.as_deref().is_some_and(|why| why.contains("`elsewhere` is nothing")),
        "{seen:?}"
    );
}

/// An exposure written over a *type* shows every instance of it, and this
/// fixture is the argument for being able to say so.
///
/// `MemberShelf` exposes `Loan.status` with no `context`, so it shows every
/// loan to everyone it faces — including Bob's, to Ada. Written on the same
/// surface, four lines below:
///
/// ```text
/// @guarantee ALoanIsVisibleToItsHolderOnly
///     -- A shelf shows the reader their own loans. Whose copy is out is a
///     -- fact about another member, and this boundary does not carry it.
/// ```
///
/// The clause and the guarantee disagree, and only one of them is checkable.
/// Before the clause was walked, a journey could not have told anybody.
#[test]
fn an_exposure_over_a_type_shows_every_instance_of_it() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. bob borrows it and ada looks at her own shelf
        bob does MemberBorrows(bob, copy) on MemberShelf creating loan: Loan
        ada cannot see loan.status on MemberShelf
}",
    );
    let seen = &result.steps[0].outcomes[1];
    assert_eq!(seen.verdict, Verdict::Refused, "{seen:?}");
    assert!(seen.detail.as_deref().is_some_and(|why| why.contains("does show")), "{seen:?}");
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
    //
    // This used to assert that no detail contained "never settled" — a string
    // the walker pushed into `undecided`, which is a list of *rule names* read
    // by exact match. It reached no detail, so the assertion could not fail.
    // What can fail is the verdict: drop the fixpoint and every `after` in
    // this journey goes undecided, because the world never stops changing.
    let walk = walked(LOSS);
    assert_eq!(
        walk.verdict(),
        Verdict::Specified,
        "the fixpoint holds, so the walk settles: {walk:#?}"
    );
    let unsettled: Vec<_> = outcomes(&walk)
        .into_iter()
        .filter(|(_, _, detail)| {
            detail.as_deref().is_some_and(|text| text.contains("had not stopped changing"))
        })
        .collect();
    assert!(unsettled.is_empty(), "{unsettled:#?}");
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
fn a_negated_sight_holds_because_the_boundary_does_not_carry_it() {
    // `cannot see` is answered from the surface, not from the value. This used
    // to report "copy.shelfmark has no value here", which is a fact about the
    // world and never the reason the claim holds — and reading the value was
    // what made a privacy claim pass on any field nothing had set, including
    // fields the surface exposes on the line above.
    let (verdict, _, detail) = form("cannot see copy.shelfmark");
    assert_eq!(verdict, Verdict::Specified);
    assert_eq!(detail.as_deref(), Some("`MemberShelf` exposes nothing like `copy.shelfmark`"));
}

#[test]
fn a_privacy_claim_about_an_exposed_field_is_refused() {
    // The one that matters. `MemberShelf` exposes `Member.open_loan_count` and
    // Ada is the member, so she can see it and the claim that she cannot is
    // the specification saying otherwise.
    //
    // This has been three answers. It *held* once, because the walker read the
    // value, found it unset and called the claim safe — a privacy claim passing
    // because nothing checked it, which is the worst answer here. Then it was
    // undecided, which was honest and unhelpful. Now the clause is walked and
    // the answer is a refusal, which is what it always was.
    let walk = walked(
        "journey J {
    cast:
        ada: Member
    1. she claims not to see it
        after 1.day
        ada cannot see ada.open_loan_count on MemberShelf
}",
    );
    let outcome = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("cannot see"))
        .expect("the claim is reported");
    assert_eq!(outcome.verdict, Verdict::Refused, "{outcome:?}");
    assert!(
        outcome.detail.as_deref().is_some_and(|detail| detail.contains("does show")),
        "and says so plainly: {:?}",
        outcome.detail
    );
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
const ABSENCE: &str = include_str!("fixtures/absence.journey");

#[test]
fn every_fault_on_one_line_is_reported_at_once() {
    // A line can be wrong in more than one way. Showing the first and stopping
    // meant fixing it, re-running, finding the second, and walking the same
    // journey three times to learn what one report could have said.
    let walk = walked(
        "journey J {
    cast:
        ada: Member
    1. she reaches past a desk that is not there
        nobody does NoSuchTrigger(ada) on NoSuchSurface
}",
    );

    let faults: Vec<_> = walk.steps[0]
        .outcomes
        .iter()
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .collect();
    assert!(faults.len() >= 2, "one line, every fault on it: {faults:#?}");

    let said = faults.iter().filter_map(|outcome| outcome.detail.as_deref()).collect::<Vec<_>>();
    assert!(
        said.iter().any(|detail| detail.contains("NoSuchSurface")),
        "the surface is named: {said:?}"
    );
    assert!(
        said.iter().any(|detail| detail.contains("NoSuchTrigger") || detail.contains("nobody")),
        "and so is the other fault: {said:?}"
    );
}

#[test]
fn a_cast_naming_a_type_the_spec_does_not_have_is_not_satisfied() {
    // The flagship case of the whole design — a requirement nobody has met —
    // and it reported "1 of 1 steps hold", exit 0, no diagnostics. `check`
    // settled it correctly at the cast line; both consumers then filtered
    // notes by *clause* line, and a cast line is never one, so the answer was
    // computed and thrown away. The unit test that guarded it asserted at the
    // `check()` level, which is exactly why it never noticed.
    let walk = walked(
        "journey SheIsNobody {
    cast:
        ada: Archivist
    1. she waits
        after 1.day
}",
    );

    assert_ne!(
        walk.verdict(),
        Verdict::Specified,
        "a journey whose cast the spec cannot supply is not satisfied: {walk:#?}"
    );
    let note = walk.notes.first().expect("the cast is reported");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.about.contains("ada"), "the note names the member: {note:?}");
    assert!(
        note.detail.as_deref().is_some_and(|detail| detail.contains("Archivist")),
        "the reason names the type nobody declared: {:?}",
        note.detail
    );
}

#[test]
fn a_note_already_shown_on_its_step_is_not_repeated_outside_it() {
    // The two halves of the filter, both directions. A journey with a bad cast
    // *and* a bad clause: the cast note has no step to sit on and belongs in
    // `notes`; the clause note is already reported against its own line by
    // `walk_step`, and listing it twice would have the reader chasing one
    // fault through two places.
    let walk = walked(
        "journey J {
    cast:
        ada: Archivist
    1. she reaches past the desk
        ada does MemberBorrows(ada, ada) on NoSuchSurface
}",
    );

    assert_eq!(walk.notes.len(), 1, "only the cast line: {:#?}", walk.notes);
    assert!(walk.notes[0].about.contains("Archivist"), "{:?}", walk.notes[0]);

    let clause_faults = walk.steps[0]
        .outcomes
        .iter()
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .count();
    assert!(clause_faults > 0, "the clause is still reported on its own step: {walk:#?}");
}

#[test]
fn a_cast_note_names_the_member_it_is_about() {
    // Two people, one of them fictional. The note is matched to a cast member
    // by line, and matching the wrong one would put a real name against a
    // complaint about a type it does not have.
    let walk = walked(
        "journey J {
    cast:
        ada:      Member
        archivist: Archivist
    1. she waits
        after 1.day
}",
    );

    let note = walk.notes.first().expect("the cast is reported");
    assert!(note.about.contains("archivist"), "names the member with the bad type: {note:?}");
    assert!(!note.about.contains("ada:"), "and not the one whose type is fine: {note:?}");
}

#[test]
fn a_given_that_wrote_nothing_is_reported_too() {
    // The same fault one line earlier. A `given` naming a root the journey
    // never bound used to return silently, so every assertion afterwards was
    // answered against a world nobody arranged — and nothing said so.
    let walk = walked(
        "journey J {
    cast:
        ada: Member
    given:
        nobody.name = \"Ada\"
    1. she waits
        after 1.day
}",
    );

    let note =
        walk.notes.iter().find(|note| note.about.contains("given")).expect("the given is reported");
    assert_eq!(note.verdict, Verdict::Undecided);
    assert!(
        note.detail.as_deref().is_some_and(|detail| detail.contains("nobody")),
        "the reason names the root: {:?}",
        note.detail
    );
    assert_ne!(walk.verdict(), Verdict::Specified);
}

#[test]
fn a_given_instance_of_a_type_the_spec_does_not_have_is_reported() {
    // The cast fault one line down, and it survived the fix to the cast for the
    // same reason: `check` registered a given instance's *name* so later lines
    // could refer to it, and never looked at its type. A mutation on the line
    // that names it is what surfaced this — nothing reached that branch,
    // because nothing ever put a note on a `given` line.
    let walk = walked(
        "journey J {
    cast:
        ada: Member
    given:
        ghost: Phantom { name: \"nobody\" }
    1. she waits
        after 1.day
}",
    );

    assert_ne!(walk.verdict(), Verdict::Specified, "{walk:#?}");
    let note = walk
        .notes
        .iter()
        .find(|note| note.about.contains("ghost"))
        .expect("the given instance is reported");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.about.contains("Phantom"), "names the type nobody declared: {note:?}");
    assert!(!note.about.contains("ada"), "and not the cast member above it: {note:?}");
}

#[test]
fn a_stipulation_reaches_through_a_reference_to_the_field_it_names() {
    // The other half of the nested write. `loan.member` is a live reference, so
    // `loan.member.name` must land on the *member's* name — not fail, and not
    // write `member` on the loan. Without this only the failing direction was
    // covered, and deleting the arm that follows a reference changed nothing
    // any test could see.
    let walk = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
    2. somebody corrects her name
        stipulate loan.member.name = \"Ada Lovelace\"
        then ada.name = \"Ada Lovelace\"
}",
    );

    let stipulation = walk.steps[1]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("stipulate"))
        .expect("the stipulation is reported");
    assert_eq!(stipulation.verdict, Verdict::Specified, "the write lands: {stipulation:?}");
    assert_eq!(
        walk.stipulated,
        vec!["loan.member.name = \"Ada Lovelace\"".to_owned()],
        "and is listed once it has"
    );
    // Read back through the other name for the same instance, which is the
    // whole point of following the reference rather than the first segment.
    let read = walk.steps[1]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("ada.name"))
        .expect("the read-back is reported");
    assert_eq!(read.verdict, Verdict::Specified, "{read:?}");
}

#[test]
fn a_stipulation_that_wrote_nothing_says_so_instead_of_printing_itself() {
    // The ledger is the guardrail this design leans on: an agent can make any
    // journey pass, but it cannot make one pass *invisibly*. A stipulation
    // that quietly wrote nothing and then listed itself anyway breaks exactly
    // that — the reader is shown a change to the world that never happened,
    // which is worse than being shown nothing.
    let walk = walked(
        "journey J {
    cast:
        ada: Member
    1. she waits
        after 1.day
        stipulate nobody.status = open
}",
    );

    let outcome = walk.steps[0]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("stipulate"))
        .expect("the stipulation is reported");
    assert_eq!(outcome.verdict, Verdict::Undecided, "{outcome:?}");
    assert!(
        outcome.detail.as_deref().is_some_and(|detail| detail.contains("nobody")),
        "the reason names the root that bound nothing: {:?}",
        outcome.detail
    );
    assert!(
        walk.stipulated.is_empty(),
        "nothing was written, so the ledger lists nothing: {:?}",
        walk.stipulated
    );
}

#[test]
fn a_stipulation_writes_the_field_it_names_and_not_the_first_one() {
    // `loan.window.due_at` used to set `loan.window`, because the write took
    // `segments.first()` and stopped. The ledger printed the path in full, so
    // the report said one thing and the world held another — and a later
    // assertion about `loan.window` would then agree with a value nobody wrote.
    let walk = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = available
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
    2. somebody asserts a due date
        stipulate loan.window.due_at = 1.day
        then loan.window != 1.day
}",
    );

    let stipulation = walk.steps[1]
        .outcomes
        .iter()
        .find(|outcome| outcome.about.contains("stipulate"))
        .expect("the stipulation is reported");
    assert_ne!(
        stipulation.verdict,
        Verdict::Specified,
        "the window is unset, so there is nothing to write `due_at` on: {stipulation:?}"
    );
    assert!(
        walk.stipulated.is_empty(),
        "a write that could not be followed lists nothing: {:?}",
        walk.stipulated
    );
}

#[test]
fn an_unknown_is_not_an_absence_in_either_direction() {
    // Both failure modes stipulation 1 names, from one assertion. Reading a
    // path that ran out as *absent* makes `does not exist` hold on a world
    // nothing described; reading it as *present* makes `exists` refuse, which
    // is the spec saying no to a question nobody asked it. The comparison
    // arm three lines away already gets this right, which is what made it
    // survive: every test written for `exists` used a name the journey had
    // either bound or never mentioned, and both of those are decidable.
    let walk = walked(ABSENCE);
    let steps = walk.steps.iter().map(|step| step.verdict()).collect::<Vec<_>>();
    assert_eq!(
        steps,
        vec![Verdict::Undecided, Verdict::Undecided],
        "an unread path is undecided whichever way it is asked, not absent one way and \
         present the other"
    );

    for step in &walk.steps {
        let outcome = step
            .outcomes
            .iter()
            .find(|outcome| outcome.about.contains("joined_at"))
            .expect("the assertion is reported");
        assert_eq!(outcome.verdict, Verdict::Undecided);
        // An unknown with no reason is indistinguishable from a bug.
        assert!(
            outcome.detail.as_deref().is_some_and(|detail| detail.contains("joined_at")),
            "the reason names the path that could not be read: {:?}",
            outcome.detail
        );
    }
}

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
    let unsettled: Vec<_> = outcomes(&walk)
        .into_iter()
        .filter(|(_, _, detail)| {
            detail.as_deref().is_some_and(|text| text.contains("had not stopped changing"))
        })
        .collect();
    assert!(unsettled.is_empty(), "{unsettled:#?}");
    // Both readers, which is the half a rule-only fixpoint drops.
    assert!(
        walk.steps.iter().all(|step| step.verdict() != Verdict::Undecided),
        "nothing was left undecided: {walk:#?}"
    );
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
    // "`Copy#1` has no `status` set" says what is missing; it does not say
    // which clause asked, and `BorrowCopy` has two preconditions. The quote is
    // sliced out of the spec text, which is why the walker is handed it —
    // without it the reader is told half of what happened.
    let walk = walked(UNDECIDED);
    let act = outcomes(&walk)
        .into_iter()
        .find(|(_, about, _)| about.contains("does MemberBorrows"))
        .expect("the act");
    let detail = act.2.expect("a detail");
    assert!(detail.contains("has no `status` set"), "{detail}");
    assert!(detail.contains("in `copy.status`"), "and which clause asked: {detail}");
}

// --- naming a moment -----------------------------------------------------

/// The clock arithmetic, asserted where it lands rather than where it parses.
///
/// The clock starts at zero, so a journey that says `now + 1.day` at the very
/// beginning cannot tell an offset that is added from one that is multiplied.
/// Every case here moves the clock first, for that reason.
#[test]
fn a_moment_is_measured_from_the_clock_where_the_line_stands() {
    let result = walked(
        "\
journey AMomentLater {
    goal: a deadline set an hour in, and reached two days later

    cast:
        held: Reservation

    1. an hour passes
        after 1.hour
        stipulate held.placed_at = now + 1.day
        then held.placed_at > now

    2. and then two days
        after 2.days
        then held.placed_at < now
}
",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The other direction, so a sign flip is not a passing test.
#[test]
fn a_moment_before_now_is_already_past() {
    let result = walked(
        "\
journey AMomentAgo {
    goal: a deadline that was already behind us

    cast:
        held: Reservation

    1. an hour passes
        after 1.hour
        stipulate held.placed_at = now - 1.minute
        then held.placed_at < now
}
",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Bare `now` is the clock itself: neither before nor after it.
#[test]
fn bare_now_is_neither_before_nor_after_the_clock() {
    let result = walked(
        "\
journey RightNow {
    goal: a moment that is exactly the clock

    cast:
        held: Reservation

    1. an hour passes
        after 1.hour
        stipulate held.placed_at = now
        then held.placed_at > now
}
",
    );

    // `>` is false, not undecided: the comparison was made and answered.
    let verdicts: Vec<Verdict> = outcomes(&result).into_iter().map(|(v, ..)| v).collect();
    assert!(verdicts.contains(&Verdict::Refused), "{:#?}", outcomes(&result));
}

/// A `creating` that catches nothing is almost never the fault.
///
/// A rule that could not be decided creates nothing, and reporting only the
/// empty hands sent a reader looking at their own journey for a mistake that
/// was three lines up in the specification. `BorrowCopy` cannot be decided
/// here — nobody said what the copy's status is — so the note has to carry
/// both halves.
#[test]
fn a_creating_that_caught_nothing_says_why() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she borrows a copy nobody has described
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
}",
    );

    let detail = result.steps[0].outcomes[0].detail.as_deref().expect("a reason");
    assert!(detail.contains("nothing of kind `Loan` was created"), "the symptom: {detail}");
    assert!(detail.contains("could not be decided"), "and the cause: {detail}");
    assert!(detail.contains("copy.status"), "named: {detail}");
}

/// An optional parameter nobody passed is `null`, which is what the `?` means.
///
/// `MemberReportsLoss(loan, note?)` is guarded by `note = null or note != ""`,
/// and nearly every caller omits the note. Left unbound the guard is undecided
/// for all of them — so the rule was unreachable by exactly the callers it was
/// written for.
#[test]
fn an_optional_argument_nobody_passed_is_null() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows it and then loses it
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada does MemberReportsLoss(loan) on MemberShelf
        then ReportCopyLost fires
        then copy.status = lost
}",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// And one that *is* passed keeps its value rather than being nulled.
#[test]
fn an_optional_argument_that_was_passed_is_what_was_passed() {
    let result = walked(
        r#"journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she loses it and says so
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
        ada does MemberReportsLoss(loan, "left on a train") on MemberShelf
        then ReportCopyLost fires
}"#,
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

// --- stipulating a call --------------------------------------------------
//
// `may_reserve(member, book)` is named by the specification and defined
// nowhere. That is an ordinary state for a spec to be in — the policy has not
// been decided — and a permanent one for a simulator: there is nothing to work
// out, now or ever. So somebody says, and the saying goes in the ledger.

#[test]
fn a_call_the_spec_never_defines_can_be_answered_by_the_journey() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    1. she reserves it, and the policy says she may
        stipulate may_reserve(ada, book) = true
        ada does MemberReservesQuietly(ada, book) on MemberShelf
        then ReserveQuietly fires
}",
    );

    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    assert_eq!(result.stipulated, ["may_reserve(ada, book) = true"]);
}

/// The other answer, which has to be sayable too or the feature is a way of
/// making journeys pass.
#[test]
fn a_call_answered_false_refuses_the_rule() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    1. she reserves it, and the policy says she may not
        stipulate may_reserve(ada, book) = false
        ada does MemberReservesQuietly(ada, book) on MemberShelf
        then ReserveQuietly does not fire
}",
    );

    let act = &result.steps[0].outcomes[1];
    assert_eq!(act.verdict, Verdict::Refused, "{act:?}");
}

/// Without one it stays undecided, which is the honest answer and the reason
/// the clause exists.
#[test]
fn a_call_nobody_answered_is_still_undecided() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    1. she reserves it
        ada does MemberReservesQuietly(ada, book) on MemberShelf
}",
    );

    let act = &result.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Undecided, "{act:?}");
    assert!(
        act.detail.as_deref().is_some_and(|why| why.contains("a function call is not simulated")),
        "{act:?}"
    );
}

/// An answer is about *these* arguments. Answering `may_reserve(ada, book)`
/// says nothing about Bob, and matching it to him would be the tool inventing
/// a policy nobody stated.
#[test]
fn an_answer_is_about_the_arguments_it_names() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        bob:  Member
        book: catalogue/Book
    1. ada may, and nobody said anything about bob
        stipulate may_reserve(ada, book) = true
        bob does MemberReservesQuietly(bob, book) on MemberShelf
}",
    );

    let act = &result.steps[0].outcomes[1];
    assert_eq!(act.verdict, Verdict::Undecided, "{act:?}");
}

/// Every one of them is in the ledger. A journey can be told anything; it
/// cannot be told it invisibly.
#[test]
fn a_stipulated_call_is_reported_like_every_other_stipulation() {
    let result = walked(
        "journey J {
    cast:
        ada:  Member
        book: catalogue/Book
    1. two things said outright
        stipulate ada.open_loan_count = 0
        stipulate may_reserve(ada, book) = true
}",
    );

    assert_eq!(result.stipulated, ["ada.open_loan_count = 0", "may_reserve(ada, book) = true"]);
}

// --- a call a surface exposes --------------------------------------------
//
// `exposes: in_good_standing(reader)` is a thing a surface may show and a
// thing no field can stand for. The clause writes the surface's own name for
// whoever it faces; the journey writes who that is. One call about one person,
// written two ways, and comparing the *text* of them says no to every case
// there is — which is how a real spec's `announces_reads(owner)` came back as
// "exposes nothing like `announces_reads(ada)`" and read as a spec gap.

#[test]
fn a_call_a_surface_exposes_is_matched_by_name_and_by_who() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
    1. she looks at her own standing
        ada sees in_good_standing(ada) on MemberShelf
}",
    );

    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// About the person it names. The surface shows a reader *their* standing, and
/// answering for somebody else would be the same privacy mistake a filter
/// exists to prevent.
#[test]
fn a_call_a_surface_exposes_is_about_who_it_names() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
        bob: Member
    1. she looks for his standing
        ada cannot see in_good_standing(bob) on MemberShelf
}",
    );

    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Specified, "{seen:?}");
}

/// A call the surface does not expose at all is settled from the clause text,
/// the same as a field it does not carry.
#[test]
fn a_call_a_surface_does_not_expose_is_unexposed() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
    1. she looks for something nobody shows
        ada sees credit_rating(ada) on MemberShelf
}",
    );

    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Unexposed, "{seen:?}");
    assert!(
        seen.detail.as_deref().is_some_and(|why| why.contains("exposes nothing like")),
        "{seen:?}"
    );
}

/// The same call name with the wrong number of arguments is a different call.
/// The clause text matches on the name alone, so this is the only thing that
/// stops `in_good_standing(ada, bob)` reading as the exposure it is not.
#[test]
fn a_call_with_the_wrong_number_of_arguments_is_not_the_one_exposed() {
    let result = walked(
        "journey J {
    cast:
        ada: Member
        bob: Member
    1. she asks a question the surface does not answer
        ada sees in_good_standing(ada, bob) on MemberShelf
}",
    );

    let seen = &result.steps[0].outcomes[0];
    assert_eq!(seen.verdict, Verdict::Unexposed, "{seen:?}");
}
