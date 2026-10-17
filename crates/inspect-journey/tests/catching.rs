//! One act that makes more than one thing, and a journey naming each of them
//! (issue #31).
//!
//! A second `creating` line under an act used to be read as a continuation of
//! the first line's *type*: `creating review: Review creating comment: Comment`
//! was one name, `review`, of a type called `Review creating comment: Comment`.
//! With qualified types the type's last segment happened to be a real entity,
//! so it caught the wrong instance under the first name and nothing under the
//! second, silently.
//!
//! Against the recorded `allium` output and the real simulator, so what holds
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::REVIEWS;
use inspect_journey::{Verdict, Walk, check, parse, render, walk};

const JOURNEYS: &str = include_str!("fixtures/reviews.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(JOURNEYS).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(REVIEWS);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let text = render(std::slice::from_ref(walk));
    for step in &walk.steps {
        for outcome in &step.outcomes {
            assert_eq!(outcome.verdict, Verdict::Specified, "`{}` in\n{text}", outcome.about);
        }
        assert_eq!(step.verdict(), Verdict::Specified, "step {} in\n{text}", step.number);
    }
}

/// The line that reads `about`, its verdict and what it says.
fn line(walk: &Walk, about: &str) -> (Verdict, String) {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .find(|outcome| outcome.about == about)
        .map(|outcome| (outcome.verdict, outcome.detail.clone().unwrap_or_default()))
        .unwrap_or_else(|| panic!("no line `{about}` in\n{}", render(std::slice::from_ref(walk))))
}

/// The instance a name was bound to, by the cast the walk lists.
fn caught(walk: &Walk, name: &str) -> Option<String> {
    walk.cast
        .iter()
        .rev()
        .find(|member| member.name == name)
        .unwrap_or_else(|| panic!("`{name}` is not in the cast: {:?}", walk.cast))
        .entity
        .clone()
}

fn said(walk: &Walk) -> Vec<&str> {
    walk.stipulated.iter().map(|told| told.said.as_str()).collect()
}

/// The issue's reproduction: both names are bound, each to its own instance,
/// and each line's `where` is in force for what its creation sets off.
#[test]
fn each_creating_line_binds_its_own_name() {
    let walk = walked("ADraftReviewAndItsComment");
    all_held(&walk);
    assert_eq!(caught(&walk, "review").as_deref(), Some("Review#1"));
    assert_eq!(caught(&walk, "comment").as_deref(), Some("Comment#1"));
    assert_eq!(said(&walk), ["watchers(review) = {}", "mentions(comment) = {}"]);
}

/// The unqualified half of the issue, which read the whole tail as one type:
/// "no entity or actor called `Box creating tag: Tag`".
#[test]
fn unqualified_types_on_two_lines_are_two_types() {
    let walk = walked("UnqualifiedTypes");
    let text = render(std::slice::from_ref(&walk));
    assert!(!text.contains("no entity or actor called"), "{text}");
    all_held(&walk);
}

/// A `where` under one line answers for that line's instance only: the
/// comment's `mentions` was asked and answered by nobody, and the act says so
/// rather than holding.
#[test]
fn an_answer_under_one_line_says_nothing_about_another() {
    let walk = walked("OnlyOneIsAnswered");
    let (verdict, detail) = line(&walk, "ada does Submit(ada, \"looks good\") on PullRequest");
    assert_eq!(verdict, Verdict::Undecided, "{detail}");
    assert!(detail.contains("mentions(comment)"), "{detail}");
    assert!(!detail.contains("watchers"), "{detail}");
    // Caught all the same: an unanswered call is not an instance not made.
    assert_eq!(caught(&walk, "comment").as_deref(), Some("Comment#1"));
    assert_eq!(said(&walk), ["watchers(review) = {}"]);
}

/// Every name an act catches is bound before any of its answers is weighed,
/// so an answer may read what another line caught — whichever line is first.
/// Both are made in one turn here; `answers_land_as_each_line_s_instance_is_made`
/// is the case where the other is made a turn later, and shows the answer's
/// value is the instance.
#[test]
fn an_answer_may_name_what_another_line_caught() {
    let walk = walked("AnAnswerNamesTheOther");
    all_held(&walk);
    assert_eq!(said(&walk), ["mentions(comment) = {review}", "watchers(review) = {}"]);
}

