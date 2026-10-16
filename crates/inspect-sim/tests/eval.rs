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

use allium_parser::Span as AstSpan;
use allium_parser::ast::{
    BinaryOp, ComparisonOp, Expr, ForBinding, Ident, JoinField, LogicalOp, QualifiedName,
    StringLiteral, StringPart,
};
use inspect_sim::{
    Truth, Value,
    eval::{Env, eval},
    value::{EntityId, Instance},
    world::World,
};

// --- building expressions ------------------------------------------------
//
// Allium's own tree, built by hand. The builders exist because a whole module
// of source per assertion would bury what each one is about — but they now
// build the same type the parser produces, so a case here is a case the
// evaluator could actually be handed.

const NOWHERE: AstSpan = AstSpan { start: 0, end: 0 };

fn ident(name: &str) -> Expr {
    Expr::Ident(Ident { span: NOWHERE, name: name.to_owned() })
}

fn field(object: Expr, name: &str) -> Expr {
    Expr::MemberAccess {
        span: NOWHERE,
        object: Box::new(object),
        field: Ident { span: NOWHERE, name: name.to_owned() },
    }
}

fn comparison_op(op: &str) -> ComparisonOp {
    match op {
        "Eq" => ComparisonOp::Eq,
        "NotEq" => ComparisonOp::NotEq,
        "Lt" => ComparisonOp::Lt,
        "LtEq" => ComparisonOp::LtEq,
        "Gt" => ComparisonOp::Gt,
        "GtEq" => ComparisonOp::GtEq,
        other => panic!("`{other}` is not a comparison the language has"),
    }
}

fn compare(left: Expr, op: &str, right: Expr) -> Expr {
    Expr::Comparison {
        span: NOWHERE,
        left: Box::new(left),
        op: comparison_op(op),
        right: Box::new(right),
    }
}

fn logical(left: Expr, op: &str, right: Expr) -> Expr {
    let op = match op {
        "And" => LogicalOp::And,
        "Or" => LogicalOp::Or,
        "Implies" => LogicalOp::Implies,
        other => panic!("`{other}` is not a connective the language has"),
    };
    Expr::LogicalOp { span: NOWHERE, left: Box::new(left), op, right: Box::new(right) }
}

fn arithmetic(left: Expr, op: &str, right: Expr) -> Expr {
    let op = match op {
        "Add" => BinaryOp::Add,
        "Sub" => BinaryOp::Sub,
        "Mul" => BinaryOp::Mul,
        "Div" => BinaryOp::Div,
        other => panic!("`{other}` is not an operator the language has"),
    };
    Expr::BinaryOp { span: NOWHERE, left: Box::new(left), op, right: Box::new(right) }
}

fn number(text: &str) -> Expr {
    Expr::NumberLiteral { span: NOWHERE, value: text.to_owned() }
}

fn duration(text: &str) -> Expr {
    Expr::DurationLiteral { span: NOWHERE, value: text.to_owned() }
}

fn null() -> Expr {
    Expr::Null { span: NOWHERE }
}

fn now() -> Expr {
    Expr::Now { span: NOWHERE }
}

fn not(operand: Expr) -> Expr {
    Expr::Not { span: NOWHERE, operand: Box::new(operand) }
}

fn exists(operand: Expr) -> Expr {
    Expr::Exists { span: NOWHERE, operand: Box::new(operand) }
}

fn text(value: &str) -> Expr {
    Expr::StringLiteral(StringLiteral {
        span: NOWHERE,
        parts: vec![StringPart::Text(value.to_owned())],
    })
}

fn call(function: Expr, args: Vec<Expr>) -> Expr {
    Expr::Call {
        span: NOWHERE,
        function: Box::new(function),
        args: args.into_iter().map(allium_parser::ast::CallArg::Positional).collect(),
    }
}

fn set_of(elements: Vec<Expr>) -> Expr {
    Expr::SetLiteral { span: NOWHERE, elements }
}

fn filtered(source: Expr, condition: Expr) -> Expr {
    Expr::Where { span: NOWHERE, source: Box::new(source), condition: Box::new(condition) }
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
fn value_of(node: &Expr, env: &Env<'_>) -> Value {
    eval(node, env).value
}

/// Evaluate as a condition.
fn truth_of(node: &Expr, env: &Env<'_>) -> Truth {
    eval(node, env).truth()
}

/// The reasons the evaluator gave for what it could not decide.
fn reasons(node: &Expr, env: &Env<'_>) -> Vec<String> {
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

/// A field read through a name bound to something already undecided — an
/// argument an emission could not settle — says so, and quotes the name. Read
/// bare, it was an unknown with no reason at all, and the chained rule
/// reported "something could not be evaluated" (issue #6).
#[test]
fn a_field_read_through_something_undecided_in_silence_says_so() {
    let source = "webhook.active";
    let world = library();
    let env = Env::new(&world, "lending", source).bind("webhook", Value::Unknown);
    let node = Expr::MemberAccess {
        span: AstSpan { start: 0, end: 14 },
        object: Box::new(Expr::Ident(Ident {
            span: AstSpan { start: 0, end: 7 },
            name: "webhook".to_owned(),
        })),
        field: Ident { span: AstSpan { start: 8, end: 14 }, name: "active".to_owned() },
    };
    let evaluated = eval(&node, &env);
    assert_eq!(evaluated.value, Value::Unknown);
    let said: Vec<_> =
        evaluated.unresolved.into_iter().map(|note| (note.reason, note.expression)).collect();
    assert_eq!(
        said,
        [(
            "`active` was read from something nothing settled".to_owned(),
            Some("webhook".to_owned())
        )]
    );
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
    let node = field(ident("config"), "absent");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    // With a reason naming the parameter. A config read was the one path that
    // came back unknown with nothing attached, so a rule guarded by a
    // parameter with no default said only "something could not be evaluated"
    // — which is indistinguishable from a bug (issue #11).
    let why = reasons(&node, &env);
    assert!(why.iter().any(|reason| reason.contains("`config.absent`")), "{why:?}");
    assert!(why.iter().any(|reason| reason.contains("nothing set it")), "{why:?}");
}

#[test]
fn a_config_parameter_set_to_something_undecided_says_that_instead() {
    // It has a default and somebody wrote over it, so "no default, and nothing
    // set it" would be false on both counts.
    let mut world = library();
    world.set_config("lending", "loan_limit", Value::Unknown);
    let env = env(&world, "lending");
    let node = field(ident("config"), "loan_limit");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    let why = reasons(&node, &env);
    assert!(why.iter().any(|reason| reason.contains("itself undecided")), "{why:?}");
    assert!(!why.iter().any(|reason| reason.contains("nothing set it")), "{why:?}");
}

#[test]
fn a_config_parameter_nobody_set_is_anchored_by_byte_offset() {
    // An em-dash in a comment above the rule: a span counted in characters
    // would land two bytes short and slice something else.
    const SOURCE: &str = "-- allium: 3
-- a switch \u{2014} with nothing said about it

config {
    sync_mode: String
}

rule Sync {
    when: Sync(repo)
    requires: config.sync_mode = \"push\"
    ensures: repo.synced = true
}
";
    let module = allium_parser::parse(SOURCE).module;
    let requires = module
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            allium_parser::ast::Decl::Block(block) => Some(block),
            _ => None,
        })
        .flat_map(|block| block.items.iter())
        .find_map(|item| match &item.kind {
            allium_parser::ast::BlockItemKind::Clause { keyword, value }
                if keyword == "requires" =>
            {
                Some(value.clone())
            }
            _ => None,
        })
        .expect("a requires clause");
    let world = World::new();
    let env = Env::new(&world, "switches", SOURCE);
    let evaluation = eval(&requires, &env);
    assert_eq!(evaluation.truth(), Truth::Unknown);
    let note = evaluation.unresolved.first().expect("a reason");
    let span = note.span.expect("a span");
    assert!(!SOURCE[..span.start].is_ascii(), "the em-dash is before the rule");
    assert_eq!(span.slice(SOURCE), Some("config.sync_mode"));
    assert_eq!(note.expression.as_deref(), Some("config.sync_mode"));
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
fn an_operator_that_could_not_be_applied_quotes_both_sides() {
    // The note names the two kinds, and the panel shows it beside the text it
    // is about — so the span has to cover the whole comparison rather than one
    // operand or neither. A note pointing at nothing sends the reader looking
    // for a line the tool never told them.
    let source = "requires: name < window";
    let world = library();
    let env = Env::new(&world, "lending", source)
        .bind("name", Value::Str("Ada".to_owned()))
        .bind("window", Value::Duration(1000));

    let at = |text: &str| {
        let start = source.find(text).expect("present");
        Expr::Ident(Ident {
            span: AstSpan { start, end: start + text.len() },
            name: text.to_owned(),
        })
    };
    let node = compare(at("name"), "Lt", at("window"));
    let note = &eval(&node, &env).unresolved[0];
    assert_eq!(note.expression.as_deref(), Some("name < window"), "{note:?}");
}

#[test]
fn arithmetic_that_could_not_be_applied_quotes_both_sides_too() {
    let source = "ensures: now + name";
    let world = library();
    let env = Env::new(&world, "lending", source).bind("name", Value::Str("Ada".to_owned()));

    let start = source.find("now").expect("present");
    let left = Expr::Now { span: AstSpan { start, end: start + 3 } };
    let at = source.find("name").expect("present");
    let right =
        Expr::Ident(Ident { span: AstSpan { start: at, end: at + 4 }, name: "name".to_owned() });
    let note = &eval(&arithmetic(left, "Add", right), &env).unresolved[0];
    assert_eq!(note.expression.as_deref(), Some("now + name"), "{note:?}");
}

#[test]
fn the_operators_are_the_language_s_own_and_cannot_be_anything_else() {
    // This used to assert that an operator the evaluator did not recognise —
    // `Spaceship`, say — was reported by name rather than guessed at. It could
    // arrive because the operator was a string read out of JSON.
    //
    // It is a closed enum now, so the case cannot be constructed and the branch
    // that handled it is gone. What is left to check is that each one the
    // language does have is actually distinguished, which is the property the
    // old test was standing in for.
    let world = library();
    let env = env(&world, "lending");
    assert_eq!(truth_of(&compare(number("1"), "Lt", number("2")), &env), Truth::True);
    assert_eq!(truth_of(&compare(number("1"), "Gt", number("2")), &env), Truth::False);
    assert_eq!(truth_of(&compare(number("2"), "GtEq", number("2")), &env), Truth::True);
    assert_eq!(truth_of(&compare(number("2"), "LtEq", number("2")), &env), Truth::True);
    assert_eq!(truth_of(&compare(number("2"), "Eq", number("2")), &env), Truth::True);
    assert_eq!(truth_of(&compare(number("2"), "NotEq", number("2")), &env), Truth::False);
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
    let node =
        Expr::Lambda { span: NOWHERE, param: Box::new(ident("x")), body: Box::new(ident("x")) };
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(reasons(&node, &env), ["a lambda is not simulated"]);
}

#[test]
fn every_form_the_language_has_is_either_evaluated_or_named() {
    // There used to be a test here for "something that is not an expression at
    // all" — a null, a bare string, a two-key object — because the evaluator
    // took JSON and any of those could reach it. None of them can now: the
    // argument is `allium_parser::ast::Expr`, so the only things that arrive
    // are things the parser built.
    //
    // What is left to check is the other half of the same promise. A form the
    // evaluator does not model must say which form it was, because "a lambda
    // is not simulated" is something a reader can act on and "unknown" is not.
    let world = library();
    let env = env(&world, "lending");
    let unmodelled = [
        Expr::Lambda { span: NOWHERE, param: Box::new(ident("x")), body: Box::new(ident("x")) },
        Expr::Pipe { span: NOWHERE, left: Box::new(ident("a")), right: Box::new(ident("b")) },
    ];
    for node in unmodelled {
        assert_eq!(value_of(&node, &env), Value::Unknown);
        let said = reasons(&node, &env);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].ends_with("is not simulated"), "{}", said[0]);
        assert!(said[0].len() > "is not simulated".len() + 4, "{}", said[0]);
    }
}

