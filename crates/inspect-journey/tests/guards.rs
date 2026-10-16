//! Walking journeys against the guards fixture: exposures a surface shows only
//! while their `when` guard holds (issue #8).
//!
//! `doc.secret when doc.is_private = false` is how the language says a field is
//! conditionally visible. The walk used to read a guarded item as never shown,
//! whatever the world said: `sees` came back unexposed, and `cannot see` held
//! without anything being checked — the worst answer the tool can give.
//!
//! The guard is now read against the world, the same way a `for … where`
//! filter is. True shows the item, false hides it, and a guard nothing could
//! settle leaves both directions undecided, with the guard named.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::GUARDS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked_nth(source: &str, at: usize) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(GUARDS);
    walk(&journeys[at], &journeys, &graph, &program, &sources)
}

/// The outcome of the line that reads `about`.
fn line(walk: &Walk, about: &str) -> (Verdict, String) {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .map(|outcome| (outcome.verdict, outcome.detail.clone().unwrap_or_default()))
        .unwrap_or_else(|| panic!("no line `{about}` in {walk:#?}"))
}

/// One step of `clauses`, over a shelf and three documents on it: one public,
/// one private, and one whose privacy nobody said. `open` and `unsaid` belong to
/// the same owner, `shut` to another.
fn journey(clauses: &str) -> String {
    on(
        "        s.name = \"s\"
        open.shelf = s
        open.owner = \"a\"
        open.is_private = false
        shut.shelf = s
        shut.owner = \"b\"
        shut.is_private = true
        unsaid.shelf = s
        unsaid.owner = \"a\"",
        clauses,
    )
}

/// One step of `clauses` over the same cast, with only `given` said about it.
fn on(given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
        s: Shelf
        open: Doc
        shut: Doc
        unsaid: Doc
    given:
{given}
    1. the step
{clauses}
}}"
    )
}

fn walked(clauses: &str) -> Walk {
    walked_nth(&journey(clauses), 0)
}

const FILED: &str = include_str!("fixtures/guards.journey");

/// The first journey of the issue: the guard holds, so the field is shown.
#[test]
fn the_issue_as_filed_shows_a_field_whose_guard_holds() {
    let result = walked_nth(FILED, 0);
    let (verdict, detail) = line(&result, "d sees d.secret on DocPage in d");
    assert_eq!(verdict, Verdict::Specified, "{detail}");
    assert_eq!(result.verdict(), Verdict::Specified);
}

/// The second, and the dangerous one: `cannot see` used to hold here without
/// anything being checked. The guard holds, so the surface does show it.
#[test]
fn the_issue_as_filed_refuses_cannot_see_when_the_guard_holds() {
    let result = walked_nth(FILED, 1);
    let (verdict, detail) = line(&result, "d cannot see d.secret on DocPage in d");
    assert_eq!(verdict, Verdict::Refused, "{detail}");
    assert!(detail.contains("does show"), "{detail}");
}

/// A guard that is false hides the field, and says so in both directions.
#[test]
fn a_false_guard_hides_the_field() {
    let result = walked(
        "        open sees shut.secret on DocPage in shut
        open cannot see shut.secret on DocPage in shut",
    );
    let (seen, why) = line(&result, "open sees shut.secret on DocPage in shut");
    assert_eq!(seen, Verdict::Unexposed, "{why}");
    let (unseen, why) = line(&result, "open cannot see shut.secret on DocPage in shut");
    assert_eq!(unseen, Verdict::Specified, "{why}");
}

/// Stipulation 1: a guard nothing set cannot be read, and neither direction
/// may be answered — least of all `cannot see`. The reason names the guard as
/// the author wrote it.
#[test]
fn a_guard_nothing_settled_leaves_both_directions_undecided() {
    let result = walked(
        "        open sees unsaid.secret on DocPage in unsaid
        open cannot see unsaid.secret on DocPage in unsaid",
    );
    for about in [
        "open sees unsaid.secret on DocPage in unsaid",
        "open cannot see unsaid.secret on DocPage in unsaid",
    ] {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Undecided, "{about}: {why}");
        assert!(why.contains("`when doc.is_private = false`"), "{about}: {why}");
    }
}

/// The guard does not reach beyond the item it is written on: the unguarded
/// field beside it is shown whatever the guard says.
#[test]
fn an_unguarded_field_beside_a_guarded_one_is_shown_regardless() {
    let result = walked("        open sees shut.owner on DocPage in shut");
    let (verdict, why) = line(&result, "open sees shut.owner on DocPage in shut");
    assert_eq!(verdict, Verdict::Specified, "{why}");
}

/// Inside a `for`, the guard is about each element in turn: one shelf, three
/// documents, three different answers.
#[test]
fn a_guard_in_a_for_body_is_read_for_each_element() {
    let result = walked(
        "        open sees open.secret on ShelfPage in s
        open cannot see shut.secret on ShelfPage in s
        open cannot see open.secret on ShelfPage in s
        open cannot see unsaid.secret on ShelfPage in s",
    );
    let (verdict, why) = line(&result, "open sees open.secret on ShelfPage in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "open cannot see shut.secret on ShelfPage in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "open cannot see open.secret on ShelfPage in s");
    assert_eq!(verdict, Verdict::Refused, "{why}");
    let (verdict, why) = line(&result, "open cannot see unsaid.secret on ShelfPage in s");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("`when doc.is_private = false`"), "{why}");
}

