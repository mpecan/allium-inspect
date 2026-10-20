//! An act that says which context it is at through a guard on its `provides`
//! line.
//!
//! `Member` is identified within a `Group`, and `Conversation` is at one. An
//! act that carries the group settles it from its arguments; reading a
//! message carries the message, not the group, so nothing on the line said
//! which group Ada was reading in, and every read by every member was
//! undecided. `PersonReadsMessage(message, reader) when message.group = group`
//! is the surface saying so, once: the act is at the message's group.
//!
//! Read as a tie and nothing looser. Only `group = …` on its own or under
//! `and` says which; an `or`, an inequality, or two ties to different groups
//! leave it undecided, saying why — the walk does not pick.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::ROOMS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(ROOMS);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

/// The outcome of the line that reads `about`.
fn line(walk: &Walk, about: &str) -> (Verdict, String) {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .chain(walk.notes.iter())
        .find(|outcome| outcome.about == about)
        .map(|outcome| (outcome.verdict, outcome.detail.clone().unwrap_or_default()))
        .unwrap_or_else(|| panic!("no line `{about}` in {walk:#?}"))
}

/// Ada, two groups, her membership of `room`, and a note and a second message
/// whose groups `given` decides.
fn journey(given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
        ada: Identity
        room: Group
        hall: Group
        mine: Membership
        note: Message
        other: Message
    given:
        mine.member = ada
        mine.left = false
        note.read = false
{given}
    1. the step
{clauses}
}}"
    )
}

const MEMBER_OF_ROOM: &str = "        mine.group = room";

// --- the tie settles it ------------------------------------------------------

/// The case as found: a member reading a note in her own group reads it.
#[test]
fn a_read_whose_guard_ties_the_group_to_the_message_is_at_the_messages_group() {
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room"),
        "        ada does PersonReadsMessage(note, ada) on Conversation\n        then note.read = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

#[test]
fn a_tie_written_the_other_way_round_settles_it_too() {
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room"),
        "        ada does GroupGreets(note, ada) on Conversation\n        then note.read = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// `membership.left = false and membership.group = group`: a tie beside
/// another condition is still a tie.
#[test]
fn a_tie_under_and_beside_another_condition_settles_it() {
    let result = walked(&journey(
        MEMBER_OF_ROOM,
        "        ada does MemberLeaves(mine, ada) on Conversation\n        then mine.left = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// Two ties that land on the same group say the same thing twice.
#[test]
fn two_ties_to_the_same_group_settle_it() {
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room\n        other.group = room"),
        "        ada does MemberMoves(note, other, ada) on Conversation\n        then note.read = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// The surface passing the group itself is settled as it always was.
#[test]
fn an_act_that_carries_the_group_is_still_settled_by_its_argument() {
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        room.quiet = true"),
        "        ada does MemberPosts(room, ada) on Conversation\n        then room.quiet = false",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// A member of another group only is definitely not the `Member` a
/// conversation in this one faces, and both polarities say so.
#[test]
fn a_member_of_another_group_only_is_refused_at_this_ones_message() {
    let does = "ada does PersonReadsMessage(note, ada) on Conversation";
    let cannot = "ada cannot do PersonReadsMessage(note, ada) on Conversation";
    let result = walked(&journey(
        "        mine.group = hall\n        note.group = room",
        &format!("        {cannot}\n        {does}\n        then note.read = false"),
    ));
    let (verdict, why) = line(&result, cannot);
    assert_eq!(verdict, Verdict::Specified, "{why}");
    assert_eq!(
        why,
        "`Conversation` faces `Member`, and `ada` is not one: not `exists Membership{group: \
         within, member: this}`"
    );
    assert_eq!(line(&result, does).0, Verdict::Refused);
    assert_eq!(line(&result, "then note.read = false").0, Verdict::Specified);
}

// --- the tie cannot be read ---------------------------------------------------

/// The tie is there and the field it reads is not: undecided both ways,
/// naming the field and the note, never the world's id for it.
#[test]
fn a_tie_through_a_field_nobody_set_is_undecided_naming_the_field() {
    for clause in [
        "ada does PersonReadsMessage(note, ada) on Conversation",
        "ada cannot do PersonReadsMessage(note, ada) on Conversation",
    ] {
        let result = walked(&journey(MEMBER_OF_ROOM, &format!("        {clause}")));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, Verdict::Undecided, "{clause}: {why}");
        assert_eq!(
            why,
            "`Member` is identified within a `Group`, and `Conversation` passes `group` as \
             `message.group`, and `note` has no `group` set, so whether `ada` is the `Member` \
             it faces is not settled"
        );
    }
}

/// No guard, so nothing changed — except that the reason now names the one
/// thing that would settle it.
#[test]
fn an_act_with_no_guard_is_told_the_guard_that_would_say_which() {
    let clause = "ada does MemberShouts(note, ada) on Conversation";
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room"),
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(
        why,
        "`Member` is identified within a `Group`, and no argument of this act is the `Group` \
         `Conversation` passes as `group`, so whether `ada` is the `Member` it faces is not \
         settled — a guard on its `provides` line such as `when message.group = group` would \
         say which"
    );
}

/// `message.group = group or message.pinned = true` is offered at a pinned
/// message of any group, so it does not say which one.
#[test]
fn a_tie_under_an_or_is_not_read_as_one() {
    let clause = "ada does MemberPins(note, ada) on Conversation";
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room"),
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(
        why,
        "`Member` is identified within a `Group`, and `Conversation` offers `MemberPins` only \
         `when message.group = group or message.pinned = true`, which ties `group` to nothing — \
         only `group = …` on its own or under `and` says which, so whether `ada` is the \
         `Member` it faces is not settled"
    );
}

#[test]
fn an_inequality_is_not_a_tie() {
    let clause = "ada does MemberHides(note, ada) on Conversation";
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = hall"),
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(
        why.contains("only `when message.group != group`, which ties `group` to nothing"),
        "{why}"
    );
}

/// Two ties that land on different groups: which one is not the walk's to
/// pick, whichever she is a member of.
#[test]
fn two_ties_to_different_groups_are_undecided_and_say_so() {
    let clause = "ada does MemberMoves(note, other, ada) on Conversation";
    let result = walked(&journey(
        &format!("{MEMBER_OF_ROOM}\n        note.group = room\n        other.group = hall"),
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(
        why,
        "`Member` is identified within a `Group`, and `Conversation` passes `group` as both \
         `message.group` and `other.group`, which are not the same `Group`, so whether `ada` is \
         the `Member` it faces is not settled"
    );
}

/// A tie that lands on somebody rather than on a group says nothing about
/// which group, and says what it landed on.
#[test]
fn a_tie_to_something_that_is_not_the_context_is_undecided_and_names_it() {
    let clause = "ada does MemberQuotes(mine, ada) on Conversation";
    let result = walked(&journey(MEMBER_OF_ROOM, &format!("        {clause}")));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(
        why.contains("`Conversation` passes `group` as `membership.member`, which is "),
        "{why}"
    );
    assert!(why.contains(", not a `Group`, so whether `ada`"), "{why}");
}
