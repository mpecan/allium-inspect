//! Who hears an emission, and what binds a listener's names — the walk and
//! `check` giving one answer (issue #12).
//!
//! Against the recorded `allium` output and the real simulator: `newsroom`
//! listens, and `press` `use`s it and emits one of its triggers.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::NEWSROOM;
use inspect_journey::{Verdict, Walk, check, parse, walk};

fn walked(source: &str) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(NEWSROOM);
    assert!(
        check(&journeys[0], &journeys, &graph, &program).is_empty(),
        "check has nothing to say about it"
    );
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

fn holds(walk: &Walk) {
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "{:#?}", step.outcomes);
        assert!(step.asides.is_empty(), "{:#?}", step.asides);
    }
}

/// `press` `use`s `newsroom` and emits `Published(story)`; `newsroom`'s
/// `MarkSeen` waits on it and imports nothing. The emission is heard: a bare
/// name in the importing module may be the imported module's trigger. Heard
/// only from modules a listener `use`s, it was not — and the step read
/// *refused*, the spec saying no, with nothing saying the emission went
/// unheard.
#[test]
fn an_emission_from_a_module_that_uses_the_listeners_is_heard() {
    let walk = walked(
        "journey J {
    cast:
        s: newsroom/Story
    given:
        s.seen = false
    1. it is printed
        s does Print(s) on Pressroom
        then MarkSeen fires
        then s.seen = true
}",
    );
    holds(&walk);
}

/// `Submit` emits `Filed(item: story)` and `File` waits on `Filed(story)`
/// with `let story = item`. The `let` binds `story` before the `when` is read,
/// so nothing is unbound: no diagnostic, no remark from `check`, and the walk
/// fires the listener. The static check skips a parameter its listener's own
/// `let` names on the strength of the walk doing the same; this holds the two
/// to it.
#[test]
fn a_listener_s_own_let_binds_what_its_when_names_in_check_and_in_the_walk() {
    let (graph, _, _) = common::library(NEWSROOM);
    let unbound: Vec<_> = graph
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some(inspect_model::listening::CODE))
        .collect();
    assert!(unbound.is_empty(), "{unbound:#?}");
    let walk = walked(
        "journey J {
    cast:
        s: Story
    given:
        s.filed = false
    1. it is submitted
        s does Submit(s) on Newsdesk
        then File fires
        then s.filed = true
}",
    );
    holds(&walk);
}