/// Two lines of one kind catch two instances, in the order they were made —
/// not the first instance twice.
#[test]
fn lines_of_one_kind_catch_in_the_order_made() {
    let walk = walked("TwoOfAKind");
    all_held(&walk);
    assert_eq!(caught(&walk, "first").as_deref(), Some("Comment#1"));
    assert_eq!(caught(&walk, "second").as_deref(), Some("Comment#2"));
}

/// A line left over when the act made fewer of a kind than were asked for is
/// listed with nothing behind it, and the act says which.
#[test]
fn a_line_with_nothing_left_to_catch_says_so() {
    let walk = walked("OneMoreThanWasMade");
    let (_, detail) = line(&walk, "ada does Submit(ada, \"looks good\") on PullRequest");
    assert!(
        detail.contains("nothing of kind `reviews/Comment` was created for `second`"),
        "{detail}"
    );
    assert!(!detail.contains("for `first`"), "{detail}");
    assert_eq!(caught(&walk, "first").as_deref(), Some("Comment#1"));
    assert_eq!(caught(&walk, "second"), None);
}

/// Before anything runs, each line's type is checked and its name known: the
/// fixture names nothing the spec does not have.
#[test]
fn the_check_knows_every_name_caught() {
    let journeys = parse(JOURNEYS).expect("the journeys parse");
    let (graph, program, _) = common::library(REVIEWS);
    for journey in &journeys {
        let found = check(journey, &journeys, &graph, &program);
        assert!(found.is_empty(), "{}: {found:?}", journey.name);
    }
}

/// A second line's type is checked as well as the first's.
#[test]
fn the_check_reads_the_type_of_every_line() {
    let journeys = parse(
        "journey J {
    cast:
        ada: Reviewer
    1. x
        ada does Submit(ada, \"x\") on PullRequest
            creating review: Review
            creating comment: Remark
}",
    )
    .expect("parses");
    let (graph, program, _) = common::library(REVIEWS);
    let found = check(&journeys[0], &journeys, &graph, &program);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("`Remark`"), "{found:?}");
}

// --- one turn apart -----------------------------------------------------------

const FOLLOWUPS: &str = include_str!("fixtures/followups.journey");

fn followed(name: &str) -> Walk {
    let journeys = parse(FOLLOWUPS).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(common::FOLLOWUPS);
    walk(journey, &journeys, &graph, &program, &sources)
}

/// The draft is made a turn after the note. The note's own answer is in force
/// for what the note sets off; the one naming the draft waits for the draft
/// and then reads as it — `Cite` announces the very draft — and the draft's
/// own answer is in force once the draft is made.
#[test]
fn answers_land_as_each_line_s_instance_is_made() {
    let walk = followed("TheOtherComesLater");
    all_held(&walk);
    assert_eq!(caught(&walk, "note").as_deref(), Some("Note#1"));
    assert_eq!(caught(&walk, "draft").as_deref(), Some("Draft#1"));
    assert_eq!(
        said(&walk),
        ["mentions(note) = {ada}", "cites(note) = {draft}", "watchers(draft) = {}"]
    );
}

/// A name an earlier step caught is not what an answer reads while the line
/// that catches it anew in this act is still waiting for its instance.
#[test]
fn an_answer_reads_this_act_s_instance_not_an_earlier_one() {
    let walk = followed("TheOtherComesLaterAgain");
    all_held(&walk);
    assert_eq!(caught(&walk, "draft").as_deref(), Some("Draft#2"));
}

/// The other line never caught anything, so the answer naming it is refused
/// on the act's line and not listed — not told to the world as `{draft}`, the
/// state a name that names nobody reads as.
#[test]
fn an_answer_naming_a_line_that_caught_nothing_is_refused() {
    let walk = followed("TheOtherNeverComes");
    let step = &walk.steps[0];
    let text = render(std::slice::from_ref(&walk));
    assert_eq!(said(&walk), ["mentions(note) = {}"], "{text}");
    let refused: Vec<&str> = step
        .outcomes
        .iter()
        .filter(|outcome| outcome.verdict == Verdict::Undecided)
        .filter_map(|outcome| outcome.detail.as_deref())
        .collect();
    assert!(
        refused.contains(
            &"`draft` has no value once `note` is made, so `where cites(note)` gives the call \
              no answer"
        ),
        "{text}"
    );
    assert_eq!(step.verdict(), Verdict::Undecided, "{text}");
    assert_eq!(caught(&walk, "draft"), None);
}