/// `within` in an actor's condition is the context the actor is identified
/// within (issue #13). It was reported as "a `within` deadline is not
/// simulated", which is neither what it is nor something a reader can act on.
#[test]
fn within_is_whatever_the_caller_bound_it_to() {
    let world = library();
    let node = Expr::Within { span: NOWHERE };
    let bound = env(&world, "lending").bind(inspect_sim::eval::WITHIN, Value::Bool(true));
    assert_eq!(value_of(&node, &bound), Value::Bool(true));
    assert!(reasons(&node, &bound).is_empty());
}

#[test]
fn within_bound_to_nothing_says_so() {
    let world = library();
    let env = env(&world, "lending");
    let node = Expr::Within { span: NOWHERE };
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(reasons(&node, &env), ["nothing is bound to `within`"]);
}

#[test]
fn arithmetic_that_runs_off_the_end_is_undecided_rather_than_a_crash() {
    // A world holds numbers a person typed. `just run` is a debug build, so
    // the unchecked form aborted the whole process on an overflow — the tool
    // vanishing mid-step rather than reporting anything. Release was worse: it
    // wrapped, and the step quietly held on a number nobody could have meant.
    let world = library();
    let env = env(&world, "lending");

    let big = number(&i64::MAX.to_string());
    for (described, node) in [
        ("addition", arithmetic(big.clone(), "Add", number("1"))),
        ("multiplication", arithmetic(big.clone(), "Mul", number("2"))),
        ("subtraction", arithmetic(number(&i64::MIN.to_string()), "Sub", number("1"))),
    ] {
        let evaluated = eval(&node, &env);
        assert!(evaluated.value.is_unknown(), "{described} overflowed into a value: {evaluated:?}");
        let said = reasons(&node, &env);
        assert!(
            said.iter().any(|reason| reason.contains("runs past")),
            "{described} says which way it failed: {said:?}"
        );
    }

    // And the distinction is kept: an operator that means nothing between two
    // kinds still says *that*, rather than blaming arithmetic that never ran.
    let undefined = arithmetic(text("a"), "Mul", text("b"));
    assert!(reasons(&undefined, &env).iter().any(|reason| reason.contains("is not defined")));

    let divided = arithmetic(number("1"), "Div", number("0"));
    assert!(reasons(&divided, &env).iter().any(|reason| reason.contains("zero")));
}

#[test]
fn every_undecided_result_carries_at_least_one_reason() {
    // The crate's whole contract, stated once as a property. An unknown with no
    // explanation is indistinguishable from a bug.
    //
    // This used to be five expressions that already passed, which is a
    // description of the contract rather than a test of it. Three paths were
    // returning a reasonless unknown the whole time it was green: membership
    // over a known non-collection, an unbound `this`, and a conditional on a
    // known non-boolean. Each had a sibling handling the same situation
    // correctly, which is the shape to look for when adding a case here —
    // reach for the arm nobody would think to write a test *about*.
    let world = library();
    let env = env(&world, "lending");
    let cases: Vec<(&str, Expr)> = vec![
        ("an unbound name", ident("nobody")),
        ("a field of an unbound name", field(ident("nobody"), "x")),
        ("a comparison against one", compare(ident("nobody"), "Lt", number("1"))),
        ("arithmetic on one", arithmetic(ident("nobody"), "Add", number("1"))),
        (
            "a pipe",
            Expr::Pipe { span: NOWHERE, left: Box::new(ident("a")), right: Box::new(ident("b")) },
        ),
        // Membership needs a collection to test against. A known scalar is not
        // one, and saying so is what `filtered` already does for the same case.
        ("membership over a scalar", within(number("1"), number("2"))),
        ("membership over an unbound name", within(number("1"), ident("nobody"))),
        // `this` is bound by an entity's own context. `check_invariants` and
        // `run_rule` both reach the evaluator without binding it.
        ("an unbound this", Expr::This { span: NOWHERE }),
        ("a field of an unbound this", field(Expr::This { span: NOWHERE }, "status")),
        // Which branch runs is not known if the condition is not a boolean.
        (
            "a conditional on a non-boolean",
            Expr::Conditional {
                span: NOWHERE,
                branches: vec![allium_parser::ast::CondBranch {
                    span: NOWHERE,
                    condition: number("1"),
                    body: number("2"),
                }],
                else_body: None,
            },
        ),
        (
            "a conditional on an unbound name",
            Expr::Conditional {
                span: NOWHERE,
                branches: vec![allium_parser::ast::CondBranch {
                    span: NOWHERE,
                    condition: ident("nobody"),
                    body: number("2"),
                }],
                else_body: None,
            },
        ),
        (
            "a set holding an unbound name",
            Expr::SetLiteral { span: NOWHERE, elements: vec![number("1"), ident("nobody")] },
        ),
        (
            "a call on an unbound name",
            call(field(ident("nobody"), "is_used_by"), vec![number("1")]),
        ),
        ("a call on a scalar", call(field(number("1"), "is_used_by"), vec![number("1")])),
        ("a call nothing defines", call(ident("nobody_defined_this"), vec![number("1")])),
        (
            "a set plus an unbound name",
            arithmetic(
                Expr::SetLiteral { span: NOWHERE, elements: Vec::new() },
                "Add",
                ident("nobody"),
            ),
        ),
        ("a count of a scalar", field(number("1"), "count")),
        ("a field of a scalar", field(number("1"), "status")),
        ("not, over an unbound name", not(ident("nobody"))),
        ("a filter over a scalar", filtered(number("1"), null())),
    ];
    for (described, node) in cases {
        let evaluated = eval(&node, &env);
        if evaluated.value.is_unknown() {
            assert!(
                !evaluated.unresolved.is_empty(),
                "{described} came back undecided with no reason: {node:?}"
            );
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
    let node =
        Expr::Ident(Ident { span: AstSpan { start, end: start + 4 }, name: "copy".to_owned() });
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

fn quantify(binding: &str, collection: Expr, body: Expr, filter: Option<Expr>) -> Expr {
    Expr::For {
        span: NOWHERE,
        binding: ForBinding::Single(Ident { span: NOWHERE, name: binding.to_owned() }),
        collection: Box::new(collection),
        filter: filter.map(Box::new),
        body: Box::new(body),
    }
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

/// And the same plural named from another module: `for m in
/// lending/Members`. Only the bare one was reduced, so the qualified one
/// ranged over an entity called `Members` and every claim about it was
/// vacuous.
#[test]
fn a_collection_in_another_module_is_named_in_the_plural_too() {
    let world = library();
    let env = env(&world, "catalogue");
    let members = Expr::QualifiedName(QualifiedName {
        span: NOWHERE,
        qualifier: Some("lending".to_owned()),
        name: "Members".to_owned(),
    });
    let never_true = quantify("m", members, compare(ident("name"), "Eq", null()), None);
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
    let claim = compare(ident("shelfmark"), "Eq", text("QA77"));
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
    assert_eq!(reasons(&sum, &env), vec!["`+` is not defined between a timestamp and a string"]);
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

fn within(needle: Expr, haystack: Expr) -> Expr {
    Expr::In { span: NOWHERE, element: Box::new(needle), collection: Box::new(haystack) }
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

// --- join lookups --------------------------------------------------------
//
// `exists Membership{group: g, member: m}` is how a specification asks whether
// a relationship is already recorded, and it is most of the interesting rules
// in a real set. The care in here is the third truth value: a candidate whose
// field nobody set has not been ruled *out*.

fn join(entity: Expr, fields: &[(&str, Expr)]) -> Expr {
    Expr::JoinLookup {
        span: NOWHERE,
        entity: Box::new(entity),
        fields: fields
            .iter()
            .map(|(name, value)| JoinField {
                span: NOWHERE,
                field: Ident { span: NOWHERE, name: (*name).to_owned() },
                value: Some(value.clone()),
            })
            .collect(),
    }
}

/// A world with one membership in it, joining `Group#1` and `Member#1`.
fn joined() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new().at(1_000);
    let group = world.create("Group", "membership");
    let member = world.create("Member", "membership");

    let membership = world.create("Membership", "membership");
    world.set_field(&membership, "group", Value::Ref(group.clone()));
    world.set_field(&membership, "member", Value::Ref(member.clone()));

    (world, group, member, membership)
}

#[test]
fn a_join_lookup_finds_the_instance_whose_fields_match() {
    let (world, group, member, membership) = joined();
    let env = env(&world, "membership").bind("g", Value::Ref(group)).bind("m", Value::Ref(member));

    let found = value_of(
        &join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]),
        &env,
    );
    assert_eq!(found, Value::Ref(membership));
}

#[test]
fn a_join_lookup_that_matches_nothing_is_null() {
    let (world, group, _, _) = joined();
    let stranger = EntityId::new("Member", 9);
    let env =
        env(&world, "membership").bind("g", Value::Ref(group)).bind("m", Value::Ref(stranger));

    assert_eq!(
        value_of(
            &join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]),
            &env
        ),
        Value::Null
    );
}

#[test]
fn exists_over_a_join_lookup_answers_the_question_a_spec_asks() {
    let (world, group, member, _) = joined();
    let env = env(&world, "membership")
        .bind("g", Value::Ref(group.clone()))
        .bind("m", Value::Ref(member));

    let lookup = join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup), &env), Truth::True);

    let stranger = join(ident("Membership"), &[("group", ident("g")), ("member", ident("nobody"))]);
    let env = env.bind("nobody", Value::Ref(EntityId::new("Member", 9)));
    assert_eq!(truth_of(&exists(stranger), &env), Truth::False);
}

