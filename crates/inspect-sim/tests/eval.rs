//! The evaluator, driven against real worlds.
//!
//! An integration test rather than a unit one because the thing worth asserting
//! is the whole behaviour — expression plus world plus bindings — and because
//! building AST fragments is verbose enough that sharing the builders across
//! every case is what keeps the assertions readable.
//!
//! Two properties are checked over and over, and they are the crate's whole
//! contract:
//!
//! 1. What the evaluator can decide, it decides.
//! 2. What it cannot, it says so about — with a reason naming the thing it
//!    could not resolve, never by defaulting to true or false.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use inspect_sim::{
    Truth, Value,
    eval::{Env, eval},
    value::{EntityId, Instance},
    world::World,
};
use serde_json::{Value as Json, json};

// --- building expressions ------------------------------------------------

fn ident(name: &str) -> Json {
    json!({"Ident": {"span": {"start": 0, "end": 0}, "name": name}})
}

fn field(object: Json, name: &str) -> Json {
    json!({"MemberAccess": {
        "span": {"start": 0, "end": 0},
        "object": object,
        "field": {"span": {"start": 0, "end": 0}, "name": name},
    }})
}

fn compare(left: Json, op: &str, right: Json) -> Json {
    json!({"Comparison": {"span": {"start": 0, "end": 0}, "left": left, "op": op, "right": right}})
}

fn logical(left: Json, op: &str, right: Json) -> Json {
    json!({"LogicalOp": {"span": {"start": 0, "end": 0}, "left": left, "op": op, "right": right}})
}

fn arithmetic(left: Json, op: &str, right: Json) -> Json {
    json!({"BinaryOp": {"span": {"start": 0, "end": 0}, "left": left, "op": op, "right": right}})
}

fn number(text: &str) -> Json {
    json!({"NumberLiteral": {"span": {"start": 0, "end": 0}, "value": text}})
}

fn duration(text: &str) -> Json {
    json!({"DurationLiteral": {"span": {"start": 0, "end": 0}, "value": text}})
}

fn null() -> Json {
    json!({"Null": {"span": {"start": 0, "end": 0}}})
}

fn now() -> Json {
    json!({"Now": {"span": {"start": 0, "end": 0}}})
}

fn not(operand: Json) -> Json {
    json!({"Not": {"span": {"start": 0, "end": 0}, "operand": operand}})
}

fn exists(operand: Json) -> Json {
    json!({"Exists": {"span": {"start": 0, "end": 0}, "operand": operand}})
}

fn filtered(source: Json, condition: Json) -> Json {
    json!({"Where": {"span": {"start": 0, "end": 0}, "source": source, "condition": condition}})
}

// --- building worlds -----------------------------------------------------

/// A library world: two copies, one on loan, and a member holding it.
fn library() -> World {
    let mut world = World::new().at(1_000);

    let available = world.create("Copy", "catalogue");
    world.set_field(&available, "shelfmark", Value::Str("QA76".to_owned()));
    world.set_field(&available, "status", Value::Enum("available".to_owned()));

    let on_loan = world.create("Copy", "catalogue");
    world.set_field(&on_loan, "shelfmark", Value::Str("QA77".to_owned()));
    world.set_field(&on_loan, "status", Value::Enum("on_loan".to_owned()));

    let member = world.create("Member", "lending");
    world.set_field(&member, "name", Value::Str("Ada".to_owned()));
    world.set_field(&member, "open_loan_count", Value::Int(1));

    world.set_config("lending", "loan_limit", Value::Int(5));
    world
}

fn env<'a>(world: &'a World, module: &'a str) -> Env<'a> {
    Env::new(world, module, "")
}

/// Evaluate against the library world, returning the value.
fn value_of(node: &Json, env: &Env<'_>) -> Value {
    eval(node, env).value
}

/// Evaluate as a condition.
fn truth_of(node: &Json, env: &Env<'_>) -> Truth {
    eval(node, env).truth()
}

/// The reasons the evaluator gave for what it could not decide.
fn reasons(node: &Json, env: &Env<'_>) -> Vec<String> {
    eval(node, env).unresolved.into_iter().map(|note| note.reason).collect()
}

// --- names ---------------------------------------------------------------

