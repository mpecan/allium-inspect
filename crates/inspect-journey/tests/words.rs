//! A bare word in a value position, against the states the spec declares.
//!
//! A word the journey did not bind is read as the state it spells. That is
//! right when the spec declares it, and when it does not the walk compared it
//! anyway: `copy.status = availble` laid out a status no lifecycle mentions,
//! `then loan.status = opne` was refused as though the spec had said no, and
//! `then copy.status != on_lone` held — the specification agreeing with a
//! sentence about a state it has never heard of.
//!
//! Each case is checked where it is written: in `given`, in `then`, in a
//! `stipulate`, and in the file's `world`. Where the field is known, against
//! that field's own states; where it is not, against every state the spec
//! declares.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::LENDING;
use inspect_journey::{Note, Verdict, Walk, check, parse, walk};

const CAST: &str = "
    cast:
        ada:  Member
        copy: catalogue/Copy";

fn journey(given: &str, steps: &str) -> String {
    format!(
        "journey J {{{CAST}
    given:
        ada.is_at_limit = false
{given}
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
{steps}
}}"
    )
}

fn checked(source: &str) -> Vec<Note> {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, _) = common::library(LENDING);
    check(&journeys[0], &journeys, &graph, &program)
}

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(LENDING);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

/// The one note mentioning `word`.
fn about<'a>(notes: &'a [Note], word: &str) -> &'a Note {
    let needle = format!("`{word}`");
    let found: Vec<&Note> = notes.iter().filter(|note| note.message.contains(&needle)).collect();
    assert_eq!(found.len(), 1, "one note about {word}: {notes:#?}");
    found[0]
}

/// The outcome of the line that reads `written`.
fn line(walk: &Walk, written: &str) -> (Verdict, Option<String>) {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == written)
        .map(|outcome| (outcome.verdict, outcome.detail.clone()))
        .unwrap_or_else(|| panic!("no line `{written}` in {:#?}", walk.steps))
}

// --- a field whose states are known -----------------------------------------

/// `availble` for `available`: no state of `Copy.status`, and close enough to
/// one to say which.
#[test]
fn a_misspelt_state_in_given_is_reported_against_the_field() {
    let notes = checked(&journey("        copy.status = availble", ""));
    let note = about(&notes, "availble");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.message.contains("`Copy.status`"), "{}", note.message);
    assert!(note.message.contains("did you mean `available`?"), "{}", note.message);
}

/// An instance written inline has its field's type too.
#[test]
fn a_misspelt_state_in_an_inline_instance_is_reported() {
    let notes = checked(&journey(
        "        copy.status = available\n        res: Reservation { member: ada, status: wating }",
        "",
    ));
    let note = about(&notes, "wating");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.message.contains("`Reservation.status`"), "{}", note.message);
    assert!(note.message.contains("did you mean `waiting`?"), "{}", note.message);
}

/// A state the spec declares — for another field. `available` is a copy's,
/// and a loan is never in it, so `then loan.status = available` can never hold
/// and the spec has not said it could.
#[test]
fn a_state_of_another_field_is_reported_against_this_one() {
    let notes = checked(&journey(
        "        copy.status = available",
        "        then loan.status = available",
    ));
    let note = about(&notes, "available");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.message.contains("`Loan.status`"), "{}", note.message);
    assert!(note.message.contains("open, returned, overdue"), "{}", note.message);
}

/// The walk does not compare it either way. `!=` against a state nobody has
/// used to *hold*, and `=` used to be refused — the spec saying no to
/// something it was never asked.
#[test]
fn a_then_naming_no_state_of_its_field_is_unspecified_rather_than_compared() {
    let walk = walked(&journey(
        "        copy.status = available",
        "        then loan.status = opne\n        then copy.status != on_lone",
    ));
    let (verdict, detail) = line(&walk, "then loan.status = opne");
    assert_eq!(verdict, Verdict::Unspecified, "{detail:?}");
    assert!(detail.unwrap_or_default().contains("`Loan.status`"));
    let (verdict, detail) = line(&walk, "then copy.status != on_lone");
    assert_eq!(verdict, Verdict::Unspecified, "{detail:?}");
    assert!(detail.unwrap_or_default().contains("did you mean `on_loan`?"));
}

/// Through more than one hop, when each hop's type is one the spec declares.
#[test]
fn the_field_is_found_through_a_path() {
    let notes = checked(&journey(
        "        copy.status = available",
        "        then loan.copy.status = on_lone",
    ));
    let note = about(&notes, "on_lone");
    assert!(note.message.contains("`Copy.status`"), "{}", note.message);
}