/// The whole care in the implementation. A membership whose `member` nobody set
/// has not been shown *not* to match, and answering `null` there would report a
/// relationship absent because nothing had looked.
#[test]
fn a_candidate_with_an_unknown_field_leaves_the_lookup_undecided() {
    let mut world = World::new().at(1_000);
    let group = world.create("Group", "membership");
    let membership = world.create("Membership", "membership");
    world.set_field(&membership, "group", Value::Ref(group.clone()));
    // `member` is never set.

    let env = env(&world, "membership")
        .bind("g", Value::Ref(group))
        .bind("m", Value::Ref(EntityId::new("Member", 7)));

    let lookup = join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup.clone()), &env), Truth::Unknown);
    assert!(
        reasons(&lookup, &env).iter().any(|why| why.contains("might match")),
        "{:?}",
        reasons(&lookup, &env)
    );
}

/// A definite mismatch on one field settles it, even with an unknown elsewhere.
#[test]
fn a_field_that_definitely_differs_rules_a_candidate_out() {
    let mut world = World::new().at(1_000);
    let membership = world.create("Membership", "membership");
    world.set_field(&membership, "group", Value::Ref(EntityId::new("Group", 5)));
    world.set_field(&membership, "member", Value::Ref(EntityId::new("Member", 5)));

    let env = env(&world, "membership")
        .bind("g", Value::Ref(EntityId::new("Group", 1)))
        .bind("m", Value::Ref(EntityId::new("Member", 1)));

    let lookup = join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup), &env), Truth::False);
}

#[test]
fn a_lookup_on_a_value_nothing_settled_is_undecided_with_its_reason() {
    let (world, group, _, _) = joined();
    let env = env(&world, "membership").bind("g", Value::Ref(group));

    // `m` is bound to nothing at all.
    let lookup = join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup.clone()), &env), Truth::Unknown);
    assert!(reasons(&lookup, &env).iter().any(|why| why.contains("nothing is bound to `m`")));
}

/// `Receipt{message: m, reporter: r, kind: read}` is how a spec asks whether a
/// read receipt is already recorded, and `read` is a *state* of the field it
/// sits beside. Reading it as a name nothing bound made every such lookup
/// undecided, which in `friend-mesh` was `MarkRead` — a rule nobody could walk.
#[test]
fn a_bare_name_beside_a_field_holding_a_state_is_that_state() {
    let mut world = World::new().at(1_000);
    let message = world.create("Message", "messaging");
    let receipt = world.create("Receipt", "messaging");
    world.set_field(&receipt, "message", Value::Ref(message.clone()));
    world.set_field(&receipt, "kind", Value::Enum("read".to_owned()));

    let env = env(&world, "messaging").bind("m", Value::Ref(message));
    let lookup = join(ident("Receipt"), &[("message", ident("m")), ("kind", ident("read"))]);
    assert_eq!(value_of(&lookup, &env), Value::Ref(receipt));

    // And it is the state that was named, not any state at all.
    let other = join(ident("Receipt"), &[("message", ident("m")), ("kind", ident("delivered"))]);
    assert_eq!(value_of(&other, &env), Value::Null);
}

/// Only against a field that holds one. `Membership{member: m}` with `m` bound
/// to nothing is a name nothing bound, and the field beside it holds a
/// reference — so the state reading has nothing to work from and must not be
/// reached for. Losing that distinction would turn every unbound name into a
/// silent no.
#[test]
fn a_bare_name_beside_a_field_holding_anything_else_stays_undecided() {
    let (world, group, _, _) = joined();
    let env = env(&world, "membership").bind("g", Value::Ref(group));

    let lookup = join(ident("Membership"), &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup.clone()), &env), Truth::Unknown);
    // And the reason it could not be decided is the one a reader can act on,
    // not a sentence about candidates.
    assert!(
        reasons(&lookup, &env).iter().any(|why| why.contains("nothing is bound to `m`")),
        "{:?}",
        reasons(&lookup, &env)
    );
}

/// A world with no candidates at all answers *no match* however the name would
/// have read: there is no receipt to be one. Carrying the held-back reason
/// into that answer would attach a doubt to a settled fact.
#[test]
fn a_name_held_back_is_forgotten_when_nothing_could_have_matched() {
    let mut world = World::new().at(1_000);
    let message = world.create("Message", "messaging");
    let env = env(&world, "messaging").bind("m", Value::Ref(message));

    let lookup = join(ident("Receipt"), &[("message", ident("m")), ("kind", ident("read"))]);
    assert_eq!(value_of(&lookup, &env), Value::Null);
    assert!(reasons(&lookup, &env).is_empty(), "{:?}", reasons(&lookup, &env));
}

#[test]
fn a_qualified_join_lookup_reads_the_same_as_a_bare_one() {
    let (world, group, member, _) = joined();
    let env = env(&world, "messaging").bind("g", Value::Ref(group)).bind("m", Value::Ref(member));

    let qualified = Expr::QualifiedName(QualifiedName {
        span: NOWHERE,
        qualifier: Some("membership".to_owned()),
        name: "Membership".to_owned(),
    });
    let lookup = join(qualified, &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&exists(lookup), &env), Truth::True);
}

#[test]
fn a_join_lookup_over_a_type_with_no_instances_is_null() {
    let world = library();
    let env = env(&world, "lending");
    let lookup = join(ident("Membership"), &[("group", ident("g"))]);
    assert_eq!(value_of(&lookup, &env.bind("g", Value::Int(1))), Value::Null);
}

// --- the qualified lookup upstream hands over as a division ---------------
//
// `exists membership/Membership{…}` parses as a division — see
// `tests/upstream.rs`, which asserts it still does. These say the recovery
// fires on exactly that shape and on nothing else: a recovery that was too
// eager would silently reinterpret real arithmetic, which is worse than the
// misparse it is for.

fn misparsed(module: &str, entity: &str, fields: &[(&str, Expr)]) -> Expr {
    Expr::BinaryOp {
        span: NOWHERE,
        left: Box::new(Expr::Exists { span: NOWHERE, operand: Box::new(ident(module)) }),
        op: BinaryOp::Div,
        right: Box::new(join(ident(entity), fields)),
    }
}

