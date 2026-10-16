//! A rule that removes an instance and reads one of its fields in another
//! `ensures` (issue #33), walked end to end.
//!
//! `ensures` clauses are a set of postconditions about the state the rule
//! leaves, so the order they are written in cannot change a verdict. Before,
//! `not exists upload` above `Blob.created(name: upload.name)` was undecided
//! and the same two lines swapped held. Both are undecided now, saying which
//! clause removed the upload and how to keep its name; the `let` that keeps it
//! holds.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::UPLOADS;
use inspect_journey::{Outcome, Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/uploads.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(UPLOADS);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// The line about `about`, and what it said.
fn line<'a>(walk: &'a Walk, about: &str) -> &'a Outcome {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .unwrap_or_else(|| panic!("no line `{about}`: {walk:#?}"))
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = walk
        .steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .filter(|outcome| outcome.verdict != Verdict::Specified)
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
}

/// The act line: the rule fired, and says what it ensures that the world
/// does not show — the removal, and how to keep the name.
fn act_says_it<'a>(walk: &'a Walk, rule: &str) -> &'a str {
    let act = walk
        .steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about.starts_with(&format!("up does {rule}(")))
        .unwrap_or_else(|| panic!("no act for {rule}: {walk:#?}"));
    assert_eq!(act.verdict, Verdict::Undecided, "{act:#?}");
    let detail = act.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains(&format!("`{rule}` fired, but part of what it ensures was not applied"))
            && removal_said(detail, rule),
        "{detail}"
    );
    detail
}

/// Whether `detail` says `rule` removed the upload, by which clause, that the
/// order does not matter, and the `let` that keeps the name.
fn removal_said(detail: &str, rule: &str) -> bool {
    detail.contains(&format!("rule `{rule}` removes `upload` (`up`) with `not exists upload`"))
        && detail.contains("whichever line comes first")
        && detail.contains("bind `let name = upload.name` beside the `when`")
}

/// Undecided, saying which rule removed the upload, by which clause, that the
/// order does not matter, and the `let` that keeps the name — on the line
/// that reads it and on the act. Returned with the rule's name taken out, so
/// two orders can be compared.
fn read_after_removal(walk: &Walk, about: &str, rule: &str) -> String {
    let said = line(walk, about);
    assert_eq!(said.verdict, Verdict::Undecided, "{said:#?}");
    let detail = said.detail.clone().unwrap_or_default();
    assert!(removal_said(&detail, rule), "{detail}");
    assert!(!detail.contains("is not in this world"), "{detail}");
    act_says_it(walk, rule);
    detail.replace(rule, "_")
}

/// The issue as it was filed: removed, then read.
#[test]
fn a_field_read_after_the_rule_removes_its_instance_is_undecided_and_says_how_to_keep_it() {
    read_after_removal(&walked("ReadAfterRemove"), "then b.name = \"x\"", "CompleteRemovingFirst");
}

/// The same two lines swapped: the same verdict, in the same words. This
/// walk held before, which is the order-sensitivity the issue reports.
#[test]
fn the_same_read_written_above_the_removal_gets_the_same_verdict() {
    let after = read_after_removal(
        &walked("ReadAfterRemove"),
        "then b.name = \"x\"",
        "CompleteRemovingFirst",
    );
    let before = read_after_removal(
        &walked("ReadBeforeRemove"),
        "then b.name = \"x\"",
        "CompleteReadingFirst",
    );
    assert_eq!(after, before);
}

/// The fix the reason points at: a `let` beside the `when`.
#[test]
fn a_name_kept_in_a_let_before_the_rule_holds() {
    all_held(&walked("TheNameKeptBeforeTheRule"));
}

/// An assigned value reads the state the rule leaves, as a creation's does.
#[test]
fn an_assigned_value_read_from_the_removed_upload_is_undecided() {
    let walk = walked("AnArchivedNameIsReadAfterTheUploadIsGone");
    read_after_removal(&walk, "then sh.last = \"x\"", "Archive");
}

/// A condition deciding whether to remove reads the upload as the rule found
/// it: a small one stays and has its name.
#[test]
fn a_small_upload_stays_and_has_its_name() {
    all_held(&walked("ASmallUploadStaysAndHasItsName"));
}

/// And a big one goes, decided — nothing says the `if` was skipped — taking
/// its name with it.
#[test]
fn a_big_upload_goes_and_its_name_is_not_there_to_read() {
    let walk = walked("ABigUploadGoesAndTakesItsName");
    read_after_removal(&walk, "then b.name = \"x\"", "Tidy");
    assert!(!format!("{walk:?}").contains("was skipped"), "{walk:#?}");
}

/// An emission's argument read from what the rule removes reaches the
/// listener as nothing settled. The line reading what the listener made
/// cannot say why — the listener was handed only the value — so the act that
/// emitted it does: the removal, by which clause, and the `let`.
#[test]
fn an_emitted_argument_read_from_the_removed_upload_is_explained_on_the_act() {
    let walk = walked("ADiscardedNameReachesNobody");
    assert_eq!(line(&walk, "then b.name = \"x\"").verdict, Verdict::Undecided, "{walk:#?}");
    act_says_it(&walk, "Discard");
}

/// A removal decided through a `let` inside `ensures`: the same verdict, in
/// the same words, whichever line comes first.
#[test]
fn a_removal_decided_through_a_let_inside_ensures_gets_the_same_verdict_in_either_order() {
    let first = read_after_removal(
        &walked("ABigUploadGoesByLetReadingFirst"),
        "then b.name = \"x\"",
        "TidyByLetReadingFirst",
    );
    let last = read_after_removal(
        &walked("ABigUploadGoesByLetReadingLast"),
        "then b.name = \"x\"",
        "TidyByLetReadingLast",
    );
    assert_eq!(first, last);
}

/// The thumbs' names through a relationship, in the state the rule leaves:
/// none are left, so a name is refused — the same in either order. Read
/// first, it was undecided; read after, refused.
#[test]
fn a_relationship_read_after_its_members_go_gets_the_same_verdict_in_either_order() {
    for journey in ["NamesReadAfterTheThumbsGo", "NamesReadBeforeTheThumbsGo"] {
        let walk = walked(journey);
        let said = line(&walk, "then b.name = \"a\"");
        assert_eq!(said.verdict, Verdict::Refused, "{journey}: {said:#?}");
        assert_eq!(said.detail.as_deref(), Some("b.name is {}"), "{journey}");
        assert_eq!(line(&walk, "then th does not exist").verdict, Verdict::Specified);
    }
}