#[test]
fn a_bound_name_evaluates_to_what_it_is_bound_to() {
    let world = library();
    let copy = EntityId::new("Copy", 1);
    let env = env(&world, "lending").bind("copy", Value::Ref(copy.clone()));
    assert_eq!(value_of(&ident("copy"), &env), Value::Ref(copy));
}

#[test]
fn an_unbound_name_is_undecided_and_says_which_one() {
    // A rule argument nobody supplied. Defaulting it either way would report a
    // precondition as settled that nothing checked.
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(value_of(&ident("member"), &env), Value::Unknown);
    assert_eq!(reasons(&ident("member"), &env), ["nothing is bound to `member`"]);
}

#[test]
fn an_entity_type_evaluates_to_every_instance_of_it() {
    let world = library();
    let env = env(&world, "catalogue");
    let Value::Set(copies) = value_of(&ident("Copy"), &env) else {
        panic!("an entity type is a collection");
    };
    assert_eq!(copies.len(), 2);
}

#[test]
fn a_type_with_no_instances_is_an_empty_collection_not_an_unknown() {
    // `exists Membership{...}` over a world with no memberships is false, and
    // reporting it as undecided would make an empty world unsimulatable.
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(value_of(&ident("Membership"), &env), Value::Set(Vec::new()));
    assert!(reasons(&ident("Membership"), &env).is_empty());
}

// --- fields --------------------------------------------------------------

#[test]
fn a_field_is_read_through_a_reference() {
    let world = library();
    let env = env(&world, "catalogue").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    assert_eq!(
        value_of(&field(ident("copy"), "status"), &env),
        Value::Enum("available".to_owned())
    );
}

#[test]
fn a_field_nothing_has_set_is_undecided() {
    let world = library();
    let env = env(&world, "catalogue").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    assert_eq!(value_of(&field(ident("copy"), "borrowed_at"), &env), Value::Unknown);
}

#[test]
fn a_reference_to_something_absent_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("ghost", Value::Ref(EntityId::new("Copy", 99)));
    let node = field(ident("ghost"), "status");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(reasons(&node, &env), ["`Copy#99` is not in this world"]);
}

#[test]
fn a_field_read_across_a_collection_is_a_projection() {
    // `receipts.reporter` is every reporter, not an error.
    let world = library();
    let env = env(&world, "catalogue");
    let statuses = value_of(&field(ident("Copy"), "status"), &env);
    assert_eq!(
        statuses,
        Value::Set(vec![Value::Enum("available".to_owned()), Value::Enum("on_loan".to_owned()),])
    );
}

#[test]
fn counting_a_collection_gives_an_integer() {
    let world = library();
    let env = env(&world, "catalogue");
    assert_eq!(value_of(&field(ident("Copy"), "count"), &env), Value::Int(2));
}

#[test]
fn counting_something_that_is_not_a_collection_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("n", Value::Int(3));
    let node = field(ident("n"), "count");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert!(reasons(&node, &env)[0].contains("has no count"));
}

#[test]
fn reading_a_field_from_a_scalar_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("n", Value::Int(3));
    let node = field(ident("n"), "status");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert!(reasons(&node, &env)[0].contains("has no fields"));
}

#[test]
fn config_is_a_namespace_rather_than_an_entity() {
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(value_of(&field(ident("config"), "loan_limit"), &env), Value::Int(5));
}

#[test]
fn a_config_parameter_nobody_set_is_undecided() {
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(value_of(&field(ident("config"), "absent"), &env), Value::Unknown);
}

// --- comparison ----------------------------------------------------------

#[test]
fn a_status_is_compared_against_a_bare_state_name() {
    // The judgement the evaluator exists to get right. `available` parses as a
    // plain identifier; the other side of the comparison is what says it meant
    // a state.
    let world = library();
    let env = env(&world, "catalogue").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = compare(field(ident("copy"), "status"), "Eq", ident("available"));
    assert_eq!(truth_of(&node, &env), Truth::True);
    assert!(reasons(&node, &env).is_empty(), "and it is not reported as undecided");

    let wrong = compare(field(ident("copy"), "status"), "Eq", ident("on_loan"));
    assert_eq!(truth_of(&wrong, &env), Truth::False);
}

#[test]
fn the_state_reading_works_from_either_side() {
    let world = library();
    let env = env(&world, "catalogue").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = compare(ident("available"), "Eq", field(ident("copy"), "status"));
    assert_eq!(truth_of(&node, &env), Truth::True);
}