#[test]
fn the_misparsed_qualified_lookup_is_read_as_the_lookup_it_is() {
    let (world, group, member, _) = joined();
    let env =
        env(&world, "messaging").bind("g", Value::Ref(group.clone())).bind("m", Value::Ref(member));

    let found =
        misparsed("membership", "Membership", &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(truth_of(&found, &env), Truth::True);

    // And answers false rather than undecided when nothing matches, which is
    // the whole difference from reporting `nothing is bound to membership`.
    let env = env.bind("nobody", Value::Ref(EntityId::new("Member", 9)));
    let missing = misparsed(
        "membership",
        "Membership",
        &[("group", ident("g")), ("member", ident("nobody"))],
    );
    assert_eq!(truth_of(&missing, &env), Truth::False);
}

/// The recovery must not reinterpret arithmetic that means what it says.
#[test]
fn ordinary_division_is_left_alone() {
    let world = library();
    let env = env(&world, "lending");

    let divided = Expr::BinaryOp {
        span: NOWHERE,
        left: Box::new(number("10")),
        op: BinaryOp::Div,
        right: Box::new(number("2")),
    };
    assert_eq!(value_of(&divided, &env), Value::Int(5));
}

/// Each half of the shape, withheld one at a time.
#[test]
fn nothing_else_is_mistaken_for_a_qualified_lookup() {
    let (world, group, _, _) = joined();
    let env = env(&world, "messaging").bind("g", Value::Ref(group));
    let lookup = join(ident("Membership"), &[("group", ident("g"))]);

    // An operator that is not division: `exists x + Y{…}` is nonsense, and
    // answering it as a lookup would be inventing a reading nobody wrote.
    let added = Expr::BinaryOp {
        span: NOWHERE,
        left: Box::new(Expr::Exists { span: NOWHERE, operand: Box::new(ident("membership")) }),
        op: BinaryOp::Add,
        right: Box::new(lookup.clone()),
    };
    assert_eq!(truth_of(&added, &env), Truth::Unknown);

    // A left side that is not an `exists`.
    let bare = Expr::BinaryOp {
        span: NOWHERE,
        left: Box::new(ident("membership")),
        op: BinaryOp::Div,
        right: Box::new(lookup.clone()),
    };
    assert_eq!(truth_of(&bare, &env), Truth::Unknown);

    // A right side that is not a lookup: real division by a field.
    let over_field = Expr::BinaryOp {
        span: NOWHERE,
        left: Box::new(Expr::Exists { span: NOWHERE, operand: Box::new(ident("membership")) }),
        op: BinaryOp::Div,
        right: Box::new(number("2")),
    };
    assert_eq!(truth_of(&over_field, &env), Truth::Unknown);
}

/// A module name is lower case. `exists Thing / Other{…}` is not a qualified
/// name however it was written, and reading it as one would be a guess.
#[test]
fn a_capitalised_left_hand_name_is_not_a_module() {
    let (world, group, member, _) = joined();
    let env = env(&world, "messaging").bind("g", Value::Ref(group)).bind("m", Value::Ref(member));

    let capitalised =
        misparsed("Membership", "Membership", &[("group", ident("g")), ("member", ident("m"))]);
    assert_eq!(
        truth_of(&capitalised, &env),
        Truth::Unknown,
        "an upper-case left side was read as a module qualifier"
    );
}

// --- sets, with one more or one fewer --------------------------------------

/// `hub.filed_on_by + member` — a set with one more in it. Undefined, it wrote
/// an unknown over the list, so the next rule reading it was undecided too.
#[test]
fn a_set_plus_an_element_has_it_once_and_minus_it_has_it_not() {
    let world = library();
    let (ada, bea) =
        (Value::Ref(EntityId::new("Member", 1)), Value::Ref(EntityId::new("Member", 2)));
    let env = env(&world, "lending")
        .bind("some", Value::Set(vec![ada.clone()]))
        .bind("ada", ada.clone())
        .bind("bea", bea.clone());

    let more = arithmetic(ident("some"), "Add", ident("bea"));
    assert_eq!(value_of(&more, &env), Value::Set(vec![ada.clone(), bea.clone()]));
    let again = arithmetic(ident("some"), "Add", ident("ada"));
    assert_eq!(value_of(&again, &env), Value::Set(vec![ada.clone()]), "never twice");
    let fewer = arithmetic(ident("some"), "Sub", ident("ada"));
    assert_eq!(value_of(&fewer, &env), Value::Set(Vec::new()));
    let union = arithmetic(ident("some"), "Add", set_of(vec![ident("bea"), ident("ada")]));
    assert_eq!(value_of(&union, &env), Value::Set(vec![ada, bea]));
}

// --- states written out as a set ----------------------------------------------

/// `status in {pending, granted}` — beside a state, the names are states.
#[test]
fn a_set_of_bare_names_beside_a_state_is_a_set_of_states() {
    let world = library();
    let env = env(&world, "lending").bind("status", Value::Enum("granted".to_owned()));
    let admitted = within(ident("status"), set_of(vec![ident("pending"), ident("granted")]));
    assert_eq!(truth_of(&admitted, &env), Truth::True);
    assert!(reasons(&admitted, &env).is_empty());

    let refused = within(ident("status"), set_of(vec![ident("pending"), ident("lapsed")]));
    assert_eq!(truth_of(&refused, &env), Truth::False);
}

/// And only beside a state. A number is not a state, so an unbound name next
/// to one is still a name nothing bound.
#[test]
fn a_bare_name_in_a_set_beside_anything_else_is_still_unbound() {
    let world = library();
    let env = env(&world, "lending");
    let node = within(number("1"), set_of(vec![ident("pending")]));
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert_eq!(reasons(&node, &env), vec!["nothing is bound to `pending`"]);
}

/// A binding in the set is the binding's value, state beside it or not.
#[test]
fn a_bound_name_in_a_set_of_states_is_what_it_is_bound_to() {
    let world = library();
    let env = env(&world, "lending")
        .bind("status", Value::Enum("granted".to_owned()))
        .bind("pending", Value::Enum("granted".to_owned()));
    let node = within(ident("status"), set_of(vec![ident("pending")]));
    assert_eq!(truth_of(&node, &env), Truth::True, "`pending` is bound, to `granted`");
}

// --- a derived value that takes an argument --------------------------------------

/// `is_used_by(who): who in filed_on_by`.
fn is_used_by() -> inspect_model::Derivations {
    let mut derived = inspect_model::Derivations::default();
    derived.functions.insert(
        inspect_model::derived_key("lending", "Member", "is_used_by"),
        inspect_model::Function {
            params: vec!["who".to_owned()],
            body: within(ident("who"), ident("filed_on_by")),
        },
    );
    derived
}

#[test]
fn a_derived_value_with_a_parameter_is_computed_for_the_argument_it_is_given() {
    let mut world = library();
    let hub = EntityId::new("Member", 1);
    let (ada, bea) = (Value::Ref(EntityId::new("Copy", 1)), Value::Ref(EntityId::new("Copy", 2)));
    world.set_field(&hub, "filed_on_by", Value::Set(vec![ada.clone()]));
    let derived = is_used_by();
    let env = env(&world, "lending")
        .deriving(&derived)
        .bind("hub", Value::Ref(hub.clone()))
        .bind("ada", ada)
        .bind("bea", bea);

    let asked = |who: &str| call(field(ident("hub"), "is_used_by"), vec![ident(who)]);
    assert_eq!(truth_of(&asked("ada"), &env), Truth::True);
    assert_eq!(truth_of(&asked("bea"), &env), Truth::False);

    // Bare, inside the entity: `this` is the instance it is asked of.
    let inside = env.bind("this", Value::Ref(hub));
    assert_eq!(truth_of(&call(ident("is_used_by"), vec![ident("bea")]), &inside), Truth::False);
}

/// Given the wrong number of arguments, or read without any, it says which.
#[test]
fn a_derived_value_with_a_parameter_asked_wrongly_says_how() {
    let world = library();
    let derived = is_used_by();
    let env = env(&world, "lending")
        .deriving(&derived)
        .bind("hub", Value::Ref(EntityId::new("Member", 1)));

    let too_many = call(field(ident("hub"), "is_used_by"), vec![number("1"), number("2")]);
    assert_eq!(truth_of(&too_many, &env), Truth::Unknown);
    assert!(
        reasons(&too_many, &env)[0].contains("takes 1 argument, and was given 2"),
        "{:?}",
        reasons(&too_many, &env)
    );

    let too_few = call(field(ident("hub"), "is_used_by"), Vec::new());
    assert!(
        reasons(&too_few, &env)[0].contains("takes 1 argument, and was given 0"),
        "{:?}",
        reasons(&too_few, &env)
    );

    let bare = field(ident("hub"), "is_used_by");
    assert_eq!(reasons(&bare, &env)[0], "`is_used_by` takes an argument, and was read without one");
}

/// Named bare inside its own entity, a function is still a function: it asks
/// for an argument, rather than the evaluator saying a lambda is not simulated.
#[test]
fn a_function_named_bare_inside_its_entity_says_it_takes_an_argument() {
    let world = library();
    let derived = is_used_by();
    let env = env(&world, "lending")
        .deriving(&derived)
        .bind("this", Value::Ref(EntityId::new("Member", 1)));
    assert_eq!(
        reasons(&ident("is_used_by"), &env),
        vec!["`is_used_by` takes an argument, and was read without one"]
    );
}

/// A set plus something nothing settled is not a set with an unknown in it:
/// which element it gained is exactly what is not known.
#[test]
fn a_set_plus_something_undecided_is_undecided() {
    let world = library();
    let env = env(&world, "lending").bind("some", Value::Set(Vec::new()));
    let sum = arithmetic(ident("some"), "Add", ident("nobody"));
    assert_eq!(value_of(&sum, &env), Value::Unknown);
    assert_eq!(reasons(&sum, &env), vec!["nothing is bound to `nobody`"]);
}

// --- `all` and `any` over a lambda (issue #7) ----------------------------

fn lambda(param: &str, body: Expr) -> Expr {
    Expr::Lambda { span: NOWHERE, param: Box::new(ident(param)), body: Box::new(body) }
}

/// `object.method(param => body)`.
fn asked(object: Expr, method: &str, param: &str, body: Expr) -> Expr {
    call(field(object, method), vec![lambda(param, body)])
}

/// `c.status = <state>`, the predicate these cases ask of every copy.
fn status_is(state: &str) -> Expr {
    compare(field(ident("c"), "status"), "Eq", ident(state))
}

#[test]
fn all_asks_every_member_and_any_asks_for_one() {
    // The library holds one available copy and one on loan.
    let world = library();
    let env = env(&world, "catalogue");

    let every_available = asked(ident("Copy"), "all", "c", status_is("available"));
    assert_eq!(truth_of(&every_available, &env), Truth::False);
    assert!(reasons(&every_available, &env).is_empty(), "{:?}", reasons(&every_available, &env));

    let one_available = asked(ident("Copy"), "any", "c", status_is("available"));
    assert_eq!(truth_of(&one_available, &env), Truth::True);
    assert!(reasons(&one_available, &env).is_empty());

    let none_lost = asked(ident("Copy"), "any", "c", status_is("lost"));
    assert_eq!(truth_of(&none_lost, &env), Truth::False);

    let all_shelved = asked(
        ident("Copy"),
        "all",
        "c",
        logical(status_is("available"), "Or", status_is("on_loan")),
    );
    assert_eq!(truth_of(&all_shelved, &env), Truth::True);
}

#[test]
fn over_a_stipulated_set_the_members_are_the_ones_it_holds() {
    // The issue's shape: a set somebody said outright, not every instance.
    let world = library();
    let first = Value::Ref(EntityId::new("Copy", 1));
    let env = env(&world, "catalogue").bind("items", Value::Set(vec![first]));
    let node = asked(ident("items"), "all", "c", status_is("available"));
    assert_eq!(truth_of(&node, &env), Truth::True);
    assert!(reasons(&node, &env).is_empty());
}

#[test]
fn over_nothing_all_is_vacuously_true_and_any_is_false() {
    let world = library();
    let env = env(&world, "catalogue").bind("none", Value::Set(Vec::new()));
    assert_eq!(truth_of(&asked(ident("none"), "all", "c", status_is("lost")), &env), Truth::True);
    assert_eq!(truth_of(&asked(ident("none"), "any", "c", status_is("lost")), &env), Truth::False);
}

#[test]
fn a_member_nobody_described_leaves_it_open_only_when_nothing_settled_it() {
    // Copy#3 has no status. Kleene: `all … = available` is already false on
    // Copy#2, and `any … = available` already true on Copy#1, whatever Copy#3
    // is. Asked about `on_loan` alone, though, the answer turns on Copy#3 for
    // `all`, and the reason has to say which field nobody stated.
    let mut world = library();
    world.create("Copy", "catalogue");
    let env = env(&world, "catalogue");

    assert_eq!(
        truth_of(&asked(ident("Copy"), "all", "c", status_is("available")), &env),
        Truth::False
    );
    assert_eq!(
        truth_of(&asked(ident("Copy"), "any", "c", status_is("available")), &env),
        Truth::True
    );

    let pair = Value::Set(vec![
        Value::Ref(EntityId::new("Copy", 2)),
        Value::Ref(EntityId::new("Copy", 3)),
    ]);
    let env = env.bind("pair", pair);
    let open = asked(ident("pair"), "all", "c", status_is("on_loan"));
    assert_eq!(value_of(&open, &env), Value::Unknown);
    let said = reasons(&open, &env);
    assert!(said.iter().any(|why| why == "`Copy#3` has no `status` set"), "{said:?}");
}

#[test]
fn a_member_that_is_itself_unknown_is_named_as_such() {
    // `{x, nobody}` — the second member is the thing nobody settled, and the
    // predicate read on it would otherwise come back undecided in silence.
    let world = library();
    let env = env(&world, "catalogue")
        .bind("items", Value::Set(vec![Value::Ref(EntityId::new("Copy", 1)), Value::Unknown]));
    let node = asked(ident("items"), "all", "c", status_is("available"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    let said = reasons(&node, &env);
    assert!(
        said.iter()
            .any(|why| why == "a member of this collection is not known, so `all` cannot check it"),
        "{said:?}"
    );
}

#[test]
fn a_predicate_that_is_not_a_condition_says_so() {
    let world = library();
    let env = env(&world, "catalogue");
    let node = asked(ident("Copy"), "any", "c", field(ident("c"), "shelfmark"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(
        reasons(&node, &env),
        ["a string is not a condition", "a string is not a condition"]
    );
}

#[test]
fn a_collection_nothing_settled_stays_undecided_and_says_why() {
    let world = library();
    let env = env(&world, "catalogue");
    let node = asked(ident("nowhere"), "all", "c", status_is("available"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(reasons(&node, &env), ["nothing is bound to `nowhere`"]);
}

#[test]
fn all_on_something_that_is_not_a_collection_says_so() {
    let world = library();
    let env = env(&world, "catalogue").bind("n", Value::Int(3));
    let node = asked(ident("n"), "all", "c", status_is("available"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(
        reasons(&node, &env),
        ["`all` asks about every member of a collection, and was called on an integer"]
    );
}

#[test]
fn a_lambda_with_a_parameter_that_is_not_a_name_is_named_as_not_simulated() {
    let world = library();
    let env = env(&world, "catalogue");
    let node = call(
        field(ident("Copy"), "all"),
        vec![Expr::Lambda {
            span: NOWHERE,
            param: Box::new(number("1")),
            body: Box::new(status_is("available")),
        }],
    );
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert_eq!(
        reasons(&node, &env),
        ["a lambda whose parameter is not a single name is not simulated"]
    );
}

#[test]
fn another_method_taking_a_lambda_is_still_not_guessed_at() {
    // Only `all` and `any` are quantifiers. `filter` is not modelled, and
    // stays undecided with a reason rather than being read as either.
    let world = library();
    let env = env(&world, "catalogue");
    let node = asked(ident("Copy"), "filter", "c", status_is("available"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    let said = reasons(&node, &env);
    assert!(said.iter().any(|why| why == "a lambda is not simulated"), "{said:?}");
}

#[test]
fn the_lambda_s_name_does_not_leak_or_shadow_outside_it() {
    // `c` bound outside is the outer `c` again after the lambda, and inside the
    // lambda the element's fields are not bare names: `status` here is the
    // outer binding, not the copy's.
    let world = library();
    let env = env(&world, "catalogue")
        .bind("c", Value::Int(7))
        .bind("status", Value::Enum("lost".to_owned()));
    let inner = compare(ident("status"), "Eq", ident("lost"));
    let node = asked(ident("Copy"), "all", "c", inner);
    assert_eq!(truth_of(&node, &env), Truth::True);
    assert_eq!(value_of(&ident("c"), &env), Value::Int(7));
}

#[test]
fn an_undecided_collection_or_predicate_that_said_nothing_is_given_a_reason() {
    // Both shapes reach the quantifier as an unknown with no reason attached —
    // a binding holding an unknown is read without complaint — and an unknown
    // with no reason is indistinguishable from a bug.
    let world = library();
    let env = env(&world, "catalogue").bind("items", Value::Unknown).bind("flag", Value::Unknown);

    let over_nothing_known = asked(ident("items"), "all", "c", status_is("available"));
    assert_eq!(value_of(&over_nothing_known, &env), Value::Unknown);
    assert_eq!(
        reasons(&over_nothing_known, &env),
        ["`all` was asked of something nothing settled"]
    );

    let silent = asked(ident("Copy"), "any", "c", ident("flag"));
    assert_eq!(value_of(&silent, &env), Value::Unknown);
    assert_eq!(
        reasons(&silent, &env),
        [
            "the predicate could not be decided for `Copy#1`",
            "the predicate could not be decided for `Copy#2`"
        ]
    );
}

#[test]
fn all_asked_of_one_instance_is_left_to_the_function_its_entity_defines() {
    // `copy.all(…)` is not a quantifier — `copy` is not a collection — so it is
    // the call path's question, and with no function of that name it stays
    // undecided with that path's reasons.
    let world = library();
    let env = env(&world, "catalogue").bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = asked(ident("copy"), "all", "c", status_is("available"));
    assert_eq!(value_of(&node, &env), Value::Unknown);
    let said = reasons(&node, &env);
    assert!(said.iter().any(|why| why == "a function call is not simulated"), "{said:?}");
}

// --- `all`, `any` and `for` over a relationship the spec computes ----------

/// `entity Box { items: Item with box = this }`, and a world with one box,
/// `x` placed in it and of kind a, and `y` of kind b that nobody placed
/// anywhere.
fn packed() -> (World, inspect_model::Derivations) {
    let mut derived = inspect_model::Derivations::default();
    derived.fields.insert(
        inspect_model::derived_key("packing", "Box", "items"),
        filtered(ident("Item"), compare(ident("box"), "Eq", ident("this"))),
    );
    let mut world = World::new();
    let bx = world.create("Box", "packing");
    let x = world.create("Item", "packing");
    world.set_field(&x, "box", Value::Ref(bx));
    world.set_field(&x, "kind", Value::Enum("a".to_owned()));
    let y = world.create("Item", "packing");
    world.set_field(&y, "kind", Value::Enum("b".to_owned()));
    (world, derived)
}

fn kind_is(state: &str) -> Expr {
    compare(field(ident("i"), "kind"), "Eq", ident(state))
}

#[test]
fn a_relationship_missing_a_member_it_could_not_decide_on_settles_nothing_by_omission() {
    // `y` is of kind b and nothing says whether it is in the box. Leaving it
    // out and answering over `x` alone would read `all … = a` as true and
    // `any … = b` as false — conclusions nobody reached.
    let (world, derived) = packed();
    let env =
        env(&world, "packing").deriving(&derived).bind("bx", Value::Ref(EntityId::new("Box", 1)));
    let items = || field(ident("bx"), "items");

    for node in [
        asked(items(), "all", "i", kind_is("a")),
        asked(items(), "any", "i", kind_is("b")),
        quantify("i", items(), kind_is("a"), None),
    ] {
        assert_eq!(value_of(&node, &env), Value::Unknown, "{node:?}");
        let said = reasons(&node, &env);
        assert!(said.iter().any(|why| why.contains("`box`")), "names y's field: {said:?}");
        assert!(
            said.iter().any(|why| why.starts_with("this collection may be missing members")),
            "{said:?}"
        );
    }

    // What a member it does hold settles is still settled.
    assert_eq!(truth_of(&asked(items(), "all", "i", kind_is("b")), &env), Truth::False);
    assert_eq!(truth_of(&asked(items(), "any", "i", kind_is("a")), &env), Truth::True);
    assert_eq!(truth_of(&quantify("i", items(), kind_is("b"), None), &env), Truth::False);
}

#[test]
fn a_relationship_whose_every_candidate_was_decided_is_complete() {
    let (mut world, derived) = packed();
    let elsewhere = world.create("Box", "packing");
    world.set_field(&EntityId::new("Item", 2), "box", Value::Ref(elsewhere));
    let env =
        env(&world, "packing").deriving(&derived).bind("bx", Value::Ref(EntityId::new("Box", 1)));
    let items = || field(ident("bx"), "items");
    assert_eq!(truth_of(&asked(items(), "all", "i", kind_is("a")), &env), Truth::True);
    assert_eq!(truth_of(&asked(items(), "any", "i", kind_is("b")), &env), Truth::False);
    assert_eq!(truth_of(&quantify("i", items(), kind_is("a"), None), &env), Truth::True);
}

/// `count`, `exists` and `in` over the same relationship ask the same thing
/// of it that `all` does: `y` may be in the box, so `items.count = 1`, `not
/// exists items` and `y not in items` were conclusions nobody reached.
#[test]
fn a_relationship_missing_a_member_has_no_count_and_no_absence() {
    let (world, derived) = packed();
    let env =
        env(&world, "packing").deriving(&derived).bind("bx", Value::Ref(EntityId::new("Box", 1)));
    let items = || field(ident("bx"), "items");
    let env = env.bind("y", Value::Ref(EntityId::new("Item", 2)));
    let env = env.bind("x", Value::Ref(EntityId::new("Item", 1)));

    let count = field(items(), "count");
    assert_eq!(value_of(&count, &env), Value::Unknown);
    let said = reasons(&count, &env);
    assert!(said.iter().any(|why| why.contains("its `count` is not known")), "{said:?}");
    assert!(said.iter().any(|why| why.contains("`box`")), "names y's field: {said:?}");

    // Absent from what it holds is not absent.
    assert_eq!(truth_of(&within(ident("y"), items()), &env), Truth::Unknown);
    assert!(
        reasons(&within(ident("y"), items()), &env)
            .iter()
            .any(|why| why.starts_with("this collection may be missing members")),
    );
    // What it does hold settles the rest.
    assert_eq!(truth_of(&within(ident("x"), items()), &env), Truth::True);
    assert_eq!(truth_of(&exists(items()), &env), Truth::True);

    // Nothing held, from a collection that may have left `y` out.
    let only_b = filtered(items(), kind_is_bare("b"));
    assert_eq!(truth_of(&exists(only_b), &env), Truth::Unknown);
}

/// The same three, once every candidate was decided, are answered.
#[test]
fn a_complete_relationship_has_a_count_and_an_absence() {
    let (mut world, derived) = packed();
    let elsewhere = world.create("Box", "packing");
    world.set_field(&EntityId::new("Item", 2), "box", Value::Ref(elsewhere));
    let env = env(&world, "packing")
        .deriving(&derived)
        .bind("bx", Value::Ref(EntityId::new("Box", 1)))
        .bind("y", Value::Ref(EntityId::new("Item", 2)));
    let items = || field(ident("bx"), "items");
    assert_eq!(value_of(&field(items(), "count"), &env), Value::Int(1));
    assert_eq!(truth_of(&within(ident("y"), items()), &env), Truth::False);
    assert_eq!(truth_of(&exists(filtered(items(), kind_is_bare("b"))), &env), Truth::False);
}

fn kind_is_bare(state: &str) -> Expr {
    compare(ident("kind"), "Eq", ident(state))
}

#[test]
fn a_relationship_is_computed_in_its_own_scope_not_the_caller_s() {
    // The rule's parameter is called `box`, and so is the field `y` never had
    // set. Inside the definition `box` is `y`'s field or nothing — never the
    // rule's argument — so the verdict cannot turn on what the parameter is
    // called.
    let (world, derived) = packed();
    let the_box = Value::Ref(EntityId::new("Box", 1));
    let named_bx = env(&world, "packing").deriving(&derived).bind("bx", the_box.clone());
    let named_box = env(&world, "packing").deriving(&derived).bind("box", the_box);
    let all_a = |name: &str| asked(field(ident(name), "items"), "all", "i", kind_is("a"));
    assert_eq!(value_of(&all_a("box"), &named_box), value_of(&all_a("bx"), &named_bx));
    assert_eq!(value_of(&all_a("box"), &named_box), Value::Unknown);
}

#[test]
fn a_filter_that_could_not_decide_leaves_for_open_rather_than_vacuous() {
    // `for c in Copy where nobody = 1: …` — the filter decides nothing, so
    // nothing is known to be claimed about, which is not the same as nothing
    // being claimed about.
    let world = library();
    let env = env(&world, "catalogue");
    let node = quantify(
        "c",
        ident("Copy"),
        status_is("lost"),
        Some(compare(ident("nobody"), "Eq", number("1"))),
    );
    assert_eq!(value_of(&node, &env), Value::Unknown);
}

#[test]
fn the_new_reasons_point_at_the_source_they_are_about() {
    // A multibyte character first, so a span counted in anything but bytes
    // lands somewhere else (stipulation 4).
    let source = "-- an em\u{2014}dash\nrequires: n.all(c => c.status = available)";
    let world = library();
    let at = |text: &str| {
        let start = source.find(text).expect("present");
        AstSpan { start, end: start + text.len() }
    };
    let name = |text: &str, span: AstSpan| Expr::Ident(Ident { span, name: text.to_owned() });
    let first = |text: &str| AstSpan { start: at(text).start, end: at(text).start + 1 };
    let receiver = name("n", first("n.all"));
    let body = Expr::Comparison {
        span: at("c.status = available"),
        left: Box::new(Expr::MemberAccess {
            span: at("c.status"),
            object: Box::new(name("c", first("c.status"))),
            field: Ident { span: NOWHERE, name: "status".to_owned() },
        }),
        op: comparison_op("Eq"),
        right: Box::new(name("available", at("available"))),
    };
    let lambda_at = at("c => c.status = available");
    let param_at = first("c => c.status = available");
    let whole = |param: Expr| Expr::Call {
        span: at("n.all(c => c.status = available)"),
        function: Box::new(Expr::MemberAccess {
            span: at("n.all"),
            object: Box::new(receiver.clone()),
            field: Ident { span: NOWHERE, name: "all".to_owned() },
        }),
        args: vec![allium_parser::ast::CallArg::Positional(Expr::Lambda {
            span: lambda_at,
            param: Box::new(param),
            body: Box::new(body.clone()),
        })],
    };
    let quoted = |env: &Env<'_>, param: Expr| -> Vec<(String, Option<String>)> {
        eval(&whole(param), env)
            .unresolved
            .into_iter()
            .map(|note| (note.reason, note.expression))
            .collect()
    };
    let c = || name("c", param_at);

    // Not a collection: the whole call.
    let env = Env::new(&world, "catalogue", source).bind("n", Value::Int(3));
    assert_eq!(
        quoted(&env, c()),
        [(
            "`all` asks about every member of a collection, and was called on an integer"
                .to_owned(),
            Some("n.all(c => c.status = available)".to_owned())
        )]
    );

    // A collection undecided in silence: the collection.
    let env = Env::new(&world, "catalogue", source).bind("n", Value::Unknown);
    assert_eq!(
        quoted(&env, c()),
        [("`all` was asked of something nothing settled".to_owned(), Some("n".to_owned()))]
    );

    // A member that is itself unknown: the collection.
    let env = Env::new(&world, "catalogue", source).bind("n", Value::Set(vec![Value::Unknown]));
    let said = quoted(&env, c());
    assert!(
        said.contains(&(
            "a member of this collection is not known, so `all` cannot check it".to_owned(),
            Some("n".to_owned())
        )),
        "{said:?}"
    );

    // A predicate undecided in silence, and one that is not a condition: the
    // predicate.
    let silent = Env::new(&world, "catalogue", source)
        .bind("n", Value::Set(vec![Value::Ref(EntityId::new("Copy", 1))]));
    let silent_body = |env: &Env<'_>, body: Expr| -> Vec<(String, Option<String>)> {
        let node = Expr::Call {
            span: at("n.all(c => c.status = available)"),
            function: Box::new(Expr::MemberAccess {
                span: at("n.all"),
                object: Box::new(receiver.clone()),
                field: Ident { span: NOWHERE, name: "all".to_owned() },
            }),
            args: vec![allium_parser::ast::CallArg::Positional(Expr::Lambda {
                span: lambda_at,
                param: Box::new(c()),
                body: Box::new(body),
            })],
        };
        eval(&node, env).unresolved.into_iter().map(|note| (note.reason, note.expression)).collect()
    };
    let flag = Env::new(&world, "catalogue", source)
        .bind("n", Value::Set(vec![Value::Ref(EntityId::new("Copy", 1))]))
        .bind("flag", Value::Unknown);
    assert_eq!(
        silent_body(&flag, name("flag", at("c.status = available"))),
        [(
            "the predicate could not be decided for `Copy#1`".to_owned(),
            Some("c.status = available".to_owned())
        )]
    );
    let shelfmark = Expr::MemberAccess {
        span: at("c.status = available"),
        object: Box::new(c()),
        field: Ident { span: NOWHERE, name: "shelfmark".to_owned() },
    };
    assert_eq!(
        silent_body(&silent, shelfmark),
        [("a string is not a condition".to_owned(), Some("c.status = available".to_owned()))]
    );

    // A parameter that is not a name: the parameter.
    let not_a_name = Expr::NumberLiteral { span: param_at, value: "1".to_owned() };
    assert_eq!(
        quoted(&silent, not_a_name),
        [(
            "a lambda whose parameter is not a single name is not simulated".to_owned(),
            Some("c".to_owned())
        )]
    );
}

#[test]
fn a_predicate_that_said_why_is_not_given_a_second_vaguer_reason() {
    let mut world = library();
    world.create("Copy", "catalogue");
    let env = env(&world, "catalogue").bind(
        "pair",
        Value::Set(vec![
            Value::Ref(EntityId::new("Copy", 2)),
            Value::Ref(EntityId::new("Copy", 3)),
        ]),
    );
    let said = reasons(&asked(ident("pair"), "all", "c", status_is("on_loan")), &env);
    assert!(said.iter().any(|why| why == "`Copy#3` has no `status` set"), "{said:?}");
    assert!(
        !said.iter().any(|why| why.starts_with("the predicate could not be decided")),
        "{said:?}"
    );
}

#[test]
fn an_entity_that_defines_all_has_its_own_called() {
    // `copy.all(…)` on one instance is that entity's function, when it has
    // one, and not the quantifier.
    let world = library();
    let mut derived = inspect_model::Derivations::default();
    derived.functions.insert(
        inspect_model::derived_key("catalogue", "Copy", "all"),
        inspect_model::Function {
            params: vec!["p".to_owned()],
            body: compare(field(ident("this"), "status"), "Eq", ident("available")),
        },
    );
    let env = env(&world, "catalogue")
        .deriving(&derived)
        .bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = asked(ident("copy"), "all", "c", status_is("lost"));
    assert_eq!(truth_of(&node, &env), Truth::True, "{:?}", reasons(&node, &env));
}

// --- a black box called with a word nothing binds -------------------------------

/// A world where somebody said what `can(Copy#1, code)` comes back as.
fn told_can(answer: bool) -> World {
    let mut world = library();
    world.answers.push(inspect_sim::world::Answer {
        call: "can".to_owned(),
        arguments: vec![Value::Ref(EntityId::new("Copy", 1)), Value::Enum("code".to_owned())],
        value: Value::Bool(answer),
    });
    world
}

/// `can(repo, code)`, with `repo` bound to `Copy#1`.
fn can(word: &str) -> Expr {
    call(ident("can"), vec![ident("repo"), ident(word)])
}

/// `code` is bound to nothing, and the journey that stipulated `can(r, code)`
/// read its own `code` as the state it spells. The two are the same call.
#[test]
fn a_stipulation_naming_the_same_word_answers_a_call_with_an_unbound_argument() {
    for answer in [true, false] {
        let world = told_can(answer);
        let env = env(&world, "lending").bind("repo", Value::Ref(EntityId::new("Copy", 1)));
        let evaluated = eval(&can("code"), &env);
        assert_eq!(evaluated.value, Value::Bool(answer), "{:?}", evaluated.unresolved);
        assert!(evaluated.unresolved.is_empty(), "settled: {:?}", evaluated.unresolved);
    }
}

/// A stipulated answer that is itself unknown settles nothing, and the call
/// says so: an unknown with no reason is indistinguishable from a bug.
#[test]
fn a_stipulated_answer_with_no_value_says_why_the_call_is_undecided() {
    let mut world = library();
    world.answers.push(inspect_sim::world::Answer {
        call: "can".to_owned(),
        arguments: vec![Value::Ref(EntityId::new("Copy", 1)), Value::Enum("code".to_owned())],
        value: Value::Unknown,
    });
    // `code` read as its spelling, and `how` bound to the state itself: the
    // two ways a stipulation is matched.
    let env = env(&world, "lending")
        .bind("repo", Value::Ref(EntityId::new("Copy", 1)))
        .bind("how", Value::Enum("code".to_owned()));
    for word in ["code", "how"] {
        let evaluated = eval(&can(word), &env);
        assert_eq!(evaluated.truth(), Truth::Unknown, "{word}");
        let said: Vec<String> = evaluated.unresolved.into_iter().map(|note| note.reason).collect();
        assert!(
            said.iter().any(|why| why == "the answer stipulated for `can` has no value"),
            "{word}: {said:?}"
        );
    }
}

/// An argument that holds something nobody knows, inside a set, is as
/// unsettled as one that is unknown outright: `can({x.f}, code)` with `x.f`
/// unset is not a question about the set of one unknown thing.
#[test]
fn an_argument_holding_an_unknown_matches_no_stipulation() {
    let mut world = library();
    world.answers.push(inspect_sim::world::Answer {
        call: "can".to_owned(),
        arguments: vec![Value::Set(vec![Value::Unknown])],
        value: Value::Bool(true),
    });
    assert_eq!(world.answer("can", &[Value::Set(vec![Value::Unknown])]), None);
}

/// Another word is another call. Answering it would be the simulator inventing
/// a policy nobody stated.
#[test]
fn a_stipulation_about_another_word_does_not_answer_it() {
    let world = told_can(true);
    let env = env(&world, "lending").bind("repo", Value::Ref(EntityId::new("Copy", 1)));
    assert_eq!(truth_of(&can("issues"), &env), Truth::Unknown);
    let said = reasons(&can("issues"), &env);
    assert!(said.iter().any(|why| why == "nothing is bound to `issues`"), "{said:?}");
}

/// About another repo, likewise: the word matching is not the call matching.
#[test]
fn a_stipulation_about_the_same_word_and_another_instance_does_not_answer_it() {
    let world = told_can(true);
    let env = env(&world, "lending").bind("repo", Value::Ref(EntityId::new("Copy", 2)));
    assert_eq!(truth_of(&can("code"), &env), Truth::Unknown);
}

/// A name that *is* bound — to a value nobody knows — is a question about that
/// value, not about a state called `code`. Matching it would answer
/// `can(repo, how)` with whatever was said about a state spelled `how`.
#[test]
fn a_bound_name_is_never_read_as_its_spelling() {
    let world = told_can(true);
    let env = env(&world, "lending")
        .bind("repo", Value::Ref(EntityId::new("Copy", 1)))
        .bind("code", Value::Unknown);
    assert_eq!(truth_of(&can("code"), &env), Truth::Unknown);
}

/// Something else in the call still undecided keeps the call undecided, and
/// keeps its own reason.
#[test]
fn a_spelling_does_not_settle_an_argument_nobody_bound_elsewhere() {
    let world = told_can(true);
    let env = env(&world, "lending");
    let evaluated = eval(&can("code"), &env);
    assert_eq!(evaluated.truth(), Truth::Unknown);
    let said: Vec<String> = evaluated.unresolved.into_iter().map(|note| note.reason).collect();
    assert!(said.iter().any(|why| why == "nothing is bound to `repo`"), "{said:?}");
}

/// A world where somebody said what `can(Copy#1, <word>)` comes back as.
fn told_can_about(word: &str) -> World {
    let mut world = library();
    world.answers.push(inspect_sim::world::Answer {
        call: "can".to_owned(),
        arguments: vec![Value::Ref(EntityId::new("Copy", 1)), Value::Enum(word.to_owned())],
        value: Value::Bool(true),
    });
    world
}

/// `Copies` is the collection of copies, not a state spelled `Copies`. A name
/// that already reads as something is that thing, and a stipulation about its
/// spelling is about something else.
#[test]
fn a_name_that_already_reads_as_something_is_not_its_spelling() {
    let world = told_can_about("Copies");
    let env = env(&world, "lending").bind("repo", Value::Ref(EntityId::new("Copy", 1)));
    assert_eq!(truth_of(&can("Copies"), &env), Truth::Unknown);
}

/// Inside an entity's own definition a bare name may be one of its fields that
/// nobody set, which reads exactly like a word nothing binds. `allowed:
/// can(this, level)` with `level` unset is a question about an unknown number,
/// and a stipulation about a state spelled `level` must not answer it.
#[test]
fn inside_an_entity_a_bare_name_is_never_read_as_its_spelling() {
    let world = told_can_about("level");
    let env = env(&world, "lending")
        .bind("repo", Value::Ref(EntityId::new("Copy", 1)))
        .bind("this", Value::Ref(EntityId::new("Copy", 1)));
    assert_eq!(truth_of(&can("level"), &env), Truth::Unknown);
    // `Copy` defines no `can`, so this is the same black box it is outside an
    // entity, and says the same thing about it.
    let said = reasons(&can("level"), &env);
    assert_eq!(
        said.first().map(String::as_str),
        Some("nothing defines `can`, and no stipulation answers this call"),
        "{said:?}"
    );
}

/// A parameter of the rule that nobody passed is a question about what would
/// have been passed, not about a state with its name.
#[test]
fn a_parameter_nobody_passed_is_never_read_as_its_spelling() {
    let world = told_can_about("kind");
    let env = env(&world, "lending")
        .bind("repo", Value::Ref(EntityId::new("Copy", 1)))
        .declaring(["repo".to_owned(), "kind".to_owned()]);
    assert_eq!(truth_of(&can("kind"), &env), Truth::Unknown);
    let said = reasons(&can("kind"), &env);
    assert!(said.iter().any(|why| why == "nothing is bound to `kind`"), "{said:?}");
}

/// A call nobody defined and nobody answered says so, and says what would
/// answer it — the generic "not simulated" sent readers looking for a missing
/// feature when what was missing was a stipulation.
#[test]
fn an_unanswered_black_box_says_a_stipulation_would_answer_it() {
    let world = library();
    let env = env(&world, "lending").bind("repo", Value::Ref(EntityId::new("Copy", 1)));
    let said = reasons(&can("code"), &env);
    assert_eq!(
        said.first().map(String::as_str),
        Some("nothing defines `can`, and no stipulation answers this call"),
        "{said:?}"
    );
}

// --- words in a definition (issue #21) ---------------------------------------

/// `if condition: then else: otherwise`.
fn if_else(condition: Expr, then: Expr, otherwise: Expr) -> Expr {
    Expr::Conditional {
        span: NOWHERE,
        branches: vec![allium_parser::ast::CondBranch { span: NOWHERE, condition, body: then }],
        else_body: Some(Box::new(otherwise)),
    }
}

/// A run, `status: waiting | running` and `other: waiting | running`, with
/// whatever `definitions` it computes, and `waiting`, `running` declared.
fn run_spec(definitions: Vec<(&str, Expr)>) -> inspect_model::Derivations {
    let mut derived = inspect_model::Derivations::default();
    for stored in ["status", "other"] {
        derived.stored.insert(inspect_model::derived_key("ci", "Run", stored));
    }
    derived.states.extend(["waiting".to_owned(), "running".to_owned()]);
    for (name, definition) in definitions {
        derived.fields.insert(inspect_model::derived_key("ci", "Run", name), definition);
    }
    derived
}

/// One run, waiting, with `other` unset.
fn waiting_run() -> (World, EntityId) {
    let mut world = World::new();
    let run = world.create("Run", "ci");
    world.set_field(&run, "status", Value::Enum("waiting".to_owned()));
    (world, run)
}

fn waiting_or_running() -> Expr {
    if_else(compare(ident("status"), "Eq", ident("waiting")), ident("waiting"), ident("running"))
}

#[test]
fn a_word_a_definition_returns_is_the_state_the_spec_declares() {
    // `agg: if status = waiting: waiting else: running`. The condition always
    // read `waiting` as a state, because `status` on the other side said so;
    // the branch it chose had nothing beside it and came back undecided.
    let (world, run) = waiting_run();
    let derived = run_spec(vec![("agg", waiting_or_running())]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run));
    let node = field(ident("r"), "agg");
    assert_eq!(value_of(&node, &env), Value::Enum("waiting".to_owned()));
    assert!(reasons(&node, &env).is_empty(), "and nothing about it was undecided");
}

#[test]
fn a_word_no_enumeration_declares_stays_undecided_in_a_definition() {
    // Only a declared state. `paused` is in no enumeration, so it may be
    // something this simulator does not model, and it says which word.
    let (world, run) = waiting_run();
    let derived = run_spec(vec![("agg", ident("paused"))]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run));
    let node = field(ident("r"), "agg");
    assert_eq!(value_of(&node, &env), Value::Unknown);
    assert!(
        reasons(&node, &env).iter().any(|reason| reason == "nothing is bound to `paused`"),
        "{:?}",
        reasons(&node, &env)
    );
}

#[test]
fn a_word_outside_a_definition_is_still_read_only_beside_a_state() {
    // The new reading is for a definition, where `this` is in scope. A rule's
    // bare word with nothing beside it is still a name nothing binds.
    let (world, _) = waiting_run();
    let derived = run_spec(Vec::new());
    let env = env(&world, "ci").deriving(&derived);
    assert_eq!(value_of(&ident("waiting"), &env), Value::Unknown);
}

#[test]
fn a_stored_field_nobody_set_is_not_taken_for_a_state() {
    // `ne: other != status`. `other` is unset, so it is not bound inside the
    // definition — and `status` beside it made the comparison read it as the
    // state `other`, which is not `waiting`, so `true`. A field nobody set is
    // undecided, and says which.
    let (world, run) = waiting_run();
    let ne = compare(ident("other"), "NotEq", ident("status"));
    let derived = run_spec(vec![("ne", ne)]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run.clone()));
    let node = field(ident("r"), "ne");
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    let said = reasons(&node, &env);
    assert!(said.iter().any(|reason| reason == &format!("`{run}` has no `other` set")), "{said:?}");
}

#[test]
fn a_stored_field_nobody_set_is_undecided_even_when_it_is_spelled_like_a_state() {
    // A field called the same as a state is still the field. Reading the word
    // would answer a question about the field with its own name.
    let (world, run) = waiting_run();
    let mut derived = run_spec(vec![("same", compare(ident("running"), "Eq", ident("status")))]);
    derived.stored.insert(inspect_model::derived_key("ci", "Run", "running"));
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run));
    assert_eq!(truth_of(&field(ident("r"), "same"), &env), Truth::Unknown);
}

#[test]
fn an_undecided_derived_value_is_not_taken_for_a_state() {
    // `stale: agg != status` with `agg` undecided — here because it reads
    // `other`, which nobody set. Kleene logic makes the comparison undecided;
    // the name `agg` is not a state called `agg`.
    let (world, run) = waiting_run();
    let agg = if_else(
        compare(ident("other"), "Eq", ident("waiting")),
        ident("waiting"),
        ident("running"),
    );
    let stale = compare(ident("agg"), "NotEq", ident("status"));
    let derived = run_spec(vec![("agg", agg), ("stale", stale)]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run.clone()));
    let node = field(ident("r"), "stale");
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    let said = reasons(&node, &env);
    assert!(said.iter().any(|reason| reason == &format!("`{run}` has no `other` set")), "{said:?}");
}

#[test]
fn a_decided_derived_value_compares_as_the_state_it_is() {
    // And the issue's own case, end to end: `stale: agg != status` is false
    // once `agg` is `waiting`.
    let (world, run) = waiting_run();
    let stale = compare(ident("agg"), "NotEq", ident("status"));
    let derived = run_spec(vec![("agg", waiting_or_running()), ("stale", stale)]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run));
    let node = field(ident("r"), "stale");
    assert_eq!(truth_of(&node, &env), Truth::False);
    assert!(reasons(&node, &env).is_empty());
}

#[test]
fn a_rule_parameter_nobody_passed_is_not_taken_for_a_state() {
    // `copy.status = how` with `how` a parameter of the rule that nobody
    // passed. Read as the state `how`, the comparison answered `false` about
    // an argument it never saw.
    let world = library();
    let env = env(&world, "catalogue")
        .declaring(["how".to_owned()])
        .bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = compare(field(ident("copy"), "status"), "Eq", ident("how"));
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    assert_eq!(reasons(&node, &env), ["nothing is bound to `how`"]);
}

#[test]
fn a_name_bound_to_an_undecided_value_is_not_taken_for_a_state() {
    // `copy.status != x` with `x` a `let` the rule could not decide. Read as
    // the state `x`, it is not `available`, so `true` — an answer about a
    // value nobody knows.
    let world = library();
    let env = env(&world, "catalogue")
        .bind("x", Value::Unknown)
        .bind("copy", Value::Ref(EntityId::new("Copy", 1)));
    let node = compare(field(ident("copy"), "status"), "NotEq", ident("x"));
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
}

#[test]
fn a_function_read_bare_is_not_taken_for_a_state() {
    // `odd: f != status` with `f(x)` a function of the run. Read without its
    // argument it is undecided; read as the state `f` it was `true`.
    let (world, run) = waiting_run();
    let mut derived = run_spec(vec![("odd", compare(ident("f"), "NotEq", ident("status")))]);
    derived.functions.insert(
        inspect_model::derived_key("ci", "Run", "f"),
        inspect_model::Function { params: vec!["x".to_owned()], body: ident("x") },
    );
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run));
    let node = field(ident("r"), "odd");
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    let said = reasons(&node, &env);
    assert!(
        said.iter().any(|reason| reason == "`f` takes an argument, and was read without one"),
        "{said:?}"
    );
}

