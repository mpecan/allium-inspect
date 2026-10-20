//! A loop over a `->` projection.
//!
//! `people: live_seats -> person` came back undecided as "a projection"
//! whatever it was over, so `for p in room.people` was never settled — while
//! the same loop over `room.live_seats`, reading `s.person`, was. A projection
//! over a settled collection is settled: each kept member mapped to its field.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::SEATS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(SEATS);
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

fn journey(given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
        a: Person
        r: Room
    given:
{given}
    1. the step
{clauses}
}}"
    )
}

/// The case as found.
#[test]
fn a_loop_over_a_projection_pings_whoever_sits_in_a_live_seat() {
    let result = walked(&journey(
        "        s: Seat { room: r, person: a, status: active }",
        "        a does Shout(r) on Hall\n            creating ping: Ping\n        then ping.to = a",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// And it agrees with the loop over the seats themselves.
#[test]
fn the_same_loop_over_the_seats_still_settles() {
    let result = walked(&journey(
        "        s: Seat { room: r, person: a, status: active }",
        "        a does ShoutSeats(r) on Hall\n            creating ping: Ping\n        then ping.to = a",
    ));
    assert_eq!(result.verdict(), Verdict::Specified, "{result:#?}");
}

/// A live seat nobody sits in may be somebody the loop would ping: the act is
/// undecided, saying which field, never applied to the rest as if complete.
#[test]
fn a_live_seat_with_nobody_set_in_it_leaves_the_loop_undecided_naming_the_field() {
    let clause = "a does Shout(r) on Hall";
    let result = walked(&journey(
        "        s: Seat { room: r, person: a, status: active }\n        t: Seat { room: r, status: active }",
        &format!("        {clause}"),
    ));
    let (verdict, why) = line(&result, clause);
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(
        why,
        "`Shout` fired, but part of what it ensures was not applied: `for p in room.people` may \
         be missing members it could not decide on (`t` has no `person` set, in `live_seats -> \
         person`), so the clauses inside it were not applied to them — in `room.people`"
    );
}