#[test]
fn an_unbound_name_compared_to_a_non_state_stays_undecided() {
    // The other half of the same judgement: without a state on the other side,
    // an unbound name is a missing argument and is reported as one.
    let world = library();
    let env = env(&world, "lending");
    let node = compare(ident("title"), "NotEq", null());
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert_eq!(reasons(&node, &env), ["nothing is bound to `title`"]);
}

#[test]
fn null_is_something_a_precondition_can_ask_about() {
    let world = library();
    let env = env(&world, "lending").bind("attachment_size", Value::Null);
    assert_eq!(truth_of(&compare(ident("attachment_size"), "Eq", null()), &env), Truth::True);
}

#[test]
fn ordering_compares_numbers() {
    let world = library();
    let env = env(&world, "lending").bind("member", Value::Ref(EntityId::new("Member", 1)));
    let count = field(ident("member"), "open_loan_count");
    let limit = field(ident("config"), "loan_limit");
    assert_eq!(truth_of(&compare(count.clone(), "Lt", limit.clone()), &env), Truth::True);
    assert_eq!(truth_of(&compare(count.clone(), "GtEq", limit.clone()), &env), Truth::False);
    assert_eq!(truth_of(&compare(count, "LtEq", limit), &env), Truth::True);
}

#[test]
fn ordering_across_incomparable_kinds_is_undecided_and_says_why() {
    // Not false: the question is malformed, and answering it either way would
    // report a precondition the spec never posed.
    let world = library();
    let env = env(&world, "lending")
        .bind("name", Value::Str("Ada".to_owned()))
        .bind("window", Value::Duration(1000));
    let node = compare(ident("name"), "Lt", ident("window"));
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert!(reasons(&node, &env)[0].contains("cannot be ordered"));
}

#[test]
fn a_comparison_this_evaluator_does_not_know_is_reported_by_name() {
    let world = library();
    let env = env(&world, "lending");
    let node = compare(number("1"), "Spaceship", number("2"));
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert!(reasons(&node, &env)[0].contains("Spaceship"));
}

// --- connectives ---------------------------------------------------------

#[test]
fn the_connectives_follow_three_valued_logic() {
    let world = library();
    let env = env(&world, "lending").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let yes = compare(field(ident("copy"), "status"), "Eq", ident("available"));
    let no = compare(field(ident("copy"), "status"), "Eq", ident("lost"));
    let undecided = compare(ident("nobody"), "Eq", number("1"));

    assert_eq!(truth_of(&logical(yes.clone(), "And", no.clone()), &env), Truth::False);
    assert_eq!(truth_of(&logical(yes.clone(), "Or", no.clone()), &env), Truth::True);
    // One false operand settles a conjunction whatever the other turns out to be.
    assert_eq!(truth_of(&logical(no.clone(), "And", undecided.clone()), &env), Truth::False);
    assert_eq!(truth_of(&logical(yes.clone(), "And", undecided.clone()), &env), Truth::Unknown);
    assert_eq!(truth_of(&logical(yes, "Or", undecided.clone()), &env), Truth::True);
    assert_eq!(truth_of(&logical(no, "Implies", undecided), &env), Truth::True);
}

#[test]
fn a_connective_carries_what_either_side_could_not_decide() {
    let world = library();
    let env = env(&world, "lending");
    let node = logical(
        compare(ident("first"), "Eq", number("1")),
        "And",
        compare(ident("second"), "Eq", number("2")),
    );
    let reported = reasons(&node, &env);
    assert_eq!(reported.len(), 2, "both sides are reported, not just the first");
}

#[test]
fn negation_leaves_the_undecided_undecided() {
    let world = library();
    let env = env(&world, "lending").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let yes = compare(field(ident("copy"), "status"), "Eq", ident("available"));
    assert_eq!(truth_of(&not(yes), &env), Truth::False);
    assert_eq!(truth_of(&not(ident("nobody")), &env), Truth::Unknown);
}

// --- arithmetic ----------------------------------------------------------

#[test]
fn a_timestamp_plus_a_duration_is_a_timestamp() {
    let world = library();
    let env = env(&world, "lending").bind("opened_at", Value::Timestamp(1_000));
    let node = arithmetic(ident("opened_at"), "Add", duration("1.seconds"));
    assert_eq!(value_of(&node, &env), Value::Timestamp(2_000));
}