#[test]
fn a_stored_field_nobody_set_is_undecided_on_either_side() {
    // `status != other` as well as `other != status`: the state reading is
    // tried from both sides, so both are refused it.
    let (world, run) = waiting_run();
    let derived = run_spec(vec![("ne_r", compare(ident("status"), "NotEq", ident("other")))]);
    let env = env(&world, "ci").deriving(&derived).bind("r", Value::Ref(run.clone()));
    let node = field(ident("r"), "ne_r");
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    let said = reasons(&node, &env);
    assert!(said.iter().any(|reason| reason == &format!("`{run}` has no `other` set")), "{said:?}");
}

#[test]
fn a_filter_in_a_rule_leaves_out_an_element_whose_field_nobody_set_and_says_so() {
    // `Runs where other != status` in a rule: each run is `this` inside the
    // filter, so an unset `other` is that run's field, not the state `other`.
    // The run is neither kept nor counted out, and the note says why.
    let (world, run) = waiting_run();
    let derived = run_spec(Vec::new());
    let env = env(&world, "ci").deriving(&derived);
    let node = filtered(ident("Runs"), compare(ident("other"), "NotEq", ident("status")));
    let evaluation = eval(&node, &env);
    assert_eq!(evaluation.value, Value::Set(Vec::new()));
    let said: Vec<_> = evaluation.unresolved.iter().map(|note| note.reason.clone()).collect();
    assert_eq!(said, [format!("`{run}` has no `other` set")]);
    let counted = field(node, "count");
    assert_eq!(value_of(&counted, &env), Value::Unknown, "and so its count is not known");
}

