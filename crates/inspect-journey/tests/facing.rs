//! Who a surface faces, and whether somebody is one (issue #13).
//!
//! `bob does Flip(bob) on AdminPanel` used to read only the rule behind the
//! act. `AdminPanel` faces a `SiteAdmin` — a user whose `is_admin` is true —
//! and `Flip` has no `requires`, so a user who was not an admin flipped it, and
//! `bob cannot do Flip(bob) on AdminPanel` was refused with "`Flip` went
//! through". A surface that said it faced site admins let anyone through, and
//! nothing reported it.
//!
//! Now the actor's `identified_by` is read against whoever is at the surface,
//! for an act and for a look, and the three answers are kept apart: one of
//! them, not one of them (with the condition, as written), or not settled
//! (with what was missing).

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::FACING;
use inspect_journey::{Origin, Verdict, Walk, parse, walk};

fn walked_nth(source: &str, at: usize) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(FACING);
    walk(&journeys[at], &journeys, &graph, &program, &sources)
}

fn walked(source: &str) -> Walk {
    walked_nth(source, 0)
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

/// One journey with a cast, a `given`, and the clauses of one step.
fn journey(cast: &str, given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
{cast}
    given:
{given}
    1. the step
{clauses}
}}"
    )
}

const FILED: &str = include_str!("fixtures/facing.journey");

// --- the issue, as filed -----------------------------------------------------

#[test]
fn the_issue_as_filed_is_refused_by_the_actor_the_surface_faces() {
    let result = walked_nth(FILED, 0);
    let (verdict, why) = line(&result, "bob cannot do Flip(bob) on AdminPanel");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    // Which actor, and which condition: the sentence the block is made of.
    assert_eq!(why, "`AdminPanel` faces `SiteAdmin`, and `bob` is not one: not `is_admin = true`");
    // And nothing fired: the world is the one he walked up to.
    assert_eq!(line(&result, "then bob.flag = false").0, Verdict::Specified);
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

#[test]
fn the_issue_as_filed_is_refused_across_a_module_as_well() {
    let result = walked_nth(FILED, 1);
    let (verdict, why) = line(&result, "bob cannot do Wave(bob) on RemotePanel");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    assert_eq!(why, "`RemotePanel` faces `SiteAdmin`, and `bob` is not one: not `is_admin = true`");
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

// --- acts ------------------------------------------------------------------

const BOB: &str = "        bob: User";

#[test]
fn somebody_the_surface_faces_acts_on_it() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = true\n        bob.flag = false",
        "        bob does Flip(bob) on AdminPanel\n        then bob.flag = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// `does` by somebody the surface does not face is the spec saying no, and
/// says which actor said it.
#[test]
fn an_act_by_somebody_the_surface_does_not_face_is_refused() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = false\n        bob.flag = false",
        "        bob does Flip(bob) on AdminPanel\n        then bob.flag = false",
    ));
    let (verdict, why) = line(&result, "bob does Flip(bob) on AdminPanel");
    assert_eq!(verdict, Verdict::Refused);
    assert!(why.contains("`AdminPanel` faces `SiteAdmin`"), "{why}");
    // Refused, so not run: the flag is where it was.
    assert_eq!(line(&result, "then bob.flag = false").0, Verdict::Specified);
}

/// The mirror: somebody it does face is not refused, so `cannot do` is.
#[test]
fn cannot_do_by_somebody_the_surface_faces_is_refused_when_it_goes_through() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = true\n        bob.flag = false",
        "        bob cannot do Flip(bob) on AdminPanel",
    ));
    let (verdict, why) = line(&result, "bob cannot do Flip(bob) on AdminPanel");
    assert_eq!(verdict, Verdict::Refused);
    assert_eq!(why, "`Flip` went through");
}

