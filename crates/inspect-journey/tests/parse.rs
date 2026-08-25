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

#[test]
fn two_dashes_inside_a_string_are_text_rather_than_a_comment() {
    // The comment cut ran before anything knew about quotes, so a title with a
    // dash in it lost its closing brace and was reported as a malformed field
    // list — an error about the shape of the line, when the fault was a dash
    // inside a literal. An em-dash in a title is not exotic; it is how people
    // write.
    let journeys = parse(
        "journey J {\n    given:\n        book: catalogue/Book { title: \"Ada -- a life\" }\n}",
    )
    .expect("a dash inside a string does not end the line");

    let Given::Instance { fields, .. } = &journeys[0].given[0] else {
        panic!("an instance: {:?}", journeys[0].given[0]);
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, "title");
    assert_eq!(fields[0].1, Term::Literal(Value::Str("Ada -- a life".to_owned())));
}

#[test]
fn one_dash_is_not_a_comment() {
    // It takes two. A hyphen in a step title is ordinary English, and reading
    // it as the start of a comment silently truncates the title at the word —
    // the step still walks, and its heading has quietly lost half of itself.
    let journeys =
        parse("journey J {\n    1. she re-shelves it\n        after 1.day\n}").expect("parses");
    assert_eq!(journeys[0].steps[0].title, "she re-shelves it");
}

#[test]
fn a_real_comment_after_a_string_still_ends_the_line() {
    // The other direction, so the fix above cannot be "never cut at all".
    let journeys = parse(
        "journey J {\n    given:\n        book: catalogue/Book { title: \"Ada\" }  -- her life\n}",
    )
    .expect("parses");

    let Given::Instance { fields, .. } = &journeys[0].given[0] else {
        panic!("an instance");
    };
    assert_eq!(fields[0].1, Term::Literal(Value::Str("Ada".to_owned())));
}

#[test]
fn a_cast_member_needs_both_halves() {
    // Half a cast line binds a name to nothing or nothing to a type, and every
    // clause that mentions it afterwards then reads against a hole. Refusing
    // the file is the only answer that leaves the author somewhere to look.
    for text in ["ada:", ": Member", ":"] {
        let message = refuse(&format!("journey J {{\n    cast:\n        {text}\n}}"));
        assert!(message.contains("name and a type"), "{text}: {message}");
    }
}

#[test]
fn a_given_carries_the_fields_it_was_written_with() {
    // The instance form. Dropping the fields would leave an instance of the
    // right type with none of the state the journey said it starts in, and
    // every later assertion about that state would answer undecided.
    let journeys =
        parse("journey J {\n    given:\n        note: Message { author: ada, body: \"hello\" }\n}")
            .expect("it parses");
    let Some(Given::Instance { name, type_expr, fields, .. }) = journeys[0].given.first() else {
        panic!("a given instance: {:#?}", journeys[0].given)
    };
    assert_eq!(name, "note");
    assert_eq!(type_expr, "Message");
    assert_eq!(fields.len(), 2, "{fields:#?}");
    assert_eq!(fields[0].0, "author");
    assert_eq!(fields[1], ("body".to_owned(), Term::Literal(Value::Str("hello".to_owned()))));
}

#[test]
fn an_empty_field_list_is_an_instance_with_no_state_rather_than_an_error() {
    let journeys =
        parse("journey J {\n    given:\n        note: Message { }\n}").expect("it parses");
    let Some(Given::Instance { fields, .. }) = journeys[0].given.first() else {
        panic!("a given instance")
    };
    assert!(fields.is_empty());
}

#[test]
fn a_brace_closes_and_the_comma_after_it_separates_again() {
    // `X(copy, {a, b}, member)` is three arguments. A splitter that opened on
    // `{` and never closed would swallow everything after it into the second,
    // and the trigger would be called with two arguments where the spec
    // declares three — silently, and matched positionally.
    let Clause::Does { arguments, .. } =
        only_clause("ada does Post(copy, {one, two}, ada) on Wall")
    else {
        panic!("an act")
    };
    assert_eq!(arguments.len(), 3, "{arguments:#?}");
    assert_eq!(arguments[0], Term::Path(path_of("copy")));
    assert_eq!(arguments[2], Term::Path(path_of("ada")));
    let Term::Set(items) = &arguments[1] else { panic!("a set: {:#?}", arguments[1]) };
    assert_eq!(items.len(), 2);
}

/// A bare name, as `term` would read it.
fn path_of(name: &str) -> inspect_journey::Path {
    let Clause::Then { assertion: Assertion::Compare { left, .. }, .. } =
        only_clause(&format!("then {name} = x"))
    else {
        panic!("a comparison")
    };
    left
}