#[test]
fn a_rule_parameter_spelled_like_a_state_is_not_read_as_one_in_a_filter() {
    // `Runs where status = waiting` in a rule that declares, and was not
    // passed, a parameter called `waiting`. The filter makes each run `this`,
    // which is where a declared word becomes a state — but not one the rule
    // has given another meaning.
    let (world, _) = waiting_run();
    let derived = run_spec(Vec::new());
    let env = env(&world, "ci").deriving(&derived).declaring(["waiting".to_owned()]);
    let node = filtered(ident("Runs"), compare(ident("status"), "Eq", ident("waiting")));
    let evaluation = eval(&node, &env);
    assert_eq!(evaluation.value, Value::Set(Vec::new()));
    let said: Vec<_> = evaluation.unresolved.iter().map(|note| note.reason.clone()).collect();
    assert_eq!(said, ["nothing is bound to `waiting`"]);
}

#[test]
fn a_definition_s_filter_asks_the_element_for_its_field_not_this() {
    // On a shelf, `live: docs where status = active`, over a doc that never set
    // `status`. `this` is the shelf, which has no `status`; the doc does, and
    // it is unset. Read off the shelf alone, `status` passed for a word, the
    // state `status` is not `active`, and `has_live` was a definite false.
    let mut world = World::new();
    let shelf = world.create("Shelf", "ci");
    let doc = world.create("Doc", "ci");
    world.set_field(&doc, "shelf", Value::Ref(shelf.clone()));
    let mut derived = inspect_model::Derivations::default();
    for stored in ["shelf", "status"] {
        derived.stored.insert(inspect_model::derived_key("ci", "Doc", stored));
    }
    derived.states.extend(["active".to_owned(), "archived".to_owned()]);
    let docs = Expr::With {
        span: NOWHERE,
        source: Box::new(ident("Docs")),
        predicate: Box::new(compare(ident("shelf"), "Eq", ident("this"))),
    };
    let live = filtered(ident("docs"), compare(ident("status"), "Eq", ident("active")));
    let has_live = compare(field(ident("live"), "count"), "Gt", number("0"));
    for (name, definition) in [("docs", docs), ("live", live), ("has_live", has_live)] {
        derived.fields.insert(inspect_model::derived_key("ci", "Shelf", name), definition);
    }
    let env = env(&world, "ci").deriving(&derived).bind("s", Value::Ref(shelf));
    let node = field(ident("s"), "has_live");
    assert_eq!(truth_of(&node, &env), Truth::Unknown);
    let said = reasons(&node, &env);
    assert!(
        said.iter().any(|reason| reason == &format!("`{doc}` has no `status` set")),
        "{said:?}"
    );
}
