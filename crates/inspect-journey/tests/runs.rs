//! A status derived from words, and compared against the stored one
//! (issue #21).
//!
//! Two faults that showed up together. `agg: if status = waiting: waiting else:
//! running` came back undecided, because the branch it chose was a bare word
//! with nothing beside it to say it was a state. And `stale: agg != status`
//! then came back `true`: the comparison read the undecided name `agg` as the
//! state `agg`, which is not `waiting`. The second is stipulation 1 broken —
//! an answer the simulator never reached — and it made Forgejo's
//! `RunStatusFollowsJobs` fire every time, for a reason that was not so.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::RUNS;
use inspect_journey::{Verdict, Walk, parse, render, walk};
use inspect_sim::{
    Value,
    eval::{Env, field_of},
    world::World,
};

const FILED: &str = include_str!("fixtures/runs.journey");

fn walked(name: &str) -> Walk {
    let journeys = parse(FILED).expect("the journeys parse");
    let journey = journeys
        .iter()
        .find(|journey| journey.name == name)
        .unwrap_or_else(|| panic!("no journey `{name}`"));
    let (graph, program, sources) = common::library(RUNS);
    walk(journey, &journeys, &graph, &program, &sources)
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

#[test]
fn the_issue_as_filed_holds() {
    // `agg_d` and `agg_e` are the state their branch names, written over two
    // lines or one, and `stale` is false because the two agree.
    let result = walked("AsFiled");
    assert_eq!(
        result.steps[0].verdict(),
        Verdict::Specified,
        "{}",
        render(std::slice::from_ref(&result))
    );
}

#[test]
fn a_comparison_with_an_unset_field_is_undecided_and_says_which() {
    // `ne: other != status` with `other` unset was `true`, and nothing on the
    // line said so: `other` was read as the state `other`.
    let result = walked("AnUnsetFieldIsNotAState");
    assert_eq!(result.steps[0].verdict(), Verdict::Undecided);
    // Either way round: `ne_r` is `status != other`.
    for about in ["then r.ne = true", "then r.ne_r = true", "then r.stale_u = true"] {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Undecided, "{about}: {why}");
        // Said about the name the journey cast, not the world's `Run#1`, and
        // without repeating `other` as "— in `other`".
        assert!(why.ends_with("`r` has no `other` set"), "{about} names the side: {why}");
    }
}

#[test]
fn the_issue_s_own_step_is_undecided_on_its_one_undecided_line_only() {
    // The journey exactly as filed: `ne` is the only line that cannot be
    // settled, and the lines either side of it still hold.
    let result = walked("IssueT");
    assert_eq!(result.steps[0].verdict(), Verdict::Undecided);
    for about in ["then r.agg_d = waiting", "then r.agg_e = waiting", "then r.stale = false"] {
        let (verdict, why) = line(&result, about);
        assert_eq!(verdict, Verdict::Specified, "{about}: {why}");
    }
    let (verdict, why) = line(&result, "then r.ne = true");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(why, "r.ne is unknown — `r` has no `other` set");
}

#[test]
fn a_run_nobody_started_is_neither_running_nor_not() {
    // `running_runs: runs where status = running` on a pipeline, over a run
    // with no `status`. `status` there is the run's field, unset — read as the
    // state `status` it was a definite "not running", and `has_running` a
    // definite false.
    let result = walked("APipelineWithARunNobodyStarted");
    let (verdict, why) = line(&result, "then p.has_running = false");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    let started = walked("APipelineWithARunningRun");
    assert_eq!(
        started.steps[0].verdict(),
        Verdict::Specified,
        "{}",
        render(std::slice::from_ref(&started))
    );
}

#[test]
fn an_unset_field_is_anchored_on_its_own_bytes() {
    // The header comments carry em-dashes, so a span counted in characters
    // or UTF-16 units lands short of `other` and highlights something else.
    let (_, program, sources) = common::library(RUNS);
    let source = sources.get("runs").expect("the runs source");
    let mut world = World::new();
    let run = world.create("Run", "runs");
    world.set_field(&run, "status", Value::Enum("waiting".to_owned()));
    let instance = world.instance(&run).expect("the run").clone();
    let env = Env::new(&world, "runs", source).deriving(program.derivations());
    let read = field_of(&instance, "ne", &env);
    assert!(read.value.is_unknown());
    let note = read
        .unresolved
        .iter()
        .find(|note| note.reason == format!("`{run}` has no `other` set"))
        .expect("a note about `other`");
    let span = note.span.expect("a span");
    let at = source.find("ne: other").expect("`ne` in the source") + "ne: ".len();
    assert!(!source[..at].is_ascii(), "something wider than a byte comes first");
    assert_eq!((span.start, span.end), (at, at + "other".len()));
    assert_eq!(span.slice(source), Some("other"));
}

#[test]
fn a_rule_guarded_by_a_stale_status_fires_when_the_two_disagree() {
    let result = walked("FollowsWhenStale");
    assert_eq!(
        result.steps[0].verdict(),
        Verdict::Specified,
        "{}",
        render(std::slice::from_ref(&result))
    );
}

#[test]
fn a_rule_guarded_by_a_stale_status_is_refused_when_the_two_agree() {
    // The Forgejo symptom: it fired every time. It is refused, and for the
    // clause that is false.
    let result = walked("StaysWhenNotStale");
    let (verdict, why) = line(&result, "r does Follow(r) on Runner");
    assert_eq!(verdict, Verdict::Refused, "{why}");
    assert!(why.contains("run.stale"), "{why}");
}

#[test]
fn a_rule_guarded_by_an_undecided_staleness_is_undecided_and_says_why() {
    let result = walked("CannotTellWhetherStale");
    let (verdict, why) = line(&result, "r does FollowOther(r) on Runner");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("`Run#1` has no `other` set"), "{why}");
}

#[test]
fn a_reason_that_does_not_quote_its_expression_still_says_where() {
    // The "— in `x`" suffix is dropped only when the reason already quotes
    // `x`. `is_at_limit`'s reason is about a count, and says nothing of
    // `open_loans` itself — without the suffix the line would not say which
    // collection it meant. `is_late`'s names `window`, which is the
    // expression, so the suffix would only repeat it.
    let journeys = parse(
        "journey J {
    cast:
        l: Loan
        m: Member
    1. look
        then l.is_late = false
        then m.is_at_limit = false
}",
    )
    .expect("the journey parses");
    let (graph, program, sources) = common::library(common::LENDING);
    let result = walk(&journeys[0], &journeys, &graph, &program, &sources);
    let (verdict, why) = line(&result, "then m.is_at_limit = false");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.ends_with("— in `open_loans`"), "says which collection: {why}");
    let (verdict, why) = line(&result, "then l.is_late = false");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert_eq!(why, "l.is_late is unknown — `l` has no `window` set");
}
