//! Reading a journey file.
//!
//! Driven against the file a person would actually write rather than against
//! fragments, because the parts that break are the joins: a clause that runs on
//! to the next line, a step whose number is missing, prose that happens to
//! contain the word `on`.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use inspect_journey::{Assertion, Clause, Comparison, Given, Journey, Term, parse};
use inspect_sim::Value;

const LENDING: &str = include_str!("fixtures/lending.journey");

fn lending() -> Vec<Journey> {
    parse(LENDING).expect("the fixture parses")
}

fn first() -> Journey {
    lending().into_iter().next().expect("a journey")
}

#[test]
fn a_file_may_hold_more_than_one_journey() {
    // A branch is a second journey, so a file of them is the ordinary case
    // rather than the exception.
    let journeys = lending();
    assert_eq!(journeys.len(), 2);
    assert_eq!(journeys[0].name, "ACopyGoesOutAndComesBack");
    assert_eq!(journeys[1].name, "ACopyNobodyBringsBack");
}

#[test]
fn the_goal_runs_across_lines_and_keeps_its_words() {
    let journey = first();
    assert_eq!(journey.goal.len(), 2);
    assert!(journey.goal[0].starts_with("Ada borrows a copy"));
    assert!(journey.goal[1].ends_with("falls due."));
}

#[test]
fn the_cast_is_instances_with_the_types_the_spec_names() {
    // Qualified where the spec qualifies it. Resolving `catalogue/Copy` is the
    // checker's job; keeping it as written is this one's.
    let journey = first();
    let named: Vec<(&str, &str)> = journey
        .cast
        .iter()
        .map(|member| (member.name.as_str(), member.type_expr.as_str()))
        .collect();
    assert_eq!(named, [("ada", "Member"), ("copy", "catalogue/Copy")]);
}

#[test]
fn given_assigns_fields_before_anything_happens() {
    let journey = first();
    assert_eq!(journey.given.len(), 3);
    let Given::Assign { path, value, .. } = &journey.given[0] else {
        panic!("expected an assignment, got {:?}", journey.given[0]);
    };
    assert_eq!(path.as_written(), "ada.name");
    assert_eq!(*value, Term::Literal(Value::Str("Ada".to_owned())));
}

#[test]
fn a_step_keeps_its_number_and_its_sentence() {
    // The number is required and never renumbered: other documents cite journey
    // steps by number, and one that shifts is a citation pointing elsewhere.
    let journey = first();
    let headers: Vec<(u32, &str)> =
        journey.steps.iter().map(|step| (step.number, step.title.as_str())).collect();
    assert_eq!(
        headers,
        [
            (1, "she borrows it"),
            (2, "a fortnight passes and nothing falls due"),
            (3, "she brings it back"),
        ]
    );
}

#[test]
fn an_act_names_its_actor_its_trigger_its_surface_and_what_it_caught() {
    let journey = first();
    let Clause::Does { actor, trigger, arguments, surface, creating, .. } =
        &journey.steps[0].clauses[0]
    else {
        panic!("expected an act");
    };
    assert_eq!(actor, "ada");
    assert_eq!(trigger, "MemberBorrows");
    assert_eq!(surface, "MemberShelf");
    assert_eq!(arguments.len(), 2);
    let caught = creating.as_ref().expect("the step caught a loan");
    assert_eq!((caught.name.as_str(), caught.type_expr.as_str()), ("loan", "Loan"));
}

#[test]
fn an_act_that_catches_nothing_says_so_by_omission() {
    let journey = first();
    let Clause::Does { creating, trigger, .. } = &journey.steps[2].clauses[0] else {
        panic!("expected an act");
    };
    assert_eq!(trigger, "MemberReturns");
    assert!(creating.is_none());
}

#[test]
fn the_clock_advances_by_a_duration_and_keeps_the_words() {
    // `21.days` and `3.weeks` are the same number of milliseconds, and the
    // report quotes what the author wrote rather than what the number renders
    // back as.
    let journeys = lending();
    let Clause::After { duration, text, .. } = &journeys[1].steps[1].clauses[0] else {
        panic!("expected the clock to move");
    };
    assert_eq!(*duration, Value::Duration(21 * 86_400_000));
    assert_eq!(text, "21.days");
}

#[test]
fn an_assertion_compares_a_path_with_a_value() {
    let journey = first();
    let Clause::Then { assertion: Assertion::Compare { left, operator, right }, .. } =
        &journey.steps[0].clauses[2]
    else {
        panic!("expected a comparison");
    };
    assert_eq!(left.as_written(), "loan.status");
    assert_eq!(*operator, Comparison::Equal);
    assert_eq!(right.as_written(), "open");
}