/// Hidden by a guard, the reader is told which guard: "nothing shows it" sends
/// an author looking at the surface, when the answer is in their `given:`.
#[test]
fn a_false_guard_says_which_guard_hid_the_field() {
    let result = walked(
        "        open sees shut.secret on DocPage in shut
        open sees shut.secret on ShelfPage in s",
    );
    for about in
        ["open sees shut.secret on DocPage in shut", "open sees shut.secret on ShelfPage in s"]
    {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Unexposed, "{about}: {why}");
        assert!(why.contains("`when doc.is_private = false`"), "{about}: {why}");
        assert!(why.contains("does not hold"), "{about}: {why}");
    }
}

/// A guard computed over a filtered set that dropped a member nothing could
/// decide is not settled, however definite the count looks: `unsaid` may be
/// the private document that would make it true. Stipulation 1, reached
/// through a guard rather than a field.
#[test]
fn a_guard_over_a_set_that_dropped_an_undecided_member_is_undecided() {
    let given = "        s.name = \"s\"
        open.shelf = s
        open.is_private = false
        shut.shelf = s
        shut.is_private = false
        unsaid.shelf = s";
    let asked = "        open sees s.name on TallyPage in s
        open cannot see s.name on TallyPage in s";
    let result = walked_nth(&on(given, asked), 0);
    for about in ["open sees s.name on TallyPage in s", "open cannot see s.name on TallyPage in s"]
    {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Undecided, "{about}: {why}");
        assert!(why.contains("`when (shelf.docs where is_private = true).count > 0`"), "{why}");
    }

    // The other direction: once it is said, the same guard settles.
    let said = format!("{given}\n        unsaid.is_private = true");
    let result = walked_nth(&on(&said, asked), 0);
    let (verdict, why) = line(&result, "open sees s.name on TallyPage in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
}

/// The same for a `for … where`: a filter computed over a set that dropped
/// what it could not decide does not admit or refuse anything. `open`'s own
/// privacy is left unsaid as well, because the element's fields are in scope
/// bare and an outer `is_private` would otherwise be read for `unsaid`'s.
#[test]
fn a_where_over_a_set_that_dropped_an_undecided_member_is_undecided() {
    let given = "        open.shelf = s
        shut.shelf = s
        shut.is_private = false
        unsaid.shelf = s";
    let asked = "        open cannot see open.secret on QuietShelf in s";
    let result = walked_nth(&on(given, asked), 0);
    let (verdict, why) = line(&result, "open cannot see open.secret on QuietShelf in s");
    assert_eq!(verdict, Verdict::Undecided, "{why}");

    let said =
        format!("{given}\n        open.is_private = false\n        unsaid.is_private = false");
    let result = walked_nth(&on(&said, asked), 0);
    let (verdict, why) = line(&result, "open cannot see open.secret on QuietShelf in s");
    assert_eq!(verdict, Verdict::Refused, "{why}");
}

/// A guard that is not a condition is neither shown nor hidden — treating a
/// string as false would make `cannot see` hold on nothing.
#[test]
fn a_guard_that_is_not_true_or_false_is_undecided_both_ways() {
    let result = walked(
        "        open sees open.secret on OddPage in open
        open cannot see open.secret on OddPage in open",
    );
    for about in [
        "open sees open.secret on OddPage in open",
        "open cannot see open.secret on OddPage in open",
    ] {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Undecided, "{about}: {why}");
        assert!(why.contains("`when doc.owner`"), "{about}: {why}");
        assert!(why.contains("came back"), "{about}: {why}");
    }
}

/// A guard about who is looking: the same item, two viewers, two answers.
#[test]
fn a_guard_about_the_viewer_answers_each_viewer_differently() {
    let result = walked(
        "        open sees unsaid.secret on MinePage in unsaid
        shut cannot see unsaid.secret on MinePage in unsaid
        shut sees unsaid.secret on MinePage in unsaid",
    );
    let (verdict, why) = line(&result, "open sees unsaid.secret on MinePage in unsaid");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "shut cannot see unsaid.secret on MinePage in unsaid");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "shut sees unsaid.secret on MinePage in unsaid");
    assert_eq!(verdict, Verdict::Unexposed, "{why}");
}

/// In a `for` body of several lines the guard reaches only its own line.
#[test]
fn a_guard_in_a_for_body_leaves_the_line_beside_it_alone() {
    let result = walked(
        "        open cannot see shut.owner on ShelfPage in s
        open cannot see shut.secret on ShelfPage in s",
    );
    let (verdict, why) = line(&result, "open cannot see shut.owner on ShelfPage in s");
    assert_eq!(verdict, Verdict::Refused, "{why}");
    let (verdict, why) = line(&result, "open cannot see shut.secret on ShelfPage in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
}

/// A `where` and a guard on the same iteration are two gates, and an element
/// has to pass both.
#[test]
fn a_guard_beside_a_where_needs_both_to_admit() {
    let result = walked(
        "        open sees open.secret on FilteredShelf in s
        open cannot see shut.secret on FilteredShelf in s
        open cannot see unsaid.secret on FilteredShelf in s",
    );
    let (verdict, why) = line(&result, "open sees open.secret on FilteredShelf in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "open cannot see shut.secret on FilteredShelf in s");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    let (verdict, why) = line(&result, "open cannot see unsaid.secret on FilteredShelf in s");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
}