/// A stipulation writes into the world the same way `given` does.
#[test]
fn a_misspelt_state_in_a_stipulation_is_reported() {
    let notes = checked(&journey(
        "        copy.status = available",
        "        stipulate loan.status = overdeu",
    ));
    let note = about(&notes, "overdeu");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.message.contains("did you mean `overdue`?"), "{}", note.message);
}

// --- a field with no states, or none known ------------------------------------

/// `name` is a `String`, and `bob` is neither somebody the journey cast nor any
/// state the spec declares.
#[test]
fn a_word_nothing_declares_in_a_field_with_no_states_is_reported() {
    let notes = checked(&journey("        copy.status = available\n        ada.name = bob", ""));
    let note = about(&notes, "bob");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.message.contains("nobody in this journey"), "{}", note.message);
}

/// The file's `world` is laid out under the journey, so a word in it is
/// checked the same way.
#[test]
fn a_misspelt_state_in_the_world_is_reported() {
    let source = format!(
        "world {{{CAST}
    given:
        copy.status = availble
}}

journey J {{
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
}}"
    );
    let notes = checked(&source);
    assert_eq!(about(&notes, "availble").verdict, Verdict::Unspecified);
    // And the walk names the line by what it says, and leaves it unset.
    let walk = walked(&source);
    assert!(
        walk.notes.iter().any(|note| note.about == "given copy.status = availble"
            && note.verdict == Verdict::Unspecified),
        "{:#?}",
        walk.notes
    );
    assert_eq!(walk.steps[0].outcomes[0].verdict, Verdict::Undecided);
}

// --- what stays as it was ------------------------------------------------------

/// Every word here is a state of the field it is written against, or somebody
/// the journey cast, or a literal — and none of it is reported.
#[test]
fn declared_states_and_cast_names_are_not_reported() {
    let notes = checked(&journey(
        "        copy.status = available\n        res: Reservation { member: ada, status: waiting, wanted_as: print }\n        ada.name = \"Ada\"",
        "        then loan.status = open\n        then loan.member = ada\n        then copy.status != lost\n        stipulate loan.status = overdue",
    ));
    assert!(notes.is_empty(), "{notes:#?}");
}

/// A word nothing declares, in a field typed by an enumeration — the field's
/// states are the enumeration's.
#[test]
fn a_field_typed_by_an_enumeration_is_checked_against_its_values() {
    let notes = checked(&journey(
        "        copy.status = available\n        res: Reservation { member: ada, status: waiting, wanted_as: audoi }",
        "",
    ));
    let note = about(&notes, "audoi");
    assert!(note.message.contains("`Reservation.wanted_as`"), "{}", note.message);
    assert!(note.message.contains("print, audio"), "{}", note.message);
}

/// A call's arguments are matched against the words the spec spells in that
/// call, not against states — `stipulate can(r, code)` names a word no
/// enumeration need declare — so they are left to the walk.
#[test]
fn a_stipulated_calls_arguments_are_not_checked() {
    let notes = checked(&journey(
        "        copy.status = available",
        "        stipulate may_reserve(ada, wibble) = true",
    ));
    assert!(notes.iter().all(|note| !note.message.contains("`wibble`")), "{notes:#?}");
}

/// `config` is not cast and is not a state, and what is written into one is
/// checked against the parameter's type by the walk — so a bare word there is
/// no note of the checker's, and the walk, which does not leave it unset as a
/// misspelt state, says what is wrong with it.
#[test]
fn configuration_is_left_to_the_walk() {
    let source = journey("        copy.status = available\n        config.loan_limit = strict", "");
    assert!(checked(&source).is_empty(), "{:#?}", checked(&source));
    let walk = walked(&source);
    assert!(
        walk.notes.iter().any(|note| note.about == "given config.loan_limit"),
        "{:#?}",
        walk.notes
    );
}