#[test]
fn an_assignment_keeps_everything_to_the_right_of_the_equals() {
    // Off by one either way and the value carries the operator with it, which
    // parses as a name rather than failing — so the field is set to something
    // spelled `= "Ada"` and every comparison against it quietly says no.
    let journeys =
        parse("journey J {\n    given:\n        ada.name = \"Ada\"\n}").expect("it parses");
    assert_eq!(
        journeys[0].given.first(),
        Some(&Given::Assign {
            path: path_of("ada.name"),
            value: Term::Literal(Value::Str("Ada".to_owned())),
            line: 3,
        })
    );
}

#[test]
fn a_given_written_as_a_comparison_is_refused_rather_than_split() {
    // `given` states what is so, and `>=` states a range. Splitting on the `=`
    // inside it would bind a field named `ada.open_loan_count >` — a name
    // nothing will ever read, so the journey would run with that precondition
    // silently absent.
    for text in ["ada.open_loan_count >= 3", "ada.name != \"Ada\"", "ada.count <= 3"] {
        let message = refuse(&format!("journey J {{\n    given:\n        {text}\n}}"));
        assert!(message.contains("expected"), "{text}: {message}");
    }
}

#[test]
fn a_given_line_knows_which_line_it_was_on() {
    // What the report points at. Every given on line zero sends the author to
    // the top of the file for a fault three screens down.
    let journeys =
        parse("journey J {\n    given:\n        ada.name = \"Ada\"\n        note: Message { }\n}")
            .expect("it parses");
    let lines: Vec<usize> = journeys[0].given.iter().map(Given::line).collect();
    assert_eq!(lines, vec![3, 4]);
}

#[test]
fn a_comparison_spelled_the_way_a_programmer_types_it_is_refused() {
    // `==` is not this grammar, and splitting on the first `=` inside it would
    // read the second one as the start of the value — an assertion comparing a
    // path against something spelled `= open`, which is not what anybody wrote
    // and would quietly answer no forever.
    for text in ["then loan.status == open", "then a == b"] {
        let message = refuse(&format!("journey J {{\n    1. she looks\n        {text}\n}}"));
        assert!(message.contains("asserts nothing"), "{text}: {message}");
    }
}

#[test]
fn a_comparison_the_grammar_does_have_still_reads() {
    // So the refusal above is a refusal of `==` rather than of comparisons.
    let Clause::Then { assertion: Assertion::Compare { operator, right, .. }, .. } =
        only_clause("then loan.status = open")
    else {
        panic!("a comparison")
    };
    assert_eq!(operator, Comparison::Equal);
    assert_eq!(right, Term::Path(path_of("open")));
}

/// The ways a journey says it should be shown.
///
/// Declaring them is the difference between evidence a harness happened to
/// produce and evidence the journey asked for: the panel can offer the control
/// before a single picture exists, and a tag outside the declaration is
/// something to report rather than a second axis nobody meant.
mod shows {
    use inspect_journey::parse;

    fn one(body: &str) -> Result<Vec<inspect_journey::Journey>, inspect_journey::ParseError> {
        parse(&format!(
            "journey Reading {{\n    goal: something\n{body}\n    1. she looks\n        then a.b = c\n}}\n"
        ))
    }

    #[test]
    fn a_journey_declares_the_ways_it_should_be_shown() {
        let journeys = one("    shows:\n        theme: dark, light").expect("it parses");
        let axis = journeys[0].shows.first().expect("one axis");

        assert_eq!(axis.key, "theme");
        assert_eq!(axis.values, ["dark", "light"]);
        assert_eq!(axis.line, 4);
    }

    #[test]
    fn a_journey_may_declare_several() {
        let journeys =
            one("    shows:\n        theme: dark, light\n        platform: ios, android")
                .expect("it parses");
        let keys: Vec<&str> = journeys[0].shows.iter().map(|a| a.key.as_str()).collect();

        // In the order they were written: the author chose which question a
        // reader meets first, and sorting them would take that away.
        assert_eq!(keys, ["theme", "platform"]);
    }

    #[test]
    fn declaring_nothing_is_the_ordinary_case() {
        let journeys = one("").expect("it parses");
        assert!(journeys[0].shows.is_empty());
    }

    /// A control offering one option is a control that does nothing.
    #[test]
    fn an_axis_with_one_value_is_refused() {
        let error = one("    shows:\n        theme: dark").expect_err("it must refuse");
        assert!(error.to_string().contains("at least two values"), "{error}");
    }

