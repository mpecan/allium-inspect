//! Walking journeys against the grants fixture: literals a journey has to be
//! able to say back to a spec (issue #10).
//!
//! Two of them. A set in `given:`, which was refused as a cast entry because
//! its brace looked like an instance's. And a word nothing binds, passed to a
//! black box — `can(repo, code)` — which a journey stipulated and the walk
//! still reported as "a function call is not simulated", because the spec's
//! `code` was nothing bound and the journey's was a state.
//!
//! Stipulation 1 is the edge of both: a word is read as the state it spells
//! only where something says so — the journey's own stipulation in that
//! position, or the spec declaring it as a state — and everything else stays
//! undecided with a reason that names it.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::GRANTS;
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked_nth(source: &str, at: usize) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(GRANTS);
    walk(&journeys[at], &journeys, &graph, &program, &sources)
}

fn walked(source: &str) -> Walk {
    walked_nth(source, 0)
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

/// One repo, `given` lines, then one step of `clauses`.
fn journey(given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
        r: Repo
        other: Repo
    given:
        r.flag = false
        r.ended = false
{given}
    1. the step
{clauses}
}}"
    )
}

const FILED: &str = include_str!("fixtures/grants.journey");

#[test]
fn the_issue_as_filed_holds() {
    for at in 0..2 {
        let result = walked_nth(FILED, at);
        let every = outcomes(&result);
        assert!(
            every.iter().all(|(verdict, _, _)| *verdict == Verdict::Specified),
            "journey {at}: every line holds: {every:#?}"
        );
        assert_eq!(result.verdict(), Verdict::Specified);
    }
}

mod a_set_in_given {
    use super::*;

    fn tagged(tags: &str) -> Walk {
        walked(&journey(
            &format!("        r.tags = {tags}"),
            "        r does Tag(r) on Settings\n        then r.flag = true",
        ))
    }

    #[test]
    fn is_what_the_rule_reads() {
        let result = tagged("{\"a\", \"b\"}");
        assert_eq!(line(&result, "r does Tag(r) on Settings").0, Verdict::Specified);
        assert_eq!(line(&result, "then r.flag = true").0, Verdict::Specified);
    }

    /// Read as a set and not as anything that would satisfy the rule whatever
    /// it held: `"a"` is not in `{"b"}`, and the spec says no.
    #[test]
    fn without_the_member_the_rule_is_refused() {
        for tags in ["{\"b\"}", "{}"] {
            let result = tagged(tags);
            let (verdict, detail) = line(&result, "r does Tag(r) on Settings");
            assert_eq!(verdict, Verdict::Refused, "{tags}: {detail:?}");
        }
    }
}

mod a_black_box_called_with_a_word {
    use super::*;

    fn granted(stipulation: &str) -> Walk {
        walked(&journey(
            "",
            &format!(
                "        {stipulation}\n        r does Grant(r) on Settings\n        then r.flag = true"
            ),
        ))
    }

    #[test]
    fn is_answered_by_the_stipulation_that_names_the_same_word() {
        let result = granted("stipulate can(r, code) = true");
        assert_eq!(line(&result, "r does Grant(r) on Settings").0, Verdict::Specified);
        assert_eq!(line(&result, "then r.flag = true").0, Verdict::Specified);
    }

    /// Matched, and then believed: a stipulation of `false` is the spec saying
    /// no, not the simulator failing to tell.
    #[test]
    fn a_false_answer_refuses_the_rule() {
        let result = granted("stipulate can(r, code) = false");
        let (verdict, detail) = line(&result, "r does Grant(r) on Settings");
        assert_eq!(verdict, Verdict::Refused, "{detail:?}");
    }

    /// The word is part of the call. Saying what `can(r, issues)` comes back
    /// as says nothing about `can(r, code)`, and answering it would be the
    /// tool inventing a policy nobody stated.
    #[test]
    fn a_stipulation_about_another_word_does_not_answer_it() {
        let result = granted("stipulate can(r, issues) = true");
        let (verdict, detail) = line(&result, "r does Grant(r) on Settings");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
        let why = detail.expect("a reason");
        assert!(why.contains("can(repo, code)"), "names the call, in the spec's words: {why}");
    }

    #[test]
    fn a_stipulation_about_another_repo_does_not_answer_it() {
        let result = granted("stipulate can(other, code) = true");
        assert_eq!(line(&result, "r does Grant(r) on Settings").0, Verdict::Undecided);
    }

    /// Undecided, and saying what would decide it — in the words the
    /// reference quotes, so the page and the walk cannot drift apart.
    #[test]
    fn with_no_stipulation_it_is_undecided_and_says_what_would_answer_it() {
        const SAID: &str = "`Grant` could not be decided: nothing defines `can`, and no \
                            stipulation answers this call — in `can(repo, code)`";
        let result = walked(&journey("", "        r does Grant(r) on Settings"));
        let (verdict, detail) = line(&result, "r does Grant(r) on Settings");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
        assert_eq!(detail.as_deref(), Some(SAID));
        let reference = include_str!("../../../docs/journeys/reference.md");
        assert!(
            reference.contains("nothing defines `can`, and no stipulation answers this call"),
            "the reference quotes the walk"
        );
    }

