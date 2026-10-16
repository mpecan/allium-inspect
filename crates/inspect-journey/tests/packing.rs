//! Walking journeys against the packing fixture: a set asked about through a
//! lambda.
//!
//! `box.items.all(i => i.kind = a)` and `box.items.any(i => i.kind = b)` are
//! the whole of what the fixture is for. Issue #7 reported both as undecided
//! however completely a journey had said what was in the set — "`all` was
//! called on a collection, which defines nothing" — which sent authors to
//! rewrite real logic as black-box predicates a journey then had to stipulate.
//!
//! Kleene throughout: one member that settles the answer settles it, and a
//! member nobody described leaves it open only when nothing else settled it.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::PACKING;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(PACKING);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

/// Every outcome, flattened, with the text it was written as.
fn outcomes(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

/// The outcome of the line that reads `about`.
fn line(walk: &Walk, about: &str) -> (Verdict, Option<String>) {
    outcomes(walk)
        .into_iter()
        .find(|(_, written, _)| written == about)
        .map(|(verdict, _, detail)| (verdict, detail))
        .unwrap_or_else(|| panic!("no line `{about}` in {:#?}", outcomes(walk)))
}

/// A box holding `items`, each item's kind given (or not), then one act.
fn packing(kinds: &[(&str, Option<&str>)], items: &str, act: &str, then: &str) -> String {
    let cast: String = kinds.iter().map(|(name, _)| format!("        {name}: Item\n")).collect();
    let given: String = kinds
        .iter()
        .filter_map(|(name, kind)| kind.map(|kind| format!("        {name}.kind = {kind}\n")))
        .collect();
    format!(
        "journey J {{
    cast:
        box: Box
{cast}    given:
{given}        box.sealed = false
        box.flagged = false
    1. pack it
        stipulate box.items = {items}
        box does {act}(box) on Packing
        then {then}
}}"
    )
}

#[test]
fn the_issue_as_filed_holds() {
    // The reproduction from #7, word for word.
    let result = walked(include_str!("fixtures/packing.journey"));
    let every: Vec<_> = outcomes(&result);
    assert!(
        every.iter().all(|(verdict, _, _)| *verdict == Verdict::Specified),
        "every line holds: {every:#?}"
    );
    assert_eq!(line(&result, "box does Seal(box) on Packing").0, Verdict::Specified);
    assert_eq!(line(&result, "then box.sealed = true").0, Verdict::Specified);
    assert_eq!(result.verdict(), Verdict::Specified);
}

#[test]
fn all_is_refused_by_one_member_that_fails_it() {
    let result = walked(&packing(
        &[("x", Some("a")), ("y", Some("b"))],
        "{x, y}",
        "Seal",
        "box.sealed = false",
    ));
    let (verdict, detail) = line(&result, "box does Seal(box) on Packing");
    assert_eq!(verdict, Verdict::Refused, "{detail:?}");
    let why = detail.expect("a reason");
    assert!(why.contains("box.items.all(i => i.kind = a)"), "in the spec's own words: {why}");
}

#[test]
fn all_over_an_empty_set_holds() {
    // Vacuously: a box with nothing in it has nothing of the wrong kind.
    let result = walked(&packing(&[], "{}", "Seal", "box.sealed = true"));
    assert_eq!(line(&result, "box does Seal(box) on Packing").0, Verdict::Specified);
    assert_eq!(line(&result, "then box.sealed = true").0, Verdict::Specified);
}

#[test]
fn any_holds_on_the_one_member_that_satisfies_it() {
    let result = walked(&packing(
        &[("x", Some("a")), ("y", Some("b"))],
        "{x, y}",
        "Flag",
        "box.flagged = true",
    ));
    assert_eq!(line(&result, "box does Flag(box) on Packing").0, Verdict::Specified);
    assert_eq!(line(&result, "then box.flagged = true").0, Verdict::Specified);
}

#[test]
fn any_is_refused_when_no_member_satisfies_it() {
    let result = walked(&packing(&[("x", Some("a"))], "{x}", "Flag", "box.flagged = false"));
    let (verdict, detail) = line(&result, "box does Flag(box) on Packing");
    assert_eq!(verdict, Verdict::Refused, "{detail:?}");
    assert!(detail.expect("a reason").contains("box.items.any(i => i.kind = b)"));
}