/// Nothing said whether Bob is an admin. Neither `does` nor `cannot do` may
/// answer — the first would let anyone through, the second would block
/// somebody nothing checked — and the reason names the condition and what
/// was missing from it, as the line to write in `given`.
#[test]
fn an_actor_nothing_settled_is_undecided_both_ways() {
    for clause in ["bob does Flip(bob) on AdminPanel", "bob cannot do Flip(bob) on AdminPanel"] {
        let result =
            walked(&journey(BOB, "        bob.flag = false", &format!("        {clause}")));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, Verdict::Undecided, "{clause}: {why}");
        assert_eq!(
            why,
            "whether `bob` is the `SiteAdmin` `AdminPanel` faces could not be settled — \
             `is_admin = true`: `bob` has no `is_admin` set",
            "{clause}"
        );
    }
}

/// Unsettled, nothing ran — but nothing *decided* it did not, either. A
/// `then Flip does not fire` holding there is unknown read as false.
#[test]
fn what_an_unsettled_act_would_have_fired_is_not_said_not_to_have() {
    let result = walked(&journey(
        BOB,
        "        bob.flag = false",
        "        bob does Flip(bob) on AdminPanel\n        then Flip does not fire",
    ));
    let (verdict, why) = line(&result, "then Flip does not fire");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("`bob` has no `is_admin` set"), "{why}");
}

/// Closed, and a `then` about the rule is told why it did not run.
#[test]
fn a_rule_behind_a_closed_surface_is_refused_by_the_surface() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = false\n        bob.flag = false",
        "        bob does Flip(bob) on AdminPanel\n        then Flip fires\n        then Flip does \
         not fire",
    ));
    let (verdict, why) = line(&result, "then Flip fires");
    assert_eq!(verdict, Verdict::Refused, "{why}");
    assert!(why.starts_with("`Flip` refused: `AdminPanel` faces `SiteAdmin`"), "{why}");
    assert_eq!(line(&result, "then Flip does not fire").0, Verdict::Specified);
}

/// Unsettled facing *and* a rule that refuses him anyway: the act does not go
/// through whichever way the first question goes, so that is the answer —
/// `cannot do` holds and `does` is refused, each saying both halves.
#[test]
fn a_rule_that_refuses_decides_an_act_the_surface_could_not() {
    for (clause, expected) in [
        ("bob cannot do Lock(bob) on AdminPanel", Verdict::Specified),
        ("bob does Lock(bob) on AdminPanel", Verdict::Refused),
    ] {
        let result = walked(&journey(BOB, "        bob.flag = true", &format!("        {clause}")));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, expected, "{clause}: {why}");
        assert!(why.starts_with("`Lock` refused: u.flag = false; "), "{clause}: {why}");
        assert!(why.contains("`bob` has no `is_admin` set"), "{clause}: {why}");
    }
}

/// And a rule that would let him through leaves it undecided, with the world
/// put back: what the act would have written is not what happened.
#[test]
fn a_rule_that_would_go_through_leaves_an_unsettled_act_undecided() {
    let result = walked(&journey(
        BOB,
        "        bob.flag = false",
        "        bob cannot do Lock(bob) on AdminPanel\n        then Lock does not fire",
    ));
    assert_eq!(line(&result, "bob cannot do Lock(bob) on AdminPanel").0, Verdict::Undecided);
    assert_eq!(line(&result, "then Lock does not fire").0, Verdict::Undecided);
    let flag = result.steps[0].world.entities.values().find_map(|it| it.fields.get("flag"));
    assert_eq!(flag, Some(&inspect_sim::Value::Bool(false)), "{result:#?}");
}

/// An act that would have made something and did not — refused, or nobody
/// could tell — still lists what it would have caught, as nobody, so a later
/// line naming it is told why.
#[test]
fn an_act_the_surface_stopped_lists_what_it_would_have_caught_as_nobody() {
    for given in ["        bob.is_admin = false", ""] {
        let result = walked(&journey(
            BOB,
            &format!("{given}\n        bob.flag = false"),
            "        bob does Flip(bob) on AdminPanel creating made: User",
        ));
        let made = result.cast.iter().find(|member| member.name == "made");
        let made = made.unwrap_or_else(|| panic!("`made` is listed: {:#?}", result.cast));
        assert_eq!(made.entity, None, "{given}");
        assert_eq!(made.origin, Origin::Caught, "{given}");
    }
}

