//! A temporal trigger over an optional timestamp that is `null`, walked
//! (issue #20).
//!
//! `when: task: Task.stopped_at + config.ttl <= now` asks whether a deadline
//! has passed. For a task still running there is no `stopped_at`, so there is
//! no deadline, and nothing is due. It used to come back undecided — `+` is not
//! defined between a null and a duration — and took the whole step with it
//! every time the clock moved and anything else woke a `Task`.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::DEADLINES;
use inspect_journey::{Verdict, Walk, parse, walk};

const AS_FILED: &str = include_str!("fixtures/deadlines.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(AS_FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey {name}"));
    let (graph, program, sources) = common::library(DEADLINES);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// Every outcome, flattened, with the text it was written as.
fn outcomes(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = outcomes(walk)
        .into_iter()
        .filter(|(verdict, _, _)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "step {}", step.number);
    }
}

/// The reproduction from the issue: the clock times a silent task out, which
/// wakes every rule on `Task` — `Expire` among them, over a `stopped_at` that
/// is still `null`. Not due, so not run; and once the task has stopped, an
/// hour later it is.
#[test]
fn a_task_that_has_not_stopped_does_not_expire_while_the_clock_moves() {
    all_held(&walked("ATaskThatHasNotStoppedDoesNotExpire"));
}

/// A bare optional deadline, and one built from two offsets with a strict
/// `<`, on an instance nothing else wakes: neither is due, and `does not fire`
/// is answered from the listings the clock read — every one said *no* — rather
/// than "not visible from here".
#[test]
fn an_invite_with_no_deadlines_is_never_due() {
    all_held(&walked("InvitesWithNoDeadlinesAreNeverDue"));
}

/// The control: the same triggers over timestamps that are set fire when their
/// time comes.
#[test]
fn an_invite_whose_deadlines_are_set_comes_due() {
    all_held(&walked("InvitesWithDeadlinesComeDue"));
}

/// One trigger on an instance comes due and wakes every rule on it — so the
/// other, over a `null`, is read in the firing rather than in the listing, and
/// is not due there either.
#[test]
fn a_null_deadline_woken_beside_a_due_one_is_still_not_due() {
    all_held(&walked("AnInviteSentWithNoExpiryNeverLapses"));
}

/// The opposite question — is the opening still ahead of the clock — is one a
/// `null` does not answer either way. Undecided, and the walk says which rule
/// and which part of its trigger, rather than holding because nothing else
/// happened to wake an `Offer`.
#[test]
fn whether_a_null_is_still_ahead_is_undecided_and_says_why() {
    let walk = walked("AnOfferWithNoOpeningIsNotAnswered");
    let all = outcomes(&walk);
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "after 1.day").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert!(detail.starts_with("while the clock moved, "), "{detail}");
    assert!(detail.contains("`Withdraw` could not be decided"), "{detail}");
    assert!(detail.contains("`Offer.opens_at > now`"), "{detail}");
    assert_eq!(detail.matches("`Withdraw`").count(), 1, "{detail}");
}

/// A `when` nobody could settle beside a `requires` that already refuses is a
/// rule that does not run, and the clock's line has nothing to say about it.
#[test]
fn an_undecided_trigger_whose_requires_refuses_is_not_reported() {
    all_held(&walked("AWithdrawnOfferWithNoOpeningHolds"));
}

/// Two offers neither of which has an opening: the rule is named once on the
/// clock's line, and `does not fire` about it stays undecided with the
/// simulator's reason rather than reading the unknowns as a no.
#[test]
fn two_unanswered_offers_name_the_rule_once() {
    let walk = walked("TwoOffersWithNoOpeningAreNotAnswered");
    let all = outcomes(&walk);
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "after 1.day").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert_eq!(detail.matches("`Withdraw`").count(), 1, "{detail}");
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "then Withdraw does not fire").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert!(detail.contains("`Offer.opens_at > now`"), "{detail}");
}

/// Two different rules the clock could not settle in one step: a task nobody
/// gave a `started_at` and an offer with no opening. One rule already said is
/// no reason to stay quiet about another — each is named, once.
#[test]
fn two_rules_the_clock_left_undecided_are_both_named() {
    let walk = walked("TwoUnansweredRulesAreBothNamed");
    let all = outcomes(&walk);
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "after 1.day").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert_eq!(detail.matches("`Withdraw` could not be decided").count(), 1, "{detail}");
    assert_eq!(detail.matches("`Zombie` could not be decided").count(), 1, "{detail}");
}

/// A temporal rule whose `when` binds nothing is never read by the clock's
/// listing, as due or as undecided. Its silence is not a *no*: the chore's
/// deadline has passed, and `then Overdue does not fire` holding would be
/// an unknown read as false.
#[test]
fn a_rule_the_listing_never_reads_is_not_decided_by_its_silence() {
    let walk = walked("AChoreWhoseRuleBindsNothingIsNotAnswered");
    let all = outcomes(&walk);
    let (verdict, _, detail) =
        all.iter().find(|(_, about, _)| about == "then Overdue does not fire").expect("the line");
    assert_eq!(*verdict, Verdict::Undecided, "{all:#?}");
    let detail = detail.clone().expect("it says why");
    assert!(detail.contains("`Overdue` never became true"), "{detail}");
}