/// A reaction removed the first of two tags. Each line keeps the instance it
/// caught: `first` names nothing afterwards, and says so when read, and
/// `second` is the tag its answer landed on — not moved up to `first`.
#[test]
fn a_removed_instance_does_not_move_its_sibling_up() {
    let walk = followed("OneOfThePairIsRemoved");
    assert_eq!(caught(&walk, "first"), None);
    assert_eq!(caught(&walk, "second").as_deref(), Some("Tag#2"));
    assert_eq!(said(&walk), ["watchers(second) = {}"]);
    assert_eq!(line(&walk, "then second.label = \"second\"").0, Verdict::Specified);
    assert_eq!(line(&walk, "then second.seen = false").0, Verdict::Specified);
    // Made and then removed, said as that — not as nothing being created,
    // which sends the reader after a rule that failed to make it.
    let (_, act) = line(&walk, "ada does Pair(ada) on Desk");
    assert!(act.contains("`first` caught `Tag#1`, which `Dedup` removed"), "{act}");
    assert!(!act.contains("was created for `first`"), "{act}");
    let (verdict, detail) = line(&walk, "then first.label = \"first\"");
    assert_eq!(verdict, Verdict::Undecided, "{detail}");
    assert!(detail.contains("`first` caught `Tag#1`, which `Dedup` removed, on line "), "{detail}");
}

/// A journey continuing from one whose act caught two names knows both.
#[test]
fn every_name_an_earlier_journey_caught_is_known() {
    let journeys = parse(FOLLOWUPS).expect("the journeys parse");
    let (graph, program, _) = common::library(common::FOLLOWUPS);
    let after = journeys.iter().find(|journey| journey.name == "AfterTheDraft").expect("there");
    let found = check(after, &journeys, &graph, &program);
    assert!(found.is_empty(), "{found:?}");

    // Known by its type as well: a field the second name's type does not have
    // is reported, which a name the check had never heard of is not.
    let text = format!(
        "{FOLLOWUPS}\njourney AfterTheDraftAgain {{\n    after: TheOtherComesLater\n    \
         1. a field the draft does not have\n        then draft.colour = false\n}}\n"
    );
    let journeys = parse(&text).expect("the journeys parse");
    let after = journeys.last().expect("there");
    let found = check(after, &journeys, &graph, &program);
    assert!(
        found.iter().any(|note| note.message == "`Draft` has no field called `colour`"),
        "{found:?}"
    );
}

/// An act at a surface that does not face its actor makes nothing, and every
/// line it would have caught is listed with nothing behind it.
#[test]
fn an_act_not_faced_lists_every_line_it_would_have_caught() {
    let walk = followed("NotFacedAtTheDesk");
    assert_eq!(walk.steps[0].verdict(), Verdict::Refused);
    assert_eq!(caught(&walk, "note"), None);
    assert_eq!(caught(&walk, "draft"), None);
}

/// Two lines left over are each said about, by name.
#[test]
fn every_line_left_over_is_named() {
    let walk = walked("TwoMoreThanWereMade");
    let (_, detail) = line(&walk, "ada does Submit(ada, \"looks good\") on PullRequest");
    assert!(
        detail.contains(
            "nothing of kind `reviews/Comment` was created for `second`; nothing of kind \
             `reviews/Comment` was created for `third`"
        ),
        "{detail}"
    );
}

/// An answer that read another line's name waited for that line's instance,
/// and what the first creation set off asked in the meantime. The remark says
/// the answer waited, and for what — not that the reaction made the note and
/// asked as it did, which blamed the wrong rule for the wrong reason.
#[test]
fn an_answer_that_waited_for_its_sibling_says_so_when_it_came_too_late() {
    let walk = followed("MentionsWaitForTheDraft");
    let text = render(std::slice::from_ref(&walk));
    assert!(
        text.contains(
            "`Mention` asked `mentions(note)` before `where mentions(note) = {draft}` was in \
             force — the answer reads `draft`, so it waited until `draft` was made, and \
             `Mention` asked no later than that"
        ),
        "{text}"
    );
    assert!(!text.contains("as it made `note`"), "{text}");
}