#[test]
fn a_due_date_check_against_the_clock_works_end_to_end() {
    // The shape of every temporal rule: `opened_at + window <= now`. The clock
    // being a field is what makes this a thing you step *to* rather than a
    // thing you wait for.
    let overdue_at = |clock: i64| {
        let world = library().at(clock);
        let scope = Env::new(&world, "lending", "").bind("opened_at", Value::Timestamp(0));
        let due = arithmetic(ident("opened_at"), "Add", duration("21.days"));
        eval(&compare(due, "LtEq", now()), &scope).truth()
    };

    let three_weeks = 21 * 86_400_000;
    assert_eq!(overdue_at(100_000), Truth::False, "the day it was borrowed");
    assert_eq!(overdue_at(three_weeks - 1), Truth::False, "a millisecond early");
    assert_eq!(overdue_at(three_weeks), Truth::True, "exactly due");
    assert_eq!(overdue_at(three_weeks + 1), Truth::True, "and after");
}

#[test]
fn two_timestamps_subtract_to_a_duration() {
    let world = library();
    let env = env(&world, "lending")
        .bind("later", Value::Timestamp(5_000))
        .bind("earlier", Value::Timestamp(2_000));
    assert_eq!(
        value_of(&arithmetic(ident("later"), "Sub", ident("earlier")), &env),
        Value::Duration(3_000)
    );
}