#[test]
fn any_over_an_empty_set_is_refused() {
    let result = walked(&packing(&[], "{}", "Flag", "box.flagged = false"));
    assert_eq!(line(&result, "box does Flag(box) on Packing").0, Verdict::Refused);
}

#[test]
fn a_member_nobody_described_leaves_all_undecided_and_names_it() {
    // `y` is in the box and nothing said what kind it is. `x` passes, so the
    // answer turns on `y` alone — and the reason has to say that, rather than
    // the simulator choosing a kind for it.
    let result =
        walked(&packing(&[("x", Some("a")), ("y", None)], "{x, y}", "Seal", "box.sealed = true"));
    let (verdict, detail) = line(&result, "box does Seal(box) on Packing");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let why = detail.expect("a reason");
    assert!(why.contains("has no `kind` set"), "names the field nobody stated: {why}");
    assert!(!why.contains("defines nothing"), "{why}");
    assert!(!why.contains("the predicate could not be decided"), "said once, specifically: {why}");
}

#[test]
fn a_member_that_settles_it_settles_it_whatever_the_rest_are() {
    // Kleene: `y` is of kind b, so not every item is of kind a — whatever `z`
    // turns out to be. Undecided here would be the simulator failing to reach
    // a conclusion the spec plainly supports.
    let result =
        walked(&packing(&[("y", Some("b")), ("z", None)], "{y, z}", "Seal", "box.sealed = false"));
    assert_eq!(line(&result, "box does Seal(box) on Packing").0, Verdict::Refused);

    let result =
        walked(&packing(&[("y", Some("b")), ("z", None)], "{y, z}", "Flag", "box.flagged = true"));
    assert_eq!(line(&result, "box does Flag(box) on Packing").0, Verdict::Specified);
}

/// A crate and two parcels, `p` placed in it and of kind a, `q` of kind `q_kind`
/// and placed wherever `q_crate` says — or nowhere anyone stated.
fn loading(q_kind: &str, q_crate: Option<&str>, then: &str) -> String {
    let placed = q_crate.map(|at| format!("        q.crate = {at}\n")).unwrap_or_default();
    format!(
        "journey J {{
    cast:
        crate: Crate
        elsewhere: Crate
        p: Parcel
        q: Parcel
    given:
        p.crate = crate
        p.kind = a
        q.kind = {q_kind}
{placed}        crate.sealed = false
    1. seal the crate
        crate does SealCrate(crate) on Loading
        then {then}
}}"
    )
}

#[test]
fn all_over_a_relationship_holds_when_every_member_it_holds_passes() {
    // `parcels: Parcel with crate = this` holds `p` alone: `q` is in another
    // crate.
    let result = walked(&loading("b", Some("elsewhere"), "crate.sealed = true"));
    let (verdict, detail) = line(&result, "crate does SealCrate(crate) on Loading");
    assert_eq!(verdict, Verdict::Specified, "{detail:?}");
}

#[test]
fn all_over_a_relationship_is_refused_by_a_member_of_the_wrong_kind() {
    let result = walked(&loading("b", Some("crate"), "crate.sealed = false"));
    let (verdict, detail) = line(&result, "crate does SealCrate(crate) on Loading");
    assert_eq!(verdict, Verdict::Refused, "{detail:?}");
}

#[test]
fn a_parcel_nobody_placed_leaves_all_over_the_relationship_undecided() {
    // `q` is of kind b and nothing says which crate it is in. It may be in this
    // one, so "every parcel is of kind a" is not something the spec supports —
    // and the rule's parameter being called `crate`, like the field, must not
    // make it look as though `q` were placed here either.
    let result = walked(&loading("b", None, "crate.sealed = true"));
    let (verdict, detail) = line(&result, "crate does SealCrate(crate) on Loading");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let why = detail.expect("a reason");
    assert!(why.contains("`crate`"), "names the field nobody stated: {why}");
}

// --- a relationship that may be missing a parcel -----------------------------