/// Every fixture journey, against its own spec set, is untouched by the word
/// checks: no word reported as no state or nobody, and no remark but the one
/// the chains journey exists to show.
#[test]
fn the_fixtures_own_journeys_report_no_word() {
    let sets: &[(&str, &[&str])] = &[
        ("absence", LENDING),
        ("chains", common::CHAINS),
        ("facing", common::FACING),
        ("fanout", common::FANOUT),
        ("forms", LENDING),
        ("grants", common::GRANTS),
        ("guards", common::GUARDS),
        ("lending", LENDING),
        ("loss", LENDING),
        ("orders", common::ORDERS),
        ("packing", common::PACKING),
        ("pinboard", common::PINBOARD),
        ("reservations", LENDING),
        ("switches", common::SWITCHES),
        ("undecided", LENDING),
    ];
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let on_disk = std::fs::read_dir(&directory)
        .expect("the fixtures are listed")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|it| it == "journey"))
        .count();
    assert_eq!(on_disk, sets.len(), "a fixture journey this test does not read");
    for (file, modules) in sets {
        let journeys = parse(&common::read(&directory.join(format!("{file}.journey"))))
            .unwrap_or_else(|error| panic!("{file}: {error}"));
        let (graph, program, _) = common::library(modules);
        for journey in &journeys {
            let notes = check(journey, &journeys, &graph, &program);
            let worded: Vec<&Note> = notes
                .iter()
                .filter(|note| {
                    ["no state", "is nobody in this journey", "is cast by this journey"]
                        .iter()
                        .any(|said| note.message.contains(said))
                })
                .collect();
            assert!(worded.is_empty(), "{file}/{}: {worded:#?}", journey.name);
            let remarks: Vec<&str> = notes
                .iter()
                .filter(|note| note.verdict == Verdict::Remark)
                .map(|note| note.message.as_str())
                .collect();
            let expected = *file == "chains" && journey.name == "ChainedByName";
            assert!(
                remarks.is_empty() || expected,
                "{file}/{}: an unexpected remark: {remarks:#?}",
                journey.name
            );
            if expected {
                assert!(!remarks.is_empty(), "the chains journey exists to show one");
            }
        }
    }
}

// --- what the walk does with one ----------------------------------------------

/// A word the checker reports is not laid out. Held, `copy.status = availble`
/// had `BorrowCopy` refused on `copy.status = available` — the spec saying no
/// on the strength of a state it has never heard of. Left unset, the act is
/// undecided and names the field, and the line itself says why.
#[test]
fn a_reported_word_in_given_is_left_unset_rather_than_compared() {
    let walk = walked(&journey("        copy.status = availble", ""));
    let given = walk
        .notes
        .iter()
        .find(|note| note.about == "given copy.status = availble")
        .unwrap_or_else(|| panic!("a note on the given line: {:#?}", walk.notes));
    assert_eq!(given.verdict, Verdict::Unspecified);
    let act = &walk.steps[0].outcomes[0];
    assert_eq!(act.verdict, Verdict::Undecided, "{act:#?}");
}

/// The same for one field of an inline instance, and only that field: the
/// instance is still made, and its other fields still written.
#[test]
fn a_reported_word_in_an_inline_instance_leaves_only_that_field_unset() {
    let walk = walked(&journey(
        "        copy.status = available\n        res: Reservation { member: ada, status: wating }",
        "        then res.member = ada\n        then res.status = waiting",
    ));
    assert_eq!(line(&walk, "then res.member = ada").0, Verdict::Specified);
    assert_eq!(line(&walk, "then res.status = waiting").0, Verdict::Undecided);
}

/// A state the spec declares — for another field — is left unset in a
/// `given` the same as a misspelt one. Laid out, `copy.status = open` had
/// `BorrowCopy` refused on a state `Copy.status` never declares.
#[test]
fn another_fields_state_in_given_is_left_unset() {
    let walk = walked(&journey("        copy.status = open", ""));
    assert_eq!(walk.steps[0].outcomes[0].verdict, Verdict::Undecided);
}

/// And in one field of an inline instance: `open` is a loan's, not a
/// reservation's.
#[test]
fn another_fields_state_in_an_inline_instance_is_left_unset() {
    let walk = walked(&journey(
        "        copy.status = available\n        res: Reservation { member: ada, status: open }",
        "        then res.status = waiting",
    ));
    assert_eq!(line(&walk, "then res.status = waiting").0, Verdict::Undecided);
}

/// The needle of an `in` is a word like any other.
#[test]
fn a_misspelt_needle_is_unspecified() {
    let source =
        journey("        copy.status = available", "        then opne in {open, returned}");
    assert_eq!(about(&checked(&source), "opne").verdict, Verdict::Unspecified);
    let walk = walked(&source);
    assert_eq!(line(&walk, "then opne in {open, returned}").0, Verdict::Unspecified);
}

/// A field may name an instance given on a later line. The checker reads the
/// names a journey gives in any order, and the walk does too: it used to
/// refuse `copy: spare` as a word no state spells and leave the field unset,
/// with nothing on the line to say why.
#[test]
fn an_instance_given_further_down_is_somebody() {
    let source = journey(
        "        copy.status = available
        old: Loan { member: ada, status: returned, copy: spare }
        spare: catalogue/Copy { status: available }",
        "        then old.copy = spare",
    );
    assert!(checked(&source).is_empty(), "{:#?}", checked(&source));
    let walk = walked(&source);
    assert_eq!(line(&walk, "then old.copy = spare").0, Verdict::Specified, "{:#?}", walk.steps);
}

