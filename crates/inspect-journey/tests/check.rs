//! What the spec says about a journey, before anything runs.
//!
//! Driven against the recorded `allium` output for the lending fixture, which
//! is the same graph the browser draws — so a check that passes here is a check
//! against a real ingestion rather than against a hand-built shape somebody
//! believed the CLI emits.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inspect_journey::{Verdict, check, parse};
use inspect_model::{Command, Ingestion, MemoryReader, SpecGraph, ingest, runner::MapRunner};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The library spec set, ingested exactly as the server ingests it.
fn library() -> SpecGraph {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut paths = Vec::new();

    for module in ["catalogue", "lending"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        reader = reader.with(&path, read(&path));
        paths.push(path);
    }

    let Ingestion { graph, .. } = ingest(&runner, &reader, &paths).expect("the fixtures ingest");
    graph
}

/// Check the one journey in `source` against the library spec.
fn notes(source: &str) -> Vec<(Verdict, String)> {
    let journeys = parse(source).expect("the journey parses");
    let graph = library();
    check(&journeys[0], &graph).into_iter().map(|note| (note.verdict, note.message)).collect()
}

const BORROWING: &str = include_str!("fixtures/lending.journey");

#[test]
fn a_journey_the_spec_supports_has_nothing_to_report() {
    // The fixture journeys name only constructs `lending.allium` declares, and
    // silence is the right answer to that.
    let journeys = parse(BORROWING).expect("parses");
    let graph = library();
    for journey in &journeys {
        let found = check(journey, &graph);
        assert!(found.is_empty(), "{}: {found:?}", journey.name);
    }
}

#[test]
fn a_collection_of_an_entity_the_spec_does_not_have_is_not_a_refusal() {
    // A capitalised unbound name reads as every instance of that entity, so an
    // entity that does not exist read as an *empty set* — and `ada in Dragons`
    // came back **refused**, the specification saying no about something it
    // has never heard of. A typo blaming the spec is the same failure as a
    // simulator guessing, one level up.
    let missing = notes(
        "journey J {
    cast:
        ada: Member
    1. she joins a species
        then ada in Dragons
}",
    );
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].0, Verdict::Unspecified);
    assert!(missing[0].1.contains("no entity called `Dragon`"), "{missing:?}");

    // The plural reading still resolves for entities that do exist, in both
    // spellings — `Copies` is `Copy` — and a bound name is a value rather than
    // a collection, so neither is reported.
    let real = notes(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she is one of them
        then ada in Members
        then copy in Copies
        then copy in Copy
}",
    );
    assert!(real.is_empty(), "{real:?}");

    // A name the journey *bound* is a value, not a collection — including a
    // capitalised one, which the grammar allows and which is the only shape
    // where the two halves of that guard disagree. The walk reads `Ada` as the
    // instance and says so; reporting "no entity called `Ada`" here would deny
    // a name the cast declares three lines up.
    let bound = notes(
        "journey J {
    cast:
        Ada:  Member
        copy: catalogue/Copy
    1. she is not a collection
        then copy in Ada
}",
    );
    assert!(bound.is_empty(), "{bound:?}");
}

#[test]
fn an_act_given_the_wrong_number_of_arguments_says_so() {
    // The extra one used to be bound to an invented name — `arg2` — that no
    // clause reads, so the rule fired on the two arguments it understood and
    // the journey reported every step holding. A step that hands an act one
    // argument too many is saying something about the spec that is not true,
    // and the whole point of this tool is to notice that.
    let too_many = notes(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she borrows it with a flourish
        ada does MemberBorrows(ada, copy, ada) on MemberShelf
}",
    );
    assert_eq!(too_many.len(), 1, "{too_many:?}");
    assert_eq!(too_many[0].0, Verdict::Unspecified);
    assert!(too_many[0].1.contains("takes 2 arguments"), "{too_many:?}");
    assert!(too_many[0].1.contains("member, copy"), "it names them: {too_many:?}");

    // Too few reads differently downstream — a parameter nobody bound makes
    // every clause naming it undecided — but the cause is the same fact, and
    // saying it once here beats three undecided lines that do not explain
    // themselves.
    let too_few = notes(
        "journey J {
    cast:
        ada: Member
    1. she borrows nothing in particular
        ada does MemberBorrows(ada) on MemberShelf
}",
    );
    assert_eq!(too_few.len(), 1, "{too_few:?}");
    assert!(too_few[0].1.contains("this gives 1"), "{too_few:?}");

    // And the right number is silent, so this cannot become "every act is
    // wrong".
    let right = notes(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    assert!(right.is_empty(), "{right:?}");
}

#[test]
fn a_surface_the_spec_does_not_have_is_a_requirement() {
    // Not an error. A journey is the demand written first, and a surface it
    // names is one somebody still has to specify.
    let found = notes(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she looks at a screen nobody built
        ada does MemberBorrows(ada, copy) on ReadingRoom
}",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Verdict::Unspecified);
    assert!(found[0].1.contains("no surface called `ReadingRoom`"), "{found:?}");
}