/// A crate, and parcels each given a kind and, or not, a crate.
fn shipping(parcels: &[(&str, &str, bool)], clauses: &str) -> String {
    let cast: String =
        parcels.iter().map(|(name, ..)| format!("        {name}: Parcel\n")).collect();
    let given: String = parcels
        .iter()
        .map(|(name, kind, placed)| {
            let placed =
                if *placed { format!("        {name}.crate = c\n") } else { String::new() };
            format!("        {name}.kind = {kind}\n        {name}.stamped = false\n{placed}")
        })
        .collect();
    format!(
        "journey J {{
    cast:
        c: Crate
{cast}    given:
        c.shipped = false
        c.retired = false
{given}    1. the step
{clauses}
}}"
    )
}

/// `for p in crate.parcels: ensures: Packed(parcel: p)`, with a parcel whose
/// `crate` nobody set. It may be in the crate, so the iteration over the rest
/// is not the whole of what `Ship` does — and `Stamp` not firing for it, and
/// it staying unstamped, were conclusions nobody reached.
#[test]
fn an_iteration_over_a_relationship_that_may_be_missing_a_member_is_not_applied_in_full() {
    let result = walked(&shipping(
        &[("p", "a", false)],
        "        c does Ship(c) on Loading
        then Stamp does not fire
        then p.stamped = false",
    ));
    let (verdict, why) = line(&result, "c does Ship(c) on Loading");
    assert_eq!(verdict, Verdict::Undecided, "{why:?}");
    let why = why.expect("it says why");
    assert!(why.contains("`for p in crate.parcels` may be missing members"), "{why}");
    for about in ["then Stamp does not fire", "then p.stamped = false"] {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Undecided, "{about}: {why:?}");
    }
}

/// A refusal on the firings that happened says nothing about a firing that
/// did not. `Stamp` refuses `q`, of kind b; `p` is of kind a and may be in the
/// crate, and if it is, `Stamp` fires for it.
#[test]
fn a_refusal_beside_a_member_never_visited_is_not_a_definite_does_not_fire() {
    let result = walked(&shipping(
        &[("q", "b", true), ("p", "a", false)],
        "        c does Ship(c) on Loading
        then Stamp does not fire",
    ));
    let (verdict, why) = line(&result, "then Stamp does not fire");
    assert_eq!(verdict, Verdict::Undecided, "{why:?}");
    let why = why.expect("it says why");
    assert!(why.starts_with("`Stamp` refused: "), "{why}");
    assert!(why.contains("may be missing members"), "{why}");
}

/// Every parcel placed, and the same lines are answered.
#[test]
fn an_iteration_over_a_complete_relationship_is_answered() {
    let result = walked(&shipping(
        &[("q", "b", true)],
        "        c does Ship(c) on Loading
        then Stamp does not fire
        then q.stamped = false
        then c.shipped = true",
    ));
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// `requires: crate.parcels.count = 0` with a parcel nobody placed. The count
/// is a lower bound, and a crate retired over a parcel that may be in it is
/// the simulator guessing.
#[test]
fn a_count_over_a_relationship_that_may_be_missing_a_member_is_undecided() {
    let result = walked(&shipping(&[("p", "a", false)], "        c does Retire(c) on Loading"));
    let (verdict, why) = line(&result, "c does Retire(c) on Loading");
    assert_eq!(verdict, Verdict::Undecided, "{why:?}");
    assert!(why.expect("it says why").contains("its `count` is not known"));

    // No parcels anywhere: nothing was left out, and the count is zero.
    let result = walked(&shipping(
        &[],
        "        c does Retire(c) on Loading\n        then c.retired = true",
    ));
    let bad: Vec<_> = outcomes(&result)
        .into_iter()
        .filter(|(verdict, ..)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    // And a parcel placed elsewhere is decided not to be in it.
    let result = walked(
        &shipping(&[("p", "a", false)], "        c does Retire(c) on Loading")
            .replace(
                "        p.stamped = false\n",
                "        p.stamped = false\n        p.crate = d\n",
            )
            .replace("        c: Crate\n", "        c: Crate\n        d: Crate\n"),
    );
    assert_eq!(line(&result, "c does Retire(c) on Loading").0, Verdict::Specified);
}