#[test]
fn arithmetic_between_kinds_it_is_not_defined_for_says_so() {
    // `due_at + 21` typechecks its way to an answer that means nothing, unless
    // the units are kept.
    let world = library();
    let env = env(&world, "lending").bind("due_at", Value::Timestamp(0));
    let node = arithmetic(ident("due_at"), "Add", number("21"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert!(reasons(&node, &env)[0].contains("not defined between"));
}

#[test]
fn integer_arithmetic_works_and_division_by_zero_does_not_panic() {
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(value_of(&arithmetic(number("6"), "Add", number("4")), &env), Value::Int(10));
    assert_eq!(value_of(&arithmetic(number("6"), "Sub", number("4")), &env), Value::Int(2));
    assert_eq!(value_of(&arithmetic(number("6"), "Mul", number("4")), &env), Value::Int(24));
    assert_eq!(value_of(&arithmetic(number("6"), "Div", number("4")), &env), Value::Int(1));
    assert_eq!(value_of(&arithmetic(number("6"), "Div", number("0")), &env), Value::Unknown);
}

// --- existence and filters -----------------------------------------------

#[test]
fn existence_over_a_collection_asks_whether_it_has_anything() {
    let world = library();
    let env = env(&world, "catalogue");
    assert_eq!(truth_of(&exists(ident("Copy")), &env), Truth::True);
    assert_eq!(truth_of(&exists(ident("Membership")), &env), Truth::False);
}

#[test]
fn existence_over_something_undecided_stays_undecided() {
    let world = library();
    let env = env(&world, "catalogue");
    assert_eq!(truth_of(&exists(ident("nobody")), &env), Truth::Unknown);
}

#[test]
fn a_filter_keeps_only_the_elements_whose_condition_holds() {
    // `Copy where status = available` — and the element's own fields are in
    // scope bare, which is how the language writes it.
    let world = library();
    let env = env(&world, "catalogue");
    let node = filtered(ident("Copy"), compare(ident("status"), "Eq", ident("available")));
    let Value::Set(kept) = value_of(&node, &env) else { panic!("a filter yields a collection") };
    assert_eq!(kept, [Value::Ref(EntityId::new("Copy", 1))]);
}

#[test]
fn a_filter_that_matches_nothing_yields_an_empty_collection() {
    let world = library();
    let env = env(&world, "catalogue");
    let node = filtered(ident("Copy"), compare(ident("status"), "Eq", ident("lost")));
    assert_eq!(value_of(&node, &env), Value::Set(Vec::new()));
    assert_eq!(truth_of(&exists(node), &env), Truth::False);
}

#[test]
fn a_filter_whose_predicate_is_undecided_leaves_the_element_out_and_says_so() {
    // Neither widening nor narrowing silently: the element does not make the
    // cut, and the reader is told the predicate could not be evaluated.
    let world = library();
    let env = env(&world, "catalogue");
    let node = filtered(ident("Copy"), compare(ident("mystery"), "Eq", number("1")));
    assert_eq!(value_of(&node, &env), Value::Set(Vec::new()));
    assert!(!reasons(&node, &env).is_empty());
}

#[test]
fn filtering_something_that_is_not_a_collection_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("n", Value::Int(1));
    let node = filtered(ident("n"), compare(number("1"), "Eq", number("1")));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert!(reasons(&node, &env)[0].contains("cannot be filtered"));
}

// --- the contract --------------------------------------------------------

#[test]
fn an_expression_kind_this_evaluator_does_not_model_is_named_rather_than_guessed() {
    let world = library();
    let env = env(&world, "lending");
    let node = json!({"Lambda": {"span": {"start": 0, "end": 0}}});
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(reasons(&node, &env), ["`Lambda` expressions are not simulated"]);
}

#[test]
fn something_that_is_not_an_expression_at_all_is_reported() {
    let world = library();
    let env = env(&world, "lending");
    for node in [json!(null), json!("text"), json!({"a": 1, "b": 2})] {
        assert_eq!(value_of(&node, &env), Value::Unknown);
        assert!(!reasons(&node, &env).is_empty(), "{node} produced no reason");
    }
}

#[test]
fn every_undecided_result_carries_at_least_one_reason() {
    // The crate's whole contract, stated once as a property. An unknown with no
    // explanation is indistinguishable from a bug.
    let world = library();
    let env = env(&world, "lending");
    let cases = [
        ident("nobody"),
        field(ident("nobody"), "x"),
        compare(ident("nobody"), "Lt", number("1")),
        arithmetic(ident("nobody"), "Add", number("1")),
        json!({"Pipe": {"span": {"start": 0, "end": 0}}}),
    ];
    for node in cases {
        let evaluated = eval(&node, &env);
        if evaluated.value.is_unknown() {
            assert!(!evaluated.unresolved.is_empty(), "{node} came back undecided with no reason");
        }
    }
}

#[test]
fn an_undecided_note_quotes_the_source_it_came_from() {
    // What the panel shows. Without it the reader is told something could not
    // be decided and not which part of the line it was.
    let source = "requires: copy.status = available";
    let world = library();
    let env = Env::new(&world, "catalogue", source);
    let start = source.find("copy.status").expect("present");
    let node = json!({"Ident": {
        "span": {"start": start, "end": start + 4},
        "name": "copy",
    }});
    let note = &eval(&node, &env).unresolved[0];
    assert_eq!(note.expression.as_deref(), Some("copy"));
    assert!(note.span.is_some());
}

#[test]
fn evaluation_is_deterministic() {
    // Everything downstream assumes it: a snapshot of a trace, a shared link,
    // and mutation testing all stop meaning anything without it.
    let world = library();
    let env = env(&world, "catalogue");
    let node = filtered(ident("Copy"), compare(ident("status"), "Eq", ident("available")));
    assert_eq!(eval(&node, &env), eval(&node, &env));
}

#[test]
fn an_instance_added_by_hand_is_readable_the_same_way() {
    // The world editor builds instances directly rather than through a rule,
    // and they have to behave identically.
    let mut world = World::new();
    world.insert(
        Instance::new(EntityId("Seeded".to_owned()), "Member", "lending")
            .with("open_loan_count", Value::Int(9)),
    );
    let env = env(&world, "lending").bind("m", Value::Ref(EntityId("Seeded".to_owned())));
    assert_eq!(value_of(&field(ident("m"), "open_loan_count"), &env), Value::Int(9));
}

// --- quantification ------------------------------------------------------

fn quantify(binding: &str, collection: Json, body: Json, filter: Option<Json>) -> Json {
    json!({"For": {
        "span": {"start": 0, "end": 0},
        "binding": {"Single": {"span": {"start": 0, "end": 0}, "name": binding}},
        "collection": collection,
        "filter": filter,
        "body": body,
    }})
}

#[test]
fn an_invariant_holds_when_every_element_satisfies_it() {
    // The shape of every invariant a real spec writes.
    let world = library();
    let env = env(&world, "catalogue");
    let all_have_a_status =
        quantify("c", ident("Copies"), compare(ident("status"), "NotEq", null()), None);
    assert_eq!(truth_of(&all_have_a_status, &env), Truth::True);
}

#[test]
fn an_invariant_fails_when_one_element_does_not() {
    let world = library();
    let env = env(&world, "catalogue");
    let all_available =
        quantify("c", ident("Copies"), compare(ident("status"), "Eq", ident("available")), None);
    assert_eq!(truth_of(&all_available, &env), Truth::False, "one copy is on loan");
}

#[test]
fn a_collection_is_named_in_the_plural() {
    // `for m in Members` — without resolving the plural every invariant ranges
    // over nothing and holds vacuously, which is a checker that always passes.
    let world = library();
    let env = env(&world, "lending");
    let over_members =
        quantify("m", ident("Members"), compare(ident("name"), "NotEq", null()), None);
    assert_eq!(truth_of(&over_members, &env), Truth::True);

    let never_true = quantify("m", ident("Members"), compare(ident("name"), "Eq", null()), None);
    assert_eq!(
        truth_of(&never_true, &env),
        Truth::False,
        "if it ranged over nothing this would be vacuously true"
    );
}

#[test]
fn an_invariant_over_an_empty_collection_is_vacuously_true() {
    // A spec with no loans does not violate a rule about loans.
    let world = library();
    let env = env(&world, "lending");
    let over_nothing = quantify("l", ident("Loans"), compare(number("1"), "Eq", number("2")), None);
    assert_eq!(truth_of(&over_nothing, &env), Truth::True);
}

#[test]
fn a_filter_narrows_what_is_claimed_about() {
    // `for c in Copies where status = on_loan: …` says nothing about the rest.
    let world = library();
    let env = env(&world, "catalogue");
    let claim = compare(
        ident("shelfmark"),
        "Eq",
        json!({"StringLiteral": {"span": {"start": 0, "end": 0}, "value": "QA77"}}),
    );
    let filter = compare(ident("status"), "Eq", ident("on_loan"));
    assert_eq!(
        truth_of(&quantify("c", ident("Copies"), claim.clone(), Some(filter)), &env),
        Truth::True
    );
    assert_eq!(
        truth_of(&quantify("c", ident("Copies"), claim, None), &env),
        Truth::False,
        "without the filter the other copy is a counterexample"
    );
}

#[test]
fn an_element_the_body_cannot_decide_leaves_the_whole_claim_undecided() {
    let world = library();
    let env = env(&world, "catalogue");
    let claim = compare(ident("mystery"), "Eq", number("1"));
    let node = quantify("c", ident("Copies"), claim, None);
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert!(!reasons(&node, &env).is_empty(), "and says which element it was");
}

#[test]
fn one_definite_counterexample_settles_it_even_among_undecided_elements() {
    // Kleene's conjunction: a false anywhere settles the claim regardless of
    // what the rest turn out to be.
    let world = library();
    let env = env(&world, "catalogue");
    let body = logical(
        compare(ident("status"), "Eq", ident("available")),
        "And",
        compare(ident("mystery"), "Eq", number("1")),
    );
    assert_eq!(truth_of(&quantify("c", ident("Copies"), body, None), &env), Truth::False);
}

#[test]
fn quantifying_over_something_that_is_not_a_collection_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("n", Value::Int(3));
    let node = quantify("x", ident("n"), compare(number("1"), "Eq", number("1")), None);
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert!(reasons(&node, &env).iter().any(|r| r.contains("has no elements")));
}