#[test]
fn an_assertion_about_a_rule_reads_as_one() {
    let journey = first();
    let Clause::Then { assertion, .. } = &journey.steps[0].clauses[1] else {
        panic!("expected an assertion");
    };
    assert_eq!(*assertion, Assertion::Fires { rule: "BorrowCopy".to_owned(), negated: false });

    let Clause::Then { assertion, .. } = &journey.steps[1].clauses[1] else {
        panic!("expected an assertion");
    };
    assert_eq!(*assertion, Assertion::Fires { rule: "LoanFallsOverdue".to_owned(), negated: true });
}

#[test]
fn seeing_names_one_value_on_one_surface() {
    // Not "somewhere on screen". A step asking whether a word was anywhere would
    // be answered by the wrong thing and pass.
    let journey = first();
    let Clause::Sees { actor, path, surface, negated, .. } = &journey.steps[0].clauses[4] else {
        panic!("expected an observation");
    };
    assert_eq!(
        (actor.as_str(), path.as_written().as_str(), surface.as_str()),
        ("ada", "loan.status", "MemberShelf")
    );
    assert!(!negated);
}

#[test]
fn the_ending_is_prose_and_stays_prose() {
    let journey = first();
    assert_eq!(journey.ends, ["The copy is back on the shelf and Ada owes nothing."]);
}

// --- what a file gets wrong -------------------------------------------------
//
// A journey is written by somebody describing what a person should be able to
// do, and often by an agent. Every message here names the line and says what
// was expected, because the alternative is a tool that says "syntax error" to
// something that is one comma from working.

fn refuse(source: &str) -> String {
    parse(source).expect_err("this should not parse").to_string()
}

#[test]
fn a_journey_that_is_never_closed_says_which_one() {
    let error = refuse("journey Unfinished {\n    goal: nothing\n");
    assert!(error.contains("line 1"), "{error}");
    assert!(error.contains("Unfinished"), "{error}");
    assert!(error.contains("never closed"), "{error}");
}

#[test]
fn a_step_without_a_number_is_not_a_step() {
    // Which makes it a clause, and there is no clause there — so the error is
    // about the line somebody actually wrote rather than about a missing one.
    let error = refuse("journey J {\n    she borrows it\n}\n");
    assert!(error.contains("line 2"), "{error}");
}

#[test]
fn an_act_with_no_surface_says_so() {
    // An act happens somewhere. A trigger fired at no boundary is a system
    // event, and the whole point of a journey is that a person did it.
    let error =
        refuse("journey J {\n    1. she borrows it\n        ada does MemberBorrows(ada)\n}\n");
    assert!(error.contains("line 3"), "{error}");
    assert!(error.contains("on <Surface>"), "{error}");
}

#[test]
fn an_act_with_unclosed_arguments_says_so() {
    let error =
        refuse("journey J {\n    1. x\n        ada does MemberBorrows(ada on MemberShelf\n}\n");
    assert!(error.contains("closed with `)`"), "{error}");
}

#[test]
fn a_clause_that_asserts_nothing_says_so() {
    let error = refuse("journey J {\n    1. x\n        then loan\n}\n");
    assert!(error.contains("asserts nothing"), "{error}");
}

#[test]
fn an_after_that_is_not_a_duration_says_so() {
    // `after 3` is a number, not a length of time, and the difference is the
    // whole of what the clause does.
    let error = refuse("journey J {\n    1. x\n        after 3\n}\n");
    assert!(error.contains("not a duration"), "{error}");
}

#[test]
fn a_clause_written_before_any_step_is_refused_rather_than_absorbed() {
    // Prose runs on across lines, so this would otherwise become another
    // sentence of the goal — and a journey that runs with fewer assertions than
    // somebody wrote is the false green this design exists to refuse.
    let error = refuse("journey J {\n    goal: g\n    then loan.status = open\n}\n");
    assert!(error.contains("line 3"), "{error}");
    assert!(error.contains("belongs under a step"), "{error}");
}

#[test]
fn prose_that_merely_reads_like_a_clause_is_still_prose() {
    // "she sees" and "he does" are how anybody writes a goal, and only the
    // three words that open a clause and nothing else are refused.
    let journeys =
        parse("journey J {\n    goal: she sees it and he does too\n}\n").expect("this is prose");
    assert_eq!(journeys[0].goal, ["she sees it and he does too"]);
}

#[test]
fn a_file_that_does_not_start_with_a_journey_says_what_was_expected() {
    let error = refuse("cast:\n    ada: Member\n");
    assert!(error.contains("journey <Name>"), "{error}");
}

// --- the shapes of a value ---------------------------------------------------

