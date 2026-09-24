//! Walking journeys against the desk fixture.
//!
//! Each case is one shape a real spec set wrote and the walker read wrongly or
//! not at all, reported from `friend-mesh` against `allium-journey` 0.1.0: a
//! context that narrows with `where`, a surface with two contexts, a surface
//! `let`, a collection named across a module boundary, a derived value taking
//! an argument, a set of states on the far side of `in`, two state rules on one
//! entity, and a state named in an act that nothing declares.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::DESK;
use inspect_journey::{Verdict, Walk, check, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(DESK);
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

/// Everything that did not hold, for a failure message that says which.
fn unheld(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    outcomes(walk).into_iter().filter(|(verdict, ..)| *verdict != Verdict::Specified).collect()
}

/// Ada places a hold, which is where most of these start.
const PLACED: &str = "
    cast:
        ada: lending/Member
        bea: lending/Member
    1. she places a hold
        ada does MemberPlacesHold(ada) on HoldReview creating hold: Hold";

fn journey(steps: &str) -> String {
    format!("journey J {{{PLACED}\n{steps}\n}}")
}

// --- two state rules on one entity -----------------------------------------

/// The rule whose condition became true runs, and the one beside it does not.
///
/// A state rule is fired by its *entity*, so `HoldIsGranted` becoming true
/// fired `Hold` — and every rule waiting on `Hold` ran, whatever its own
/// `when` said. `HoldExpires` sorts first, its only precondition is that the
/// hold is pending, and so a hold told it was ready lapsed a minute later,
/// with nothing having written `lapsed` but a rule whose condition was false.
#[test]
fn a_state_rule_runs_only_when_its_own_condition_holds() {
    let result = walked(&journey(
        "    2. it is ready
        stipulate hold.is_ready = true
    3. a minute later
        after 1.minute
        then HoldIsGranted fires
        then HoldExpires does not fire
        then hold.status = granted",
    ));

    assert_eq!(line(&result, "then HoldIsGranted fires").0, Verdict::Specified);
    assert_eq!(line(&result, "then hold.status = granted").0, Verdict::Specified);
    // Not refused, which is what it was. Whether it holds is a separate
    // question — see `does_not_fire_errs_toward_not_knowing_too`.
    assert_ne!(line(&result, "then HoldExpires does not fire").0, Verdict::Refused);
}

/// And the rule that one makes true in the same instant, on the same hold.
///
/// Settling remembered what it had run by trigger and instance, and for a
/// state rule the trigger is the entity: `HoldIsAnnounced` waiting on the hold
/// `HoldIsGranted` had just run for looked like `HoldIsGranted` again.
#[test]
fn a_second_state_rule_on_the_same_instance_is_not_taken_for_the_first() {
    let result = walked(&journey(
        "    2. it is ready
        stipulate hold.is_ready = true
    3. a minute later
        after 1.minute
        then HoldIsAnnounced fires",
    ));
    let (verdict, detail) = line(&result, "then HoldIsAnnounced fires");
    assert_eq!(verdict, Verdict::Specified, "{detail:?}");
    // And the firing that granted it reached `HoldIsAnnounced` too, which
    // calls the hold `news`. Read about nothing, it came back undecided, and
    // the minute passing was reported as something that could not be decided.
    let (verdict, detail) = line(&result, "after 1.minute");
    assert_eq!(verdict, Verdict::Specified, "{detail:?}");
}

/// Two holds ready in the same minute are two firings. One firing per
/// instance is the rule, and taking two instances for one would grant the
/// first and leave the second where it was.
#[test]
fn two_instances_ready_at_once_each_get_their_own_firing() {
    let result = walked(
        "journey J {
    cast:
        ada: lending/Member
        bea: lending/Member
    1. both place a hold
        ada does MemberPlacesHold(ada) on HoldReview creating first: Hold
        bea does MemberPlacesHold(bea) on HoldReview creating second: Hold
    2. both are ready
        stipulate first.is_ready = true
        stipulate second.is_ready = true
    3. a minute later
        after 1.minute
        then first.status = granted
        then second.status = granted
}",
    );
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// A negative claim about a rule that *did* run says that it ran.
///
/// The line was refused with no detail, so the report printed it with nothing
/// under it — which reads, at a glance, like a line that held.
#[test]
fn a_rule_that_ran_against_does_not_fire_says_it_ran() {
    let result = walked(&journey(
        "    2. it is ready
        stipulate hold.is_ready = true
    3. a minute later
        after 1.minute
        then HoldIsGranted does not fire",
    ));
    let (verdict, detail) = line(&result, "then HoldIsGranted does not fire");
    assert_eq!(verdict, Verdict::Refused);
    assert_eq!(detail.as_deref(), Some("`HoldIsGranted` ran"));
}

// --- a context that narrows -------------------------------------------------

/// `context hold: Hold where status = pending` is a context, and a filter.
///
/// The `where` made the whole clause unreadable as a context, so nothing was
/// ever bound to `hold` and every field the surface shows was undecided.
#[test]
fn a_context_with_a_filter_is_bound_and_its_filter_admits_it() {
    let result = walked(&journey("        ada sees hold.placed_at on PendingHold in hold"));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// And a filter that says no is a surface this instance does not have.
#[test]
fn a_context_its_filter_refuses_shows_nothing() {
    let result = walked(&journey(
        "    2. she withdraws it
        ada does MemberWithdrawsHold(hold, changed_mind) on HoldReview
        ada cannot see hold.placed_at on PendingHold in hold",
    ));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
    let (_, detail) = line(&result, "ada cannot see hold.placed_at on PendingHold in hold");
    assert!(
        detail.as_deref().is_some_and(|why| why.contains("not `status = pending`")),
        "{detail:?}"
    );
}

// --- two contexts -----------------------------------------------------------

/// Each context bound on its own: the hold from `in`, the borrower from who is
/// looking.
///
/// A surface held one context, so the second declaration replaced the first
/// and `in` could only ever bind the one that was left. Naming the hold
/// unbound the borrower, and not naming it asked for the hold.
#[test]
fn two_contexts_are_bound_one_from_in_and_one_from_the_actor() {
    let result = walked(&journey(
        "        ada sees hold.status on HoldDesk in hold
        ada sees ada.name on HoldDesk in hold
        ada cannot see bea.name on HoldDesk in hold",
    ));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// Both named, when the actor is neither.
#[test]
fn in_names_several_contexts_at_once() {
    let result = walked(&journey("        hold sees ada.name on HoldDesk in ada, hold"));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// A context nothing bound is undecided only where something reads it, and
/// the reason is still the remedy.
#[test]
fn a_context_nobody_named_is_asked_for_where_it_is_read() {
    let result = walked(&journey("        ada sees hold.status on HoldDesk"));
    let (verdict, detail) = line(&result, "ada sees hold.status on HoldDesk");
    assert_eq!(verdict, Verdict::Undecided);
    assert!(detail.as_deref().is_some_and(|why| why.contains("in <the Hold>")), "{detail:?}");
}

/// A name `in` gives that fits no context is said to, rather than dropped.
///
/// `PendingHold` is at a hold and nothing else, so `ada` there names a room
/// the surface is not in — and answering as if she had not been named would
/// be answering a different question.
#[test]
fn in_naming_something_no_context_takes_is_undecided() {
    let result = walked(&journey("        ada sees hold.placed_at on PendingHold in hold, ada"));
    let (verdict, detail) = line(&result, "ada sees hold.placed_at on PendingHold in hold, ada");
    assert_eq!(verdict, Verdict::Undecided);
    assert!(detail.as_deref().is_some_and(|why| why.contains("`ada` is `Member`")), "{detail:?}");
}

/// A second name of the same type goes to a second context of it, and with
/// only one to go to it is one too many.
#[test]
fn in_naming_two_of_one_type_for_one_context_is_undecided() {
    let result = walked(&journey("        ada sees hold.status on HoldDesk in hold, hold"));
    let (verdict, detail) = line(&result, "ada sees hold.status on HoldDesk in hold, hold");
    assert_eq!(verdict, Verdict::Undecided);
    assert!(detail.as_deref().is_some_and(|why| why.contains("no context")), "{detail:?}");
}

// --- a surface `let` --------------------------------------------------------

#[test]
fn a_surface_let_is_bound_before_its_exposes_clause_reads_it() {
    let result = walked(&journey(
        "    2. bea reviews it
        bea does MemberReviewsHold(hold, bea) on HoldReview
        ada sees bea.name on HoldDesk in hold
        ada cannot see ada.name on HoldDesk in bea, hold",
    ));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

// --- a collection named in another module ------------------------------------

/// `for loan in lending/Loans where member = borrower` — the plural, qualified.
///
/// The same-module spelling was reduced to its entity and this one was not, so
/// it ranged over instances of an entity called `Loans`, of which there are
/// none, and reported the loan unexposed.
#[test]
fn a_qualified_collection_is_the_entity_it_is_the_plural_of() {
    let result = walked(
        "journey J {
    cast:
        ada:  lending/Member
        bea:  lending/Member
        copy: catalogue/Copy
    given:
        copy.status = available
    1. she borrows, and places a hold
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: lending/Loan
        ada does MemberPlacesHold(ada) on HoldReview creating hold: Hold
        ada sees loan.status on HoldDesk in hold
        bea cannot see loan.status on HoldDesk in hold
}",
    );
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

// --- a derived value that takes an argument ----------------------------------

/// `is_reviewed_by(who): who in reviewers`, called as `hold.is_reviewed_by(x)`.
#[test]
fn a_derived_value_with_a_parameter_is_computed_for_its_argument() {
    let result = walked(&journey(
        "    2. bea reviews it, and cannot twice
        bea does MemberReviewsHold(hold, bea) on HoldReview
        then bea in hold.reviewers
    3. again
        bea does MemberReviewsHold(hold, bea) on HoldReview
        then hold.reviewers = {bea}",
    ));
    assert_eq!(result.steps[1].outcomes[0].verdict, Verdict::Specified, "{:#?}", result.steps[1]);
    assert_eq!(result.steps[2].outcomes[0].verdict, Verdict::Refused, "{:#?}", result.steps[2]);
}

// --- states in a set literal -------------------------------------------------

/// `hold.status in {pending, granted}` — the names are the field's states.
#[test]
fn a_set_of_states_beside_a_state_is_read_as_states() {
    let result = walked(&journey(
        "    2. bea reviews it
        bea does MemberReviewsHold(hold, bea) on HoldReview
        then ReviewHold fires",
    ));
    assert_eq!(line(&result, "then ReviewHold fires").0, Verdict::Specified);
}

/// And a trigger argument that is one: the journey passes `changed_mind`.
#[test]
fn a_state_passed_to_an_act_is_found_in_a_set_of_states() {
    let result = walked(&journey(
        "    2. she withdraws it
        ada does MemberWithdrawsHold(hold, changed_mind) on HoldReview
        then hold.status = withdrawn
        then hold.withdrawn_because = changed_mind",
    ));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// The third state is declared and not admitted, which is a refusal.
#[test]
fn a_state_the_set_does_not_hold_is_refused() {
    let result = walked(&journey(
        "    2. she withdraws it
        ada does MemberWithdrawsHold(hold, moved_away) on HoldReview",
    ));
    assert_eq!(result.steps[1].outcomes[0].verdict, Verdict::Refused, "{:#?}", result.steps[1]);
}

// --- a state nothing declares ------------------------------------------------

/// `removal` where the spec says `remove_user`: a word the walker reads as a
/// state, which no enumeration in the spec declares, so no rule can ever match
/// it. It walked — and the journey spent months blaming the walker.
#[test]
fn a_state_no_enumeration_declares_is_reported_by_the_checker() {
    let source = journey(
        "    2. she withdraws it
        ada does MemberWithdrawsHold(hold, chnaged_mind) on HoldReview",
    );
    let journeys = parse(&source).expect("parses");
    let (graph, program, _) = common::library(DESK);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    assert!(
        notes
            .iter()
            .any(|note| note.verdict == Verdict::Unspecified
                && note.message.contains("`chnaged_mind`")),
        "{notes:#?}"
    );
}

/// A word no enumeration declares, handed to a rule that only *stores* it, is
/// not reported: nothing is ever asked to match it. `PersonCreatesGroup(she,
/// "Bruno", genesis, …)` is this shape, and reporting it stopped a step the
/// spec supports — and every step after it that needed the group.
#[test]
fn a_word_a_rule_only_stores_is_not_reported() {
    let source = journey(
        "    2. bea reviews it
        bea does MemberReviewsHold(hold, bea) on HoldReview
    3. and somebody nobody cast places another
        ada does MemberPlacesHold(somebody) on HoldReview",
    );
    let journeys = parse(&source).expect("parses");
    let (graph, program, _) = common::library(DESK);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    assert!(
        !notes.iter().any(|note| note.message.contains("no state the spec declares")),
        "{notes:#?}"
    );
}

/// Somebody the journey cast, passed where a rule compares states, is who
/// they are — a question for the walk, not a word the checker reads as a state.
#[test]
fn a_cast_name_where_a_rule_compares_states_is_not_reported() {
    let source = journey(
        "    2. she withdraws it, oddly
        ada does MemberWithdrawsHold(hold, bea) on HoldReview",
    );
    let journeys = parse(&source).expect("parses");
    let (graph, program, _) = common::library(DESK);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    assert!(
        !notes.iter().any(|note| note.message.contains("no state the spec declares")),
        "{notes:#?}"
    );
}

/// Only the rules on *this* act's trigger count. `WithdrawHold` compares its
/// second parameter against states, and `ReviewHold`'s second is a reviewer —
/// so an undeclared word there is somebody nobody cast, not a misspelt state.
#[test]
fn a_rule_on_another_trigger_does_not_make_a_word_a_state() {
    let source = journey(
        "    2. somebody nobody cast reviews it
        ada does MemberReviewsHold(hold, stranger) on HoldReview",
    );
    let journeys = parse(&source).expect("parses");
    let (graph, program, _) = common::library(DESK);
    let notes = check(&journeys[0], &journeys, &graph, &program);
    assert!(
        !notes.iter().any(|note| note.message.contains("no state the spec declares")),
        "{notes:#?}"
    );
}

/// And a declared one is not, nor is somebody the journey cast.
#[test]
fn a_declared_state_and_a_cast_name_are_not_reported() {
    let source = journey(
        "    2. she withdraws it
        ada does MemberWithdrawsHold(hold, changed_mind) on HoldReview",
    );
    let journeys = parse(&source).expect("parses");
    let (graph, program, _) = common::library(DESK);
    assert!(check(&journeys[0], &journeys, &graph, &program).is_empty());
}

// --- removal -----------------------------------------------------------------

/// `ensures: not exists hold` removes the hold, which is what the language
/// says it means as an outcome. It was noted and not acted on, so `then hold
/// does not exist` was refused after a removal that went through.
#[test]
fn a_rule_that_ensures_something_does_not_exist_removes_it() {
    let result = walked(&journey(
        "    2. she withdraws it, and throws it away
        ada does MemberWithdrawsHold(hold, changed_mind) on HoldReview
        ada does MemberDiscardsHold(hold) on HoldReview
        then DiscardHold fires
        then hold does not exist",
    ));
    assert!(unheld(&result).is_empty(), "{:#?}", unheld(&result));
}

/// And only once: a hold still pending is not discarded, and still exists.
#[test]
fn a_removal_its_rule_refused_leaves_the_instance_where_it_was() {
    let result = walked(&journey(
        "    2. she tries to throw away one still waiting
        ada does MemberDiscardsHold(hold) on HoldReview
        then hold exists",
    ));
    assert_eq!(result.steps[1].outcomes[0].verdict, Verdict::Refused);
    assert_eq!(line(&result, "then hold exists").0, Verdict::Specified);
}