#[test]
fn the_bound_name_is_usable_as_well_as_the_bare_fields() {
    // `for c in Copies: c.status = available` and `… : status = available` are
    // both written in real specs.
    let world = library();
    let env = env(&world, "catalogue");
    let by_name =
        quantify("c", ident("Copies"), compare(field(ident("c"), "status"), "NotEq", null()), None);
    assert_eq!(truth_of(&by_name, &env), Truth::True);
}

// --- ordering decimals ---------------------------------------------------
//
// Every one of these is a comparison a spec writes against a config value, and
// the arms that answer them are separate because the kinds do not compare
// directly. An arm quietly missing reads to a user as "undecided" on a question
// with an obvious answer.

#[test]
fn two_decimals_are_ordered_against_each_other() {
    let world = library();
    let env = env(&world, "lending").bind("a", Value::Float(1.5)).bind("b", Value::Float(2.5));
    assert_eq!(truth_of(&compare(ident("a"), "Lt", ident("b")), &env), Truth::True);
    assert_eq!(truth_of(&compare(ident("a"), "Gt", ident("b")), &env), Truth::False);
}

#[test]
fn a_whole_number_is_ordered_against_a_decimal_either_way_round() {
    // `requires: copy.rating > config.threshold` is an integer against a
    // decimal, and a spec never says which side is which.
    let world = library();
    let env = env(&world, "lending").bind("whole", Value::Int(2)).bind("part", Value::Float(2.5));
    assert_eq!(truth_of(&compare(ident("whole"), "Lt", ident("part")), &env), Truth::True);
    assert_eq!(truth_of(&compare(ident("part"), "Lt", ident("whole")), &env), Truth::False);
    assert_eq!(truth_of(&compare(ident("part"), "Gt", ident("whole")), &env), Truth::True);
}