    /// Inside the entity's own definition, `level` is a field nobody set —
    /// an unknown number, not a state spelled `level`. A stipulation about the
    /// spelling answering it would be a definite answer about a value nobody
    /// knows. It held, before.
    #[test]
    fn a_field_nobody_set_is_not_the_word_it_spells() {
        let result = walked(&journey(
            "",
            "        stipulate can(r, level) = true\n        r does Check(r) on Settings\n        then r.flag = true",
        ));
        let (verdict, detail) = line(&result, "r does Check(r) on Settings");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
        assert!(detail.expect("a reason").contains("can(this, level)"));
        assert_eq!(result.verdict(), Verdict::Undecided);
    }

    /// `Poke` emits `Ask(repo)` and never passes `kind`. `kind` is a question
    /// about what would have been passed, and the stipulation spelling it does
    /// not answer it.
    #[test]
    fn a_parameter_nobody_passed_is_not_the_word_it_spells() {
        let result = walked(&journey(
            "",
            "        stipulate can(r, kind) = true\n        r does Poke(r) on Settings\n        then r.flag = true",
        ));
        let (verdict, detail) = line(&result, "r does Poke(r) on Settings");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
        assert!(detail.expect("a reason").contains("`Ask` could not be decided"));
        assert_eq!(result.verdict(), Verdict::Undecided);
    }
}

mod a_word_passed_down_a_chain {
    use super::*;

    /// `Stop` emits `Finish(repo, cancel)`, and `cancel` is a state the spec
    /// declares. `Finish` asks `how = cancel` of what it was handed — which
    /// was nothing at all, before, so the chained rule was undecided and the
    /// step reported the repo as never ended.
    #[test]
    fn a_declared_state_arrives_as_that_state() {
        let result = walked(&journey(
            "",
            "        r does Stop(r) on Settings\n        then Finish fires\n        then r.ended = true",
        ));
        let every = outcomes(&result);
        assert!(
            every.iter().all(|(verdict, _, _)| *verdict == Verdict::Specified),
            "every line holds: {every:#?}"
        );
    }

    /// The same word reaching a black box one rule further on, where the
    /// journey's stipulation answers it.
    #[test]
    fn and_answers_a_stipulation_one_rule_further_on() {
        let holds = walked(&journey(
            "",
            "        stipulate may_close(r, cancel) = true\n        r does Close(r) on Settings\n        then r.ended = true",
        ));
        assert_eq!(
            line(&holds, "then r.ended = true").0,
            Verdict::Specified,
            "{:#?}",
            outcomes(&holds)
        );

        let other_word = walked(&journey(
            "",
            "        stipulate may_close(r, fail) = true\n        r does Close(r) on Settings\n        then r.ended = true",
        ));
        let (verdict, detail) = line(&other_word, "then r.ended = true");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    }

    /// `abort` is no state the spec declares, so nothing says it is one and it
    /// is not read as one. The chained rule is undecided — and so is the act
    /// that set it off, though its own rule fired: what follows from it was not
    /// decided. A refusal after it in the step is not the spec refusing.
    #[test]
    fn an_undeclared_word_leaves_the_act_undecided_and_says_which_rule_and_why() {
        let result =
            walked(&journey("", "        r does Halt(r) on Settings\n        then r.ended = true"));
        let (verdict, detail) = line(&result, "r does Halt(r) on Settings");
        assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
        assert_eq!(
            detail.as_deref(),
            Some(
                "what it set off: `Halted` could not be decided: nothing is bound to `abort` — in \
                 `abort`"
            )
        );
        assert_eq!(line(&result, "then r.ended = true").0, Verdict::Undecided);
    }

    /// The other side of the same hole. `Halted` might have ended the repo,
    /// so the repo still not having ended is no more decided than it having
    /// ended — and a step that said so read as holding.
    #[test]
    fn a_then_that_would_hold_either_way_does_not_make_the_step_hold() {
        let result = walked(&journey(
            "",
            "        r does Halt(r) on Settings\n        then r.ended = false",
        ));
        assert_eq!(result.steps[0].verdict(), Verdict::Undecided, "{:#?}", outcomes(&result));
    }

    /// `then … fires` about it says why, not only that.
    #[test]
    fn then_fires_says_why_the_rule_could_not_be_decided() {
        let result =
            walked(&journey("", "        r does Halt(r) on Settings\n        then Halted fires"));
        let (verdict, detail) = line(&result, "then Halted fires");
        assert_eq!(verdict, Verdict::Undecided);
        assert!(detail.expect("a reason").contains("nothing is bound to `abort`"));
    }
}

/// What the engine could not decide belongs to the step whose act set it off.
/// A later step that says what is so and is refused by it has been refused by
/// the journey's own words, and blurring that into undecided would hide a
/// definite answer behind a rule from somewhere else.
#[test]
fn a_rule_undecided_in_one_step_does_not_soften_a_refusal_in_the_next() {
    let result = walked(
        "journey J {
    cast:
        r: Repo
    given:
        r.ended = false
    1. halt it
        r does Halt(r) on Settings
    2. say what is so, and be refused by it
        stipulate r.ended = false
        then r.ended = true
}",
    );
    let (verdict, detail) = line(&result, "then r.ended = true");
    assert_eq!(verdict, Verdict::Refused, "{detail:?}");
    // And the step that acted is the one that says something was undecided.
    assert_eq!(result.steps[0].verdict(), Verdict::Undecided);
}