// --- two modules naming a type alike -------------------------------------------

fn queued(source: &str) -> (Vec<Note>, Walk) {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(common::QUEUE);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    (notes, walk(&journeys[0], &journeys, &graph, &program, &sources))
}

/// `drafts` and `queue` each declare an `enum Status`. An issue's is the
/// queue's — `open | closed` — and not the first `Status` in the set. Read as
/// the drafts', a correct `given` was reported unspecified and left unset, and
/// a step that holds came back undecided.
#[test]
fn a_fields_enumeration_is_the_one_its_own_module_declares() {
    let (notes, walk) = queued(
        "journey J {
    cast:
        it: Issue
    given:
        it.status = open
    1. it closes
        it does Close(it) on Desk
        then it.status = closed
}",
    );
    assert!(notes.is_empty(), "{notes:#?}");
    assert!(walk.steps[0].verdict().holds(), "{:#?}", walk.steps);
    // And a misspelling is still one, against the right states.
    let (notes, _) = queued(
        "journey J {
    cast:
        it: Issue
    given:
        it.status = opne
    1. it closes
        it does Close(it) on Desk
}",
    );
    assert!(about(&notes, "opne").message.contains("which declares open, closed"), "{notes:#?}");
}

/// Both modules declare an `Item`. The journey says which; the walk reads the
/// field from what it said, as the checker does, rather than from the first
/// `Item` in the set — which declares no `open`, and left the field unset with
/// no note saying why.
#[test]
fn the_walk_reads_a_field_from_the_type_the_journey_wrote() {
    let (notes, walk) = queued(
        "journey J {
    cast:
        it:   Issue
        item: queue/Item
    given:
        item.stage = open
    1. it is finished
        it does Finish(item) on Desk
        then item.done = true
}",
    );
    assert!(notes.is_empty(), "{notes:#?}");
    assert!(walk.steps[0].verdict().holds(), "{:#?}", walk.steps);
}

// --- the checker and the walk give one answer -----------------------------------

/// A set with a misspelt member in it is reported by the checker, member by
/// member — and the walk, asking the same question of the whole set, leaves
/// the field unset rather than laying the set out with the bare word inside
/// it. It was laid out, and `then ada in hold.reviewers` *held* beside the
/// note saying the line was wrong.
#[test]
fn a_set_with_a_reported_member_is_left_unset_and_said_once() {
    let source = "journey J {
    cast:
        ada:  lending/Member
        bob:  lending/Member
        hold: Hold
    given:
        hold.reviewers = {ada, bbo}
    1. look
        then ada in hold.reviewers
}";
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(common::DESK);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    assert_eq!(about(&notes, "bbo").verdict, Verdict::Unspecified);
    let walk = walk(&journeys[0], &journeys, &graph, &program, &sources);
    let (verdict, detail) = line(&walk, "then ada in hold.reviewers");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let said: Vec<_> = walk.notes.iter().filter(|note| note.about.contains("reviewers")).collect();
    assert_eq!(said.len(), 1, "one note on the line, not one from each side: {said:#?}");
}

/// The file's world is laid out before the journey's cast, so a world line
/// naming somebody only the journey casts cannot be written. The checker read
/// world lines with the journey's names and said nothing; the walk then wrote
/// nothing and said nothing either, and `then loan.member = ada` read unknown
/// with no word about why.
#[test]
fn a_world_line_naming_somebody_only_the_journey_casts_is_reported() {
    let source = "world {
    cast:
        loan: Loan
    given:
        loan.status = open
        loan.member = ada
}

journey J {
    cast:
        ada: Member
    1. look
        then loan.member = ada
}";
    let notes = checked(source);
    let note = about(&notes, "ada");
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(
        note.message.contains("is cast by this journey, and the file's world is laid out before"),
        "{}",
        note.message
    );
    let walk = walked(source);
    let said: Vec<_> =
        walk.notes.iter().filter(|note| note.about == "given loan.member = ada").collect();
    assert_eq!(said.len(), 1, "{:#?}", walk.notes);
    assert_eq!(said[0].detail.as_deref(), Some(note.message.as_str()));
}