/// Cast as the actor, which is the journey saying so where a reader looks
/// first — and how this repository's own journeys cast their people.
#[test]
fn somebody_cast_as_the_actor_is_one() {
    let clause = "boss does Flip(boss) on AdminPanel";
    let result = walked(&journey("        boss: SiteAdmin", "", &format!("        {clause}")));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Specified, "{why}");
}

/// The same from another module: the cast names the actor where it is
/// declared, and the surface that faces it is in the module that imports it.
#[test]
fn somebody_cast_as_another_modules_actor_is_one() {
    let clause = "boss does Wave(boss) on RemotePanel";
    let result =
        walked(&journey("        boss: admins/SiteAdmin", "", &format!("        {clause}")));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Specified, "{why}");
}

/// `identified_by: User` with no `where` demands a user and nothing more.
#[test]
fn an_actor_with_no_condition_demands_nothing_of_a_user() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = false\n        bob.flag = false",
        "        bob does Flip(bob) on OpenPanel\n        then bob.flag = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// A surface facing an entity of another type than whoever acts is not
/// adjudicated: it is a small inconsistency or one person being both, and the
/// static check already declines to pick between the two.
#[test]
fn a_surface_facing_an_entity_of_another_type_is_not_a_refusal() {
    let result = walked(&journey(
        "        bob: User\n        crew: Team",
        "        crew.flag = false",
        "        bob does Flip(crew) on TeamPanel\n        then crew.flag = true",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// A *condition* on a `User` asked of a `Team` is a question with no answer
/// here: undecided, naming both types, never a yes and never a no.
#[test]
fn a_condition_asked_of_somebody_of_another_type_is_undecided() {
    let clause = "crew does Flip(crew) on AdminPanel";
    let result = walked(&journey(
        "        crew: Team",
        "        crew.flag = false",
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(
        why,
        "`AdminPanel` faces `SiteAdmin`, a `User` where `is_admin = true`, and `crew` is a \
         `Team` — whether they are one is not something the specification says"
    );
}

/// `within` is the workspace the act is at, and the surface says which
/// argument that is: `provides: Archive(workspace)`. A seat in that workspace
/// is its owner, and a seat in another one is not.
#[test]
fn an_act_binds_within_to_the_argument_its_surface_passes_the_context_as() {
    let result = walked(&journey(
        "        mine: Seat\n        theirs: Seat\n        desk: Workspace\n        other: Workspace",
        "        mine.workspace = desk\n        theirs.workspace = other\n        desk.archived = false",
        "        theirs cannot do Archive(desk) on WorkspacePanel\n        mine does Archive(desk) \
         on WorkspacePanel\n        then desk.archived = true",
    ));
    let (verdict, why) = line(&result, "theirs cannot do Archive(desk) on WorkspacePanel");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    assert_eq!(
        why,
        "`WorkspacePanel` faces `WorkspaceOwner`, and `theirs` is not one: not `workspace = within`"
    );
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// A surface at no workspace has no argument or `in` that says which one, so
/// an act and a look are undecided both ways — and told the remedy that works,
/// not an `in` the walk would then reject.
#[test]
fn an_actor_identified_within_a_context_the_surface_is_not_at_is_undecided() {
    for clause in [
        "seat does Leave(seat) on BarePanel",
        "seat cannot do Leave(seat) on BarePanel",
        "seat sees seat.left on BarePanel",
        "seat cannot see seat.left on BarePanel",
    ] {
        let result = walked(&journey(
            "        seat: Seat\n        desk: Workspace",
            "        seat.workspace = desk\n        seat.left = false",
            &format!("        {clause}"),
        ));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, Verdict::Undecided, "{clause}: {why}");
        assert!(why.contains("`WorkspaceOwner` is identified within a `Workspace`"), "{why}");
        assert!(why.contains("`BarePanel` is not at any `Workspace`"), "{clause}: {why}");
        assert!(!why.contains("say which one with"), "{clause}: {why}");
    }
}

/// Where the surface is at a workspace and the act passes none — a seat
/// nothing placed — both polarities are undecided, never blocked.
#[test]
fn an_act_by_an_owner_nothing_settled_is_undecided_both_ways() {
    for clause in [
        "seat does Archive(desk) on WorkspacePanel",
        "seat cannot do Archive(desk) on WorkspacePanel",
    ] {
        let result = walked(&journey(
            "        seat: Seat\n        desk: Workspace",
            "        desk.archived = false",
            &format!("        {clause}"),
        ));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, Verdict::Undecided, "{clause}: {why}");
        assert!(why.contains("`seat` has no `workspace` set"), "{clause}: {why}");
    }
}

// --- looks -----------------------------------------------------------------

/// A surface closed to somebody shows them nothing, so `cannot see` holds and
/// says why, and `sees` is not exposed to them.
#[test]
fn a_surface_that_does_not_face_somebody_shows_them_nothing() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = false\n        bob.flag = false",
        "        bob cannot see bob.flag on AdminPanel\n        bob sees bob.flag on AdminPanel",
    ));
    let (verdict, why) = line(&result, "bob cannot see bob.flag on AdminPanel");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    assert!(why.contains("`AdminPanel` faces `SiteAdmin`"), "{why}");
    assert_eq!(line(&result, "bob sees bob.flag on AdminPanel").0, Verdict::Unexposed);
}

#[test]
fn a_surface_that_faces_somebody_shows_them_what_it_exposes() {
    let result = walked(&journey(
        BOB,
        "        bob.is_admin = true\n        bob.flag = false",
        "        bob sees bob.flag on AdminPanel",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// Unsettled, a look is undecided both ways — a privacy claim that passed
/// because nothing checked who was looking is the worst answer available.
#[test]
fn a_look_by_somebody_nothing_settled_is_undecided_both_ways() {
    for clause in ["bob sees bob.flag on AdminPanel", "bob cannot see bob.flag on AdminPanel"] {
        let result =
            walked(&journey(BOB, "        bob.flag = false", &format!("        {clause}")));
        let (verdict, why) = line(&result, clause);
        assert_eq!(verdict, Verdict::Undecided, "{clause}: {why}");
        assert!(why.contains("`is_admin = true`"), "{clause}: {why}");
    }
}

/// A look names its context with `in`, so `within` is bound to it: a seat in
/// this workspace is its owner, and a seat in another one is not.
#[test]
fn a_look_binds_within_to_the_context_it_names() {
    let result = walked(&journey(
        "        mine: Seat\n        theirs: Seat\n        desk: Workspace\n        other: Workspace",
        "        mine.workspace = desk\n        theirs.workspace = other\n        desk.archived = false",
        "        mine sees desk.archived on WorkspacePanel in desk\n        theirs cannot see \
         desk.archived on WorkspacePanel in desk",
    ));
    assert_eq!(
        line(&result, "mine sees desk.archived on WorkspacePanel in desk").0,
        Verdict::Specified
    );
    let (verdict, why) = line(&result, "theirs cannot see desk.archived on WorkspacePanel in desk");
    assert_eq!(verdict, Verdict::Specified, "{why}");
    assert_eq!(
        why,
        "`WorkspacePanel` faces `WorkspaceOwner`, and `theirs` is not one: not `workspace = within`"
    );
}

/// `within` bound, and something *else* in the condition unsettled: the reason
/// is what was missing, not advice to name a context the line already named.
#[test]
fn a_look_that_named_its_context_says_what_else_was_missing() {
    let clause = "seat sees desk.archived on WorkspacePanel in desk";
    let result = walked(&journey(
        "        seat: Seat\n        desk: Workspace",
        "        desk.archived = false",
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("`seat` has no `workspace` set"), "{why}");
    assert!(!why.contains("say which one"), "{why}");
}