#[test]
fn a_whole_number_equals_the_decimal_that_names_the_same_number() {
    let world = library();
    let env = env(&world, "lending").bind("whole", Value::Int(3)).bind("part", Value::Float(3.0));
    assert_eq!(truth_of(&compare(ident("whole"), "LtEq", ident("part")), &env), Truth::True);
    assert_eq!(truth_of(&compare(ident("whole"), "Lt", ident("part")), &env), Truth::False);
    assert_eq!(truth_of(&compare(ident("part"), "GtEq", ident("whole")), &env), Truth::True);
    assert_eq!(truth_of(&compare(ident("part"), "Gt", ident("whole")), &env), Truth::False);
}

#[test]
fn the_tolerance_on_decimals_scales_with_the_size_of_the_numbers() {
    // A fixed tolerance is a tolerance that stops working. At a million, the
    // gap between one representable double and the next is already larger than
    // any absolute epsilon worth writing down, so two values a spec means to be
    // the same number have to still compare equal there.
    let world = library();
    let big = 1_000_000.0_f64;
    let together = env(&world, "lending")
        .bind("left", Value::Float(big))
        .bind("right", Value::Float(big + 1e-9));
    assert_eq!(truth_of(&compare(ident("left"), "Lt", ident("right")), &together), Truth::False);
    assert_eq!(truth_of(&compare(ident("left"), "GtEq", ident("right")), &together), Truth::True);

    // And it is still a tolerance, not a licence: a real difference is real.
    let apart = env(&world, "lending")
        .bind("left", Value::Float(big))
        .bind("right", Value::Float(big + 1.0));
    assert_eq!(truth_of(&compare(ident("left"), "Lt", ident("right")), &apart), Truth::True);
}

#[test]
fn strictly_greater_is_not_greater_or_equal() {
    // The one comparison where an equal pair separates the two operators.
    let world = library();
    let env = env(&world, "lending").bind("a", Value::Int(4)).bind("b", Value::Int(4));
    assert_eq!(truth_of(&compare(ident("a"), "Gt", ident("b")), &env), Truth::False);
    assert_eq!(truth_of(&compare(ident("a"), "GtEq", ident("b")), &env), Truth::True);
}

// --- arithmetic on times -------------------------------------------------

#[test]
fn a_duration_taken_off_a_timestamp_is_a_timestamp() {
    // `intent.created_at + config.window <= now` is the shape of every
    // temporal trigger in a real spec, and it runs in both directions.
    let world = library();
    let env = env(&world, "lending")
        .bind("at", Value::Timestamp(10_000))
        .bind("by", Value::Duration(2_500));
    assert_eq!(
        value_of(&arithmetic(ident("at"), "Sub", ident("by")), &env),
        Value::Timestamp(7_500)
    );
    assert_eq!(
        value_of(&arithmetic(ident("at"), "Add", ident("by")), &env),
        Value::Timestamp(12_500)
    );
}

#[test]
fn two_durations_add_and_subtract_to_a_duration() {
    let world = library();
    let env = env(&world, "lending")
        .bind("long", Value::Duration(9_000))
        .bind("short", Value::Duration(4_000));
    assert_eq!(
        value_of(&arithmetic(ident("long"), "Add", ident("short")), &env),
        Value::Duration(13_000)
    );
    assert_eq!(
        value_of(&arithmetic(ident("long"), "Sub", ident("short")), &env),
        Value::Duration(5_000)
    );
}

#[test]
fn arithmetic_between_kinds_that_do_not_combine_says_which_kinds() {
    let world = library();
    let env = env(&world, "lending")
        .bind("at", Value::Timestamp(10_000))
        .bind("name", Value::Str("Ada".to_owned()));
    let sum = arithmetic(ident("at"), "Add", ident("name"));
    assert_eq!(value_of(&sum, &env), Value::Unknown);
    assert_eq!(reasons(&sum, &env), vec!["`Add` is not defined between a timestamp and a string"]);
}