    #[test]
    fn an_axis_with_no_values_is_refused() {
        assert!(one("    shows:\n        theme:").is_err());
    }

    #[test]
    fn an_axis_with_no_name_is_refused() {
        let error = one("    shows:\n        : dark, light").expect_err("it must refuse");
        assert!(error.to_string().contains("needs a name"), "{error}");
    }

    #[test]
    fn a_line_that_is_not_an_axis_at_all_is_refused() {
        let error = one("    shows:\n        just some words").expect_err("it must refuse");
        assert!(error.to_string().contains("<name>: <value>"), "{error}");
    }

    /// Two identical options in one dropdown is a typo, and it would be a
    /// dropdown where one of the two entries could never be reached.
    #[test]
    fn an_axis_listing_a_value_twice_is_refused() {
        let error =
            one("    shows:\n        theme: dark, light, dark").expect_err("it must refuse");
        assert!(error.to_string().contains("same value twice"), "{error}");
    }

    #[test]
    fn an_error_names_the_line_it_is_on() {
        let error = one("    shows:\n        theme: dark").expect_err("it must refuse");
        assert_eq!(error.line, 4);
    }

    #[test]
    fn the_block_ends_where_the_next_one_begins() {
        let journeys = parse(
            "journey Reading {\n    shows:\n        theme: dark, light\n\n    cast:\n        ada: Member\n\n    1. she looks\n        then a.b = c\n}\n",
        )
        .expect("it parses");

        assert_eq!(journeys[0].shows.len(), 1);
        assert_eq!(journeys[0].cast.len(), 1);
    }
}

/// Naming a moment.
///
/// The one arithmetic in the grammar, and the reason it exists: a rule guarded
/// by `requires: x.expires_at > now` cannot be reached from a world where
/// `expires_at` holds an integer, because an integer cannot be ordered against
/// a timestamp. Before this, every rule with a deadline in it was unreachable.
mod clock {
    use inspect_journey::{Given, Term, parse};

    fn given_value(written: &str) -> Term {
        let source = format!(
            "journey Reading {{\n    goal: x\n    given:\n        inv.expires_at = {written}\n\n    1. a step\n        then inv.status = live\n}}\n"
        );
        let journeys = parse(&source).unwrap_or_else(|error| panic!("`{written}`: {error}"));
        match journeys[0].given.first().expect("one given") {
            Given::Assign { value, .. } => value.clone(),
            other => panic!("expected an assignment, got {other:?}"),
        }
    }

    fn offset(written: &str) -> i64 {
        match given_value(written) {
            Term::Clock { offset, .. } => offset,
            other => panic!("`{written}` is not a clock term: {other:?}"),
        }
    }

    #[test]
    fn bare_now_is_the_clock_where_it_stands() {
        assert_eq!(offset("now"), 0);
    }

    #[test]
    fn a_moment_after_now() {
        assert_eq!(offset("now + 1.day"), 86_400_000);
        assert_eq!(offset("now + 2.hours"), 7_200_000);
    }

    #[test]
    fn a_moment_before_now() {
        assert_eq!(offset("now - 1.day"), -86_400_000);
        assert_eq!(offset("now - 30.minutes"), -1_800_000);
    }

    #[test]
    fn it_keeps_what_the_author_wrote() {
        match given_value("now + 1.day") {
            Term::Clock { written, .. } => assert_eq!(written, "now + 1.day"),
            other => panic!("{other:?}"),
        }
    }

    /// Silence here would leave the line reading as the bare name `now`, which
    /// resolves to nothing and reports as an unbound cast member — an error
    /// about the journey when the fault is in the line.
    #[test]
    fn now_followed_by_something_that_is_not_a_duration_is_refused() {
        for written in ["now + 1.fortnight", "now + banana", "now * 2", "now 1.day", "now +"] {
            let source = format!(
                "journey R {{\n    goal: x\n    given:\n        a.b = {written}\n\n    1. s\n        then a.b = c\n}}\n"
            );
            assert!(parse(&source).is_err(), "`{written}` should not read as a moment");
        }
    }

    /// `nowhere` opens with the same three letters and is an ordinary word.
    #[test]
    fn a_word_that_merely_starts_with_now_is_not_a_moment() {
        assert!(matches!(given_value("nowhere"), Term::Path(_)));
    }

    #[test]
    fn a_duration_on_its_own_is_still_a_duration() {
        assert!(matches!(given_value("1.day"), Term::Literal(_)));
    }
}