#[test]
fn an_act_offered_by_another_surface_says_which() {
    // The wrong surface and an unspecified operation are different problems
    // with different fixes, and the message is the difference.
    let found = notes(
        "journey J {
    cast:
        ada: Member
    1. she borrows at the wrong counter
        ada does MemberBorrows(ada) on CatalogueDesk
}",
    );
    let unspecified: Vec<&String> = found
        .iter()
        .filter(|(verdict, _)| *verdict == Verdict::Unspecified)
        .map(|(_, m)| m)
        .collect();
    assert!(
        unspecified
            .iter()
            .any(|message| message.contains("MemberShelf") && message.contains("does not offer")),
        "{unspecified:?}"
    );
}

#[test]
fn an_act_nobody_offers_says_that_instead() {
    let found = notes(
        "journey J {
    cast:
        ada: Member
    1. she does something nobody specified
        ada does MemberDonatesABook(ada) on MemberShelf
}",
    );
    assert!(
        found.iter().any(|(verdict, message)| *verdict == Verdict::Unspecified
            && message.contains("no surface offers `MemberDonatesABook`")),
        "{found:?}"
    );
}

#[test]
fn a_value_no_surface_exposes_is_unexposed_rather_than_unspecified() {
    // The distinction the whole design turns on: the act exists, it does what
    // it should, and nothing tells the person it happened.
    let found = notes(
        "journey J {
    cast:
        ada:  Member
        loan: Loan
    1. she looks for something the boundary does not carry
        ada sees loan.window on MemberShelf
}",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Verdict::Unexposed);
    assert!(found[0].1.contains("exposes nothing like"), "{found:?}");
}

#[test]
fn a_value_the_surface_does_expose_passes_quietly() {
    let found = notes(
        "journey J {
    cast:
        ada:  Member
        loan: Loan
    1. she reads her own loan
        ada sees loan.status on MemberShelf
        ada sees loan.is_late on MemberShelf
        ada sees ada.open_loan_count on MemberShelf
}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn cannot_see_is_answered_by_a_boundary_that_does_not_carry_it() {
    // The strongest form of the assertion, and the only one this pass can
    // settle on its own: not "no instance matched" but "this boundary does not
    // carry it at all".
    let found = notes(
        "journey J {
    cast:
        ada:  Member
        loan: Loan
    1. the shelf does not say who else has one out
        ada cannot see loan.window on MemberShelf
}",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Verdict::Specified);
    assert!(found[0].1.contains("nobody sees it there"), "{found:?}");
}

#[test]
fn a_cast_type_the_spec_does_not_declare_is_a_requirement() {
    let found = notes(
        "journey J {
    cast:
        ada: Archivist
    1. nothing
        after 1.day
}",
    );
    assert!(
        found.iter().any(|(verdict, message)| *verdict == Verdict::Unspecified
            && message.contains("no entity or actor called `Archivist`")),
        "{found:?}"
    );
}

#[test]
fn a_qualified_cast_type_resolves_across_the_module_boundary() {
    // `catalogue/Copy` is `Copy` over there, and reading the set as a set is
    // the whole reason this tool exists.
    let found = notes(
        "journey J {
    cast:
        copy: catalogue/Copy
    1. nothing
        after 1.day
}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_actor_nobody_cast_is_named_as_such() {
    let found = notes(
        "journey J {
    cast:
        ada: Member
    1. somebody who is not in this journey acts
        bruno does MemberBorrows(bruno) on MemberShelf
}",
    );
    assert!(
        found.iter().any(|(_, message)| message.contains("`bruno` is nobody in this journey")),
        "{found:?}"
    );
}

#[test]
fn a_step_that_catches_something_binds_it_for_later_steps() {
    // Bindings, not simulator ids: an agent should not have to predict which
    // number a rule's creation will be given.
    let found = notes(
        "journey J {
    cast:
        ada:  Member
        copy: catalogue/Copy
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
    2. she reads it back
        ada sees loan.status on MemberShelf
}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_rule_the_spec_does_not_have_is_a_requirement() {
    let found = notes(
        "journey J {
    cast:
        ada: Member
    1. something nobody wrote runs
        then RenewLoan fires
}",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Verdict::Unspecified);
    assert!(found[0].1.contains("no rule called `RenewLoan`"), "{found:?}");
}

#[test]
fn a_cast_type_that_disagrees_with_the_surface_is_not_rejected() {
    // A surface facing one construct and an act taking another is either a
    // small inconsistency or a person being both, and a checker that picks is
    // the same failure as a simulator that guesses. `friend-mesh` has one of
    // these and it is not obviously wrong.
    let found = notes(
        "journey J {
    cast:
        ada:  Loan
        copy: catalogue/Copy
    1. an entity acts at a surface facing an actor
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_instance_a_step_catches_can_act_in_the_next_one() {
    // The thing you caught goes on to do something — a member who registers
    // and then borrows. Only the cast list is written up front, so without
    // carrying the catch forward every such journey would be told its own
    // protagonist is nobody, and the author would go add a cast line for
    // something the journey creates.
    let reported = notes(
        "journey SheRegistersAndThenBorrows {
    cast:
        staff: Staff
        copy:  catalogue/Copy

    1. the desk enrols her
        staff does LibrarianAddsBook(staff, title, medium) on CatalogueDesk
            creating ada: Member

    2. she borrows a copy
        ada does MemberBorrows(ada, copy) on MemberShelf
}",
    );
    let strangers: Vec<_> =
        reported.iter().filter(|(_, message)| message.contains("is nobody")).collect();
    assert!(strangers.is_empty(), "{reported:#?}");
}