#[test]
fn arithmetic_on_something_already_undecided_adds_no_second_complaint() {
    // The operand's own reason is the one worth reading. Saying additionally
    // that `Add` is not defined between an unknown and an integer is noise
    // that buries it, and it blames the operator for the operand's problem.
    let world = library();
    let env = env(&world, "lending").bind("known", Value::Int(1));
    for sum in [
        arithmetic(ident("nobody_bound_this"), "Add", ident("known")),
        arithmetic(ident("known"), "Add", ident("nobody_bound_this")),
    ] {
        assert_eq!(value_of(&sum, &env), Value::Unknown);
        assert_eq!(reasons(&sum, &env), vec!["nothing is bound to `nobody_bound_this`"]);
    }
}

// --- membership ----------------------------------------------------------

fn within(needle: Json, haystack: Json) -> Json {
    json!({"In": {"span": {"start": 0, "end": 0}, "value": needle, "collection": haystack}})
}

#[test]
fn membership_of_a_collection_is_decided_both_ways() {
    let world = library();
    let env = env(&world, "lending").bind(
        "kinds",
        Value::Set(vec![Value::Enum("read".to_owned()), Value::Enum("delivered".to_owned())]),
    );
    let present = within(ident("kind"), ident("kinds"));
    let held = |state: &str| {
        let mut scope = Env::new(&world, "lending", "");
        scope.bindings.clone_from(&env.bindings);
        truth_of(&present, &scope.bind("kind", Value::Enum(state.to_owned())))
    };
    assert_eq!(held("read"), Truth::True);
    assert_eq!(held("vetoed"), Truth::False);
}

#[test]
fn membership_of_something_that_is_not_a_collection_is_undecided() {
    let world = library();
    let env = env(&world, "lending").bind("kinds", Value::Int(3));
    assert_eq!(truth_of(&within(number("3"), ident("kinds")), &env), Truth::Unknown);
}

// --- existence -----------------------------------------------------------

#[test]
fn a_relationship_that_is_null_does_not_exist() {
    // An entity with no attachment has none — that is false, not undecided,
    // and `not exists attachment` is a precondition real specs write.
    let world = library();
    let env = env(&world, "lending").bind("attachment", Value::Null);
    assert_eq!(truth_of(&exists(ident("attachment")), &env), Truth::False);
    assert_eq!(truth_of(&not(exists(ident("attachment"))), &env), Truth::True);
}

// --- naming a field that was set to nothing in particular ------------------

#[test]
fn a_field_deliberately_set_to_unknown_is_not_reported_as_unset() {
    // "`Copy#1` has no `status` set" would be wrong: it has one, and nobody
    // could work out what it is. The distinction is the difference between
    // "fill this in" and "this simulator cannot compute it".
    let mut world = World::new().at(1_000);
    let copy = world.create("Copy", "catalogue");
    world.set_field(&copy, "status", Value::Unknown);
    let env = env(&world, "catalogue").bind("copy", Value::Ref(copy));

    let status = field(ident("copy"), "status");
    assert_eq!(value_of(&status, &env), Value::Unknown);
    assert!(reasons(&status, &env).is_empty(), "{:?}", reasons(&status, &env));

    let missing = field(ident("copy"), "shelfmark");
    assert_eq!(reasons(&missing, &env), vec!["`Copy#1` has no `shelfmark` set"]);
}

// --- collections named in the plural ---------------------------------------

#[test]
fn a_plural_name_with_no_instances_behind_it_is_an_empty_collection() {
    // `for m in Members` over a world holding no members ranges over nothing.
    // It must not resolve through the singular branch to some other type's
    // instances, and it must not be undecided either.
    let world = World::new().at(1_000);
    let env = env(&world, "lending");
    assert_eq!(value_of(&ident("Members"), &env), Value::Set(Vec::new()));
    assert!(reasons(&ident("Members"), &env).is_empty());
}

#[test]
fn a_plural_name_resolves_to_the_instances_of_its_singular() {
    let world = library();
    let env = env(&world, "catalogue");
    let Value::Set(copies) = value_of(&ident("Copies"), &env) else {
        panic!("expected a collection");
    };
    assert_eq!(copies.len(), 2);
}