fn only_clause(text: &str) -> Clause {
    let source = format!("journey J {{\n    1. x\n        {text}\n}}\n");
    let mut journeys = parse(&source).expect("parses");
    journeys.remove(0).steps.remove(0).clauses.remove(0)
}

#[test]
fn the_comparison_operators_are_read_longest_first() {
    // `<=` must not be read as `<` with a stray `=`, and `!=` must not be read
    // as a bare `=` — which is the classic way a hand-written operator table
    // silently inverts an assertion.
    for (written, expected) in [
        ("=", Comparison::Equal),
        ("!=", Comparison::NotEqual),
        ("<", Comparison::Less),
        ("<=", Comparison::LessOrEqual),
        (">", Comparison::Greater),
        (">=", Comparison::GreaterOrEqual),
    ] {
        let Clause::Then { assertion: Assertion::Compare { operator, .. }, .. } =
            only_clause(&format!("then member.open_loan_count {written} 5"))
        else {
            panic!("expected a comparison for `{written}`");
        };
        assert_eq!(operator, expected, "`{written}` was read as `{}`", operator.as_str());
    }
}

#[test]
fn a_set_of_things_already_named_is_a_set() {
    let Clause::Does { arguments, .. } =
        only_clause("ada does PersonDeletes(ada, {note, other}) on Conversation")
    else {
        panic!("expected an act");
    };
    assert_eq!(arguments.len(), 2, "the comma inside the braces is not an argument break");
    assert_eq!(arguments[1].as_written(), "{note, other}");
}

#[test]
fn a_bare_word_stays_a_name_for_the_walker_to_settle() {
    // `open` is a state a spec declares and `copy` is somebody the journey
    // cast, and they are the same shape. A parser that guessed would hand the
    // *name* `copy` to a rule expecting the copy — so both stay names, and the
    // walker, which knows what the journey has bound, decides.
    let Clause::Then { assertion: Assertion::Compare { right, .. }, .. } =
        only_clause("then loan.status = open")
    else {
        panic!("expected a comparison");
    };
    assert_eq!(
        right,
        Term::Path(inspect_journey::Path { root: "open".to_owned(), segments: Vec::new() })
    );

    let Clause::Then { assertion: Assertion::Compare { right, .. }, .. } =
        only_clause("then loan.status = other.status")
    else {
        panic!("expected a comparison");
    };
    assert_eq!(right.as_written(), "other.status");
}

#[test]
fn a_written_literal_is_still_a_literal() {
    // Only the shapes that cannot be a name: quoted text, numbers, durations,
    // and the three words the language spells out.
    for (written, expected) in [
        ("\"Ada\"", Value::Str("Ada".to_owned())),
        ("5", Value::Int(5)),
        ("21.days", Value::Duration(21 * 86_400_000)),
        ("true", Value::Bool(true)),
        ("null", Value::Null),
    ] {
        let Clause::Then { assertion: Assertion::Compare { right, .. }, .. } =
            only_clause(&format!("then a.b = {written}"))
        else {
            panic!("expected a comparison for `{written}`");
        };
        assert_eq!(right, Term::Literal(expected), "`{written}`");
    }
}

#[test]
fn a_comment_is_dropped_wherever_it_sits() {
    let Clause::Then { assertion, .. } = only_clause("then loan.status = open  -- for now") else {
        panic!("expected an assertion");
    };
    assert_eq!(
        assertion,
        Assertion::Compare {
            left: inspect_journey::Path {
                root: "loan".to_owned(),
                segments: vec!["status".to_owned()]
            },
            operator: Comparison::Equal,
            right: Term::Path(inspect_journey::Path {
                root: "open".to_owned(),
                segments: Vec::new(),
            }),
        }
    );
}

#[test]
fn a_wrapped_clause_is_one_clause() {
    // The act, the surface and what it caught, on three lines, because one line
    // with all of it is not something anybody reads.
    let source = "journey J {\n    1. x\n        ada does MemberBorrows(ada, copy)\n            on MemberShelf\n            creating loan: Loan\n}\n";
    let journeys = parse(source).expect("parses");
    assert_eq!(journeys[0].steps[0].clauses.len(), 1);
    let Clause::Does { surface, creating, .. } = &journeys[0].steps[0].clauses[0] else {
        panic!("expected an act");
    };
    assert_eq!(surface, "MemberShelf");
    assert_eq!(creating.as_ref().expect("caught").name, "loan");
}

#[test]
fn a_line_at_the_same_indent_is_the_next_clause_and_not_a_continuation() {
    let source = "journey J {\n    1. x\n        then a.b = open\n        then c.d = open\n}\n";
    let journeys = parse(source).expect("parses");
    assert_eq!(journeys[0].steps[0].clauses.len(), 2);
}