/// Where the checker has nothing to say about a line the walk did not write,
/// the walk says it. A journey standing on one whose catching step was
/// refused names what it would have caught: the checker counts the name, and
/// the walk has nothing bound to it.
#[test]
fn a_given_the_walk_could_not_write_is_said_even_where_the_checker_was_silent() {
    let source = "journey TriesToBorrow {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        ada.is_at_limit = false
        copy.status = lost
    1. she tries
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
}

journey NoticesIt {
    after: TriesToBorrow
    given:
        n: Notice { loan: loan }
    1. look
        then n.loan = loan
}";
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(LENDING);
    assert!(check(&journeys[1], &journeys, &graph, &program).is_empty());
    let walk = walk(&journeys[1], &journeys, &graph, &program, &sources);
    let note = walk
        .notes
        .iter()
        .find(|note| note.about == "given n: Notice")
        .unwrap_or_else(|| panic!("the line is reported: {:#?}", walk.notes));
    assert_eq!(note.verdict, Verdict::Unspecified);
    assert!(note.detail.as_deref().is_some_and(|detail| detail.contains("`loan`")), "{note:#?}");
}

/// A bare word an act hands on that no rule compares is `check`'s remark, and
/// the walk carries it beside the step — the browser shows walks, so a remark
/// only `check` printed was one nobody there saw. Beside the step and not in
/// it: the remark is no part of whether the step holds.
#[test]
fn an_act_s_remark_is_carried_into_the_walk_beside_the_step() {
    let source = "journey J {
    cast:
        ada:  Member
        loan: Loan
    given:
        loan.member = ada
        loan.status = open
    1. report
        ada does MemberReportsLoss(loan, mislaid) on MemberShelf
}";
    let notes = checked(source);
    assert_eq!(about(&notes, "mislaid").verdict, Verdict::Remark);
    let walk = walked(source);
    let step = &walk.steps[0];
    assert_eq!(step.asides.len(), 1, "{step:#?}");
    assert_eq!(step.asides[0].verdict, Verdict::Remark);
    assert_eq!(step.asides[0].about, "ada does MemberReportsLoss(loan, mislaid) on MemberShelf");
    assert!(
        step.asides[0].detail.as_deref().is_some_and(|detail| detail.contains("`mislaid`")),
        "{step:#?}"
    );
    assert!(step.outcomes.iter().all(|outcome| outcome.verdict != Verdict::Remark), "{step:#?}");
}

/// An aside is a remark on that line, and nothing else: another step's
/// remark is not this step's, and a line `check` could not support is the
/// step's verdict rather than something said beside it.
#[test]
fn only_a_remark_on_the_step_s_own_line_is_an_aside() {
    let source = "journey J {
    cast:
        ada:  Member
        loan: Loan
    given:
        loan.member = ada
        loan.status = open
    1. report
        ada does MemberReportsLoss(loan, mislaid) on MemberShelf
    2. look
        then loan.status = opne
}";
    let notes = checked(source);
    assert_eq!(about(&notes, "mislaid").verdict, Verdict::Remark);
    assert_ne!(about(&notes, "opne").verdict, Verdict::Remark);
    let walk = walked(source);
    assert_eq!(walk.steps[0].asides.len(), 1, "{:#?}", walk.steps[0]);
    let second = &walk.steps[1];
    assert!(second.asides.is_empty(), "{second:#?}");
    assert!(second.outcomes.iter().any(|outcome| outcome.about == "then loan.status = opne"));
}

/// Both modules declare an `Item`, and an issue's `item: Item` is its own
/// module's. Followed by name alone, the hop landed on the first `Item` in the
/// set — the drafts', which declares no `open` — and a correct `then` was
/// reported unspecified and never compared.
#[test]
fn a_hop_through_a_field_is_read_in_the_module_declaring_it() {
    let (notes, walk) = queued(
        "journey J {
    cast:
        bo: Issue
        it: queue/Item
    given:
        bo.status = open
        bo.item = it
        it.stage = open
    1. it closes
        bo does Close(bo) on Desk
        then bo.item.stage = open
        then bo.status = closed
}",
    );
    assert!(notes.is_empty(), "{notes:#?}");
    assert!(walk.notes.is_empty(), "{:#?}", walk.notes);
    assert!(walk.steps[0].verdict().holds(), "{:#?}", walk.steps);
    // And a state of the other `Item` is reported against this one's.
    let (notes, _) = queued(
        "journey J {
    cast:
        bo: Issue
    1. it closes
        then bo.item.stage = draft
}",
    );
    assert!(
        about(&notes, "draft").message.contains("no state of `Item.stage`, which declares open"),
        "{notes:#?}"
    );
}
