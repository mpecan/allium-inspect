//! Applying postconditions: what a rule actually changes.
//!
//! Driven directly rather than through a whole step, because the cases worth
//! covering are the ones a well-behaved spec does not reach — a transition the
//! lifecycle forbids, a conditional whose condition cannot be decided, an
//! iteration over something that is not a collection. Those are where a
//! simulator either reports honestly or quietly invents behaviour.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

/// The definitions an `Application` is given when the case is not about them.
///
/// Every one of these is about applying a postcondition, and a postcondition
/// that read a computed field would be a different test.
static NOTHING: std::sync::LazyLock<std::collections::BTreeMap<String, allium_parser::ast::Expr>> =
    std::sync::LazyLock::new(std::collections::BTreeMap::new);

use std::collections::BTreeMap;

use allium_parser::{
    Span,
    ast::{ComparisonOp, Expr},
};
use inspect_model::{
    Node, NodeDetail, NodeKind, SpecGraph,
    graph::{EntityDetail, EntityField, EntityKind, EnumDetail, TransitionEdge, TransitionGraph},
};
use inspect_sim::{
    Effect, Value,
    apply::{Against, Application},
    value::EntityId,
    world::World,
};

mod common;
use common::*;

// --- the spec -------------------------------------------------------------

/// `Copy` with a lifecycle, and `Loan` with a status but no lifecycle.
fn spec() -> SpecGraph {
    let mut graph = SpecGraph::new("test");

    let mut status = EntityField::new("status", "available | on_loan | lost");
    status.enum_values = ["available", "on_loan", "lost"].map(ToOwned::to_owned).to_vec();
    graph.nodes.push(Node::new("catalogue", NodeKind::Entity, "Copy").with(NodeDetail::Entity(
        EntityDetail {
            kind: EntityKind::Internal,
            fields: vec![EntityField::new("shelfmark", "String"), status],
            transitions: vec![TransitionGraph {
                field: "status".to_owned(),
                states: ["available", "on_loan", "lost"].map(ToOwned::to_owned).to_vec(),
                edges: vec![
                    TransitionEdge { from: "available".to_owned(), to: "on_loan".to_owned() },
                    TransitionEdge { from: "on_loan".to_owned(), to: "available".to_owned() },
                    TransitionEdge { from: "on_loan".to_owned(), to: "lost".to_owned() },
                ],
                terminal: vec!["lost".to_owned()],
            }],
            parent: None,
        },
    )));

    let mut loan_status = EntityField::new("status", "open | returned");
    loan_status.enum_values = ["open", "returned"].map(ToOwned::to_owned).to_vec();
    graph.nodes.push(Node::new("lending", NodeKind::Entity, "Loan").with(NodeDetail::Entity(
        EntityDetail {
            kind: EntityKind::Internal,
            fields: vec![
                EntityField::new("copy", "catalogue/Copy"),
                loan_status,
                // Typed by an enumeration declared elsewhere, which carries
                // its states instead of the field carrying them. Qualified,
                // because an enumeration is filed under its bare name in the
                // module that declares it and the two have to meet.
                EntityField::new("wanted_as", "catalogue/Medium"),
            ],
            transitions: Vec::new(),
            parent: None,
        },
    )));

    // Two of them, and `Binding` first, because a spec has more than one
    // enumeration and the field names which. Finding *an* enumeration rather
    // than *the* one would let a state of any of them land in any field.
    graph.nodes.push(Node::new("catalogue", NodeKind::Enum, "Binding").with(NodeDetail::Enum(
        EnumDetail { values: ["hardback", "paperback"].map(ToOwned::to_owned).to_vec() },
    )));
    graph.nodes.push(Node::new("catalogue", NodeKind::Enum, "Medium").with(NodeDetail::Enum(
        EnumDetail { values: ["print", "audio"].map(ToOwned::to_owned).to_vec() },
    )));

    graph.normalise();
    graph
}

/// A world holding one available copy.
fn world() -> (World, EntityId) {
    let mut world = World::new();
    let copy = world.create("Copy", "catalogue");
    world.set_field(&copy, "shelfmark", Value::Str("QA76".to_owned()));
    world.set_field(&copy, "status", Value::Enum("available".to_owned()));
    (world, copy)
}

/// Apply `clause` with `bindings` in scope, returning the effects and world.
fn apply(clause: &Expr, bindings: &[(&str, Value)]) -> (Vec<Effect>, Vec<String>, World) {
    let graph = spec();
    let (mut world, _) = world();
    let scope: BTreeMap<String, Value> =
        bindings.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect();

    let applied = {
        let mut application = Application::new(
            Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
            &mut world,
            scope,
        );
        application.apply(clause)
    };
    let reasons = applied.unresolved.into_iter().map(|note| note.reason).collect();
    (applied.effects, reasons, world)
}

// --- building clauses -----------------------------------------------------

#[test]
fn a_creation_makes_an_instance_and_sets_its_fields() {
    let copy = EntityId::new("Copy", 1);
    let (effects, _, world) = apply(
        &creation("Loan", vec![("copy", ident("copy"))]),
        &[("copy", Value::Ref(copy.clone()))],
    );

    assert_eq!(effects.len(), 1);
    let Effect::Created { id, entity } = &effects[0] else { panic!("expected a creation") };
    assert_eq!(entity, "Loan");

    let instance = world.instance(id).expect("the loan exists");
    assert_eq!(instance.field("copy"), Value::Ref(copy));
    assert_eq!(instance.module, "lending", "the module that declares it, not the rule's");
}

#[test]
fn a_bare_state_in_a_creation_is_read_from_the_entitys_declared_states() {
    // At creation there is no previous value to say `open` is a state, so the
    // spec's own declaration settles it. Without this the instance starts with
    // an undecided status and every later rule reading it is undecided too.
    let (_, _, world) = apply(&creation("Loan", vec![("status", ident("open"))]), &[]);
    let instance = world.instance(&EntityId::new("Loan", 1)).expect("the loan");
    assert_eq!(instance.field("status"), Value::Enum("open".to_owned()));
}

#[test]
fn a_misspelled_state_stays_undecided_rather_than_becoming_one() {
    // Accepting any bare name would invent a state the lifecycle never mentions.
    let (_, _, world) = apply(&creation("Loan", vec![("status", ident("opne"))]), &[]);
    let instance = world.instance(&EntityId::new("Loan", 1)).expect("the loan");
    assert_eq!(instance.field("status"), Value::Unknown);
}

#[test]
fn a_creation_binds_its_own_name_for_the_clause_after_it() {
    // `ensures: CopyBorrowed(loan: loan)` on the next line reads this binding.
    let graph = spec();
    let (mut world, _) = world();
    let mut application = Application::new(
        Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
        &mut world,
        BTreeMap::new(),
    );
    application.apply(&creation("Loan", vec![]));
    let bindings = application.into_bindings();
    assert_eq!(bindings.get("loan"), Some(&Value::Ref(EntityId::new("Loan", 1))));
}

#[test]
fn a_creation_argument_that_cannot_be_evaluated_is_reported() {
    let (_, reasons, world) = apply(&creation("Loan", vec![("copy", ident("nobody"))]), &[]);
    assert!(reasons.iter().any(|reason| reason.contains("nobody")));
    let instance = world.instance(&EntityId::new("Loan", 1)).expect("the loan is still created");
    assert_eq!(instance.field("copy"), Value::Unknown);
}

// --- assignment -----------------------------------------------------------

#[test]
fn an_assignment_the_lifecycle_permits_is_written_and_reported() {
    let copy = EntityId::new("Copy", 1);
    let (effects, _, world) = apply(
        &assign(field(ident("copy"), "status"), ident("on_loan")),
        &[("copy", Value::Ref(copy.clone()))],
    );

    let Effect::Assigned { from, to, .. } = &effects[0] else { panic!("expected an assignment") };
    assert_eq!(from, &Value::Enum("available".to_owned()), "the trace says what changed");
    assert_eq!(to, &Value::Enum("on_loan".to_owned()));
    assert_eq!(
        world.instance(&copy).expect("the copy").field("status"),
        Value::Enum("on_loan".to_owned())
    );
}

#[test]
fn an_assignment_the_lifecycle_forbids_is_refused_rather_than_written() {
    // `available -> lost` is not an edge in the graph. Writing it anyway would
    // demonstrate behaviour the specification forbids.
    let copy = EntityId::new("Copy", 1);
    let (effects, _, world) = apply(
        &assign(field(ident("copy"), "status"), ident("lost")),
        &[("copy", Value::Ref(copy.clone()))],
    );

    let Effect::Refused { from, to, reason, .. } = &effects[0] else {
        panic!("expected a refusal, got {effects:?}")
    };
    assert_eq!(from, "available");
    assert_eq!(to, "lost");
    assert!(reason.contains("on_loan"), "and says what is allowed instead: {reason}");
    assert_eq!(
        world.instance(&copy).expect("the copy").field("status"),
        Value::Enum("available".to_owned()),
        "and nothing was written"
    );
}

#[test]
fn a_refusal_out_of_a_terminal_state_says_it_is_terminal() {
    let graph = spec();
    let (mut world, copy) = world();
    world.set_field(&copy, "status", Value::Enum("lost".to_owned()));

    let scope = BTreeMap::from([("copy".to_owned(), Value::Ref(copy))]);
    let applied = {
        let mut application = Application::new(
            Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
            &mut world,
            scope,
        );
        application.apply(&assign(field(ident("copy"), "status"), ident("available")))
    };
    let Effect::Refused { reason, .. } = &applied.effects[0] else { panic!("expected a refusal") };
    assert!(reason.contains("terminal"), "{reason}");
}

#[test]
fn assigning_a_state_to_itself_is_not_a_forbidden_transition() {
    // The lifecycle has no self-edge and does not need one; re-stating the
    // state a thing is already in changes nothing and forbids nothing.
    let copy = EntityId::new("Copy", 1);
    let (effects, _, _) = apply(
        &assign(field(ident("copy"), "status"), ident("available")),
        &[("copy", Value::Ref(copy))],
    );
    assert!(matches!(effects[0], Effect::Assigned { .. }), "{effects:?}");
}

#[test]
fn a_field_with_no_lifecycle_takes_any_value() {
    // `Loan.status` has states but no declared transitions, so nothing
    // constrains the move.
    let graph = spec();
    let (mut world, _) = world();
    let loan = world.create("Loan", "lending");
    world.set_field(&loan, "status", Value::Enum("open".to_owned()));

    let scope = BTreeMap::from([("loan".to_owned(), Value::Ref(loan.clone()))]);
    let applied = {
        let mut application = Application::new(
            Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
            &mut world,
            scope,
        );
        application.apply(&assign(field(ident("loan"), "status"), ident("returned")))
    };
    assert!(matches!(applied.effects[0], Effect::Assigned { .. }));
    assert_eq!(
        world.instance(&loan).expect("the loan").field("status"),
        Value::Enum("returned".to_owned())
    );
}

#[test]
fn a_comparison_that_is_not_an_assignment_is_noted_rather_than_acted_on() {
    // `ensures: a != b` asserts something about the end state; it is not an
    // instruction, and acting on it would mean inventing a value.
    let clause = compare(field(ident("copy"), "status"), ComparisonOp::NotEq, ident("lost"));
    let (effects, _, _) = apply(&clause, &[("copy", Value::Ref(EntityId::new("Copy", 1)))]);
    assert!(matches!(effects[0], Effect::Noted { .. }), "{effects:?}");
}

#[test]
fn assigning_through_something_that_is_not_an_instance_changes_nothing() {
    let (effects, reasons, world) =
        apply(&assign(field(ident("nobody"), "status"), ident("lost")), &[]);
    assert!(effects.is_empty());
    assert!(!reasons.is_empty(), "and says why");
    assert_eq!(
        world.instance(&EntityId::new("Copy", 1)).expect("the copy").field("status"),
        Value::Enum("available".to_owned())
    );
}

// --- emission, blocks, conditionals, iteration ----------------------------

#[test]
fn an_emission_is_reported_without_changing_anything() {
    let (effects, _, world) = apply(&emission("CopyBorrowed"), &[]);
    let Effect::Emitted { trigger, module } = &effects[0] else { panic!("expected an emission") };
    assert_eq!(trigger, "CopyBorrowed");
    assert_eq!(module, "lending");
    assert_eq!(world.entities.len(), 1, "the world is untouched");
}

#[test]
fn a_block_applies_each_of_its_statements_in_order() {
    let clause = block(vec![creation("Loan", vec![]), emission("CopyBorrowed")]);
    let (effects, _, _) = apply(&clause, &[]);
    assert!(matches!(effects[0], Effect::Created { .. }));
    assert!(matches!(effects[1], Effect::Emitted { .. }));
}

#[test]
fn a_conditional_applies_its_branch_when_the_condition_holds() {
    let clause = conditional(boolean(true), creation("Loan", vec![]));
    let (effects, _, _) = apply(&clause, &[]);
    assert!(matches!(effects[0], Effect::Created { .. }), "{effects:?}");
}

#[test]
fn a_conditional_whose_condition_is_false_applies_nothing() {
    let clause = conditional(boolean(false), creation("Loan", vec![]));
    let (effects, _, world) = apply(&clause, &[]);
    assert!(effects.is_empty());
    assert_eq!(world.count_of("Loan"), 0);
}

#[test]
fn a_conditional_the_simulator_cannot_decide_is_skipped_and_said_so() {
    // Neither taken nor silently skipped: applying it would be a coin toss with
    // side effects, and skipping it quietly would hide a branch that may matter.
    let clause = conditional(assign(ident("nobody"), number("1")), creation("Loan", vec![]));
    let (effects, reasons, world) = apply(&clause, &[]);
    assert_eq!(world.count_of("Loan"), 0, "the branch was not taken");
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Noted { description } if description.contains("skipped"))),
        "and the reader is told: {effects:?}"
    );
    assert!(!reasons.is_empty());
}

#[test]
fn an_iteration_applies_its_body_once_per_element() {
    let graph = spec();
    let (mut world, _) = world();
    world.create("Copy", "catalogue");
    world.create("Copy", "catalogue");

    let clause = iteration("c", ident("Copy"), emission("CopySeen"));
    let applied = {
        let mut application = Application::new(
            Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
            &mut world,
            BTreeMap::new(),
        );
        application.apply(&clause)
    };
    assert_eq!(applied.effects.len(), 3, "one per copy in the world");
}

#[test]
fn an_iteration_restores_the_binding_it_shadowed() {
    // A loop variable sharing a name with a rule argument must not leak past
    // the loop, or the clause after it reads the last element instead.
    let graph = spec();
    let (mut world, copy) = world();
    let scope = BTreeMap::from([("c".to_owned(), Value::Str("outer".to_owned()))]);

    let clause = iteration("c", ident("Copy"), emission("Seen"));
    let bindings = {
        let mut application = Application::new(
            Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
            &mut world,
            scope,
        );
        application.apply(&clause);
        application.into_bindings()
    };
    assert_eq!(bindings.get("c"), Some(&Value::Str("outer".to_owned())));
    let _ = copy;
}

#[test]
fn iterating_over_something_that_is_not_a_collection_applies_nothing() {
    let clause = iteration("x", ident("nobody"), emission("Seen"));
    let (effects, reasons, _) = apply(&clause, &[]);
    assert!(effects.is_empty());
    assert!(!reasons.is_empty());
}

#[test]
fn a_removal_assertion_is_noted_rather_than_performed() {
    // Removal in Allium asserts something about the end state; guessing which
    // instance was meant would invent one.
    let clause = not_exists(ident("Copy"));
    let (effects, _, world) = apply(&clause, &[]);
    assert!(matches!(effects[0], Effect::Noted { .. }));
    assert_eq!(world.count_of("Copy"), 1, "nothing was removed");
}

#[test]
fn a_clause_shape_this_module_does_not_model_changes_nothing_and_says_so() {
    // Two halves, and the second used to be missing. The world must not move —
    // applying a form nobody has modelled would be a guess with side effects —
    // but the rule is still reported as Fired, so a reader comparing the spec
    // against the world needs to be told which promise the world does not
    // carry. Silence there reads as "the rule did everything it said".
    let (effects, _, world) = apply(&unmodelled(), &[]);
    assert_eq!(world.entities.len(), 1, "nothing was created or changed");
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Noted { .. })),
        "the postcondition it could not apply is recorded: {effects:?}"
    );
}

#[test]
fn applying_is_deterministic() {
    let clause = creation("Loan", vec![("status", ident("open"))]);
    let (first, _, one) = apply(&clause, &[]);
    let (second, _, two) = apply(&clause, &[]);
    assert_eq!(first, second);
    assert_eq!(one, two);
}

// --- states, and the two places a field's are written ----------------------

/// `status: open | returned` carries its own states.
#[test]
fn a_state_a_field_declares_itself_is_stored_as_one() {
    let (_, _, world) = apply(&creation("Loan", vec![("status", ident("open"))]), &[]);
    let loan = world.instance(&EntityId::new("Loan", 1)).expect("created");
    assert_eq!(loan.fields.get("status"), Some(&Value::Enum("open".to_owned())));
}

/// `wanted_as: catalogue/Medium` carries none, and names an enumeration that
/// does. Reading only the first place left `Receipt.created(kind: read)`
/// storing an unknown — so a rule guarding on `not exists Receipt{…, kind:
/// read}` could not tell whether it had already recorded one.
#[test]
fn a_state_the_enumeration_a_field_names_declares_is_stored_as_one() {
    let (_, _, world) = apply(&creation("Loan", vec![("wanted_as", ident("print"))]), &[]);
    let loan = world.instance(&EntityId::new("Loan", 1)).expect("created");
    assert_eq!(loan.fields.get("wanted_as"), Some(&Value::Enum("print".to_owned())));
}

/// Checked rather than accepted on sight, in both places. A misspelling that
/// became a state would be a state nothing in the lifecycle mentions, and the
/// panel would draw a world the spec cannot reach.
#[test]
fn a_state_neither_place_declares_stays_unknown_and_says_why() {
    for (field, wrong) in [("status", "returnd"), ("wanted_as", "papyrus")] {
        let (_, reasons, world) = apply(&creation("Loan", vec![(field, ident(wrong))]), &[]);
        let loan = world.instance(&EntityId::new("Loan", 1)).expect("created");
        assert_eq!(loan.fields.get(field), Some(&Value::Unknown), "{field} took `{wrong}`");
        assert!(
            reasons.iter().any(|why| why.contains(wrong)),
            "no reason naming `{wrong}`: {reasons:?}"
        );
    }
}

/// And the enumeration has to be the one the field names. A spec has several,
/// and taking the first one found would let a state of any of them land in a
/// field typed by another — `wanted_as: hardback` on a field of `Medium`.
#[test]
fn a_state_of_some_other_enumeration_is_not_this_fields() {
    let (_, _, world) = apply(&creation("Loan", vec![("wanted_as", ident("hardback"))]), &[]);
    let loan = world.instance(&EntityId::new("Loan", 1)).expect("created");
    assert_eq!(loan.fields.get("wanted_as"), Some(&Value::Unknown));

    // Nor does a field typed by something that is not an enumeration at all
    // take one's states.
    let (_, _, world) = apply(&creation("Loan", vec![("copy", ident("print"))]), &[]);
    let loan = world.instance(&EntityId::new("Loan", 1)).expect("created");
    assert_eq!(loan.fields.get("copy"), Some(&Value::Unknown));
}

// --- where a created instance comes from ----------------------------------

#[test]
fn a_created_instance_belongs_to_the_module_that_declares_its_type() {
    // `BorrowCopy` lives in `lending` and creates a `Copy`, which `catalogue`
    // declares. Recording it under `lending` would put it in the wrong module
    // everywhere afterwards: the source strip, the inspector, the world panel.
    let (effects, _, world) = apply(&creation("Copy", vec![]), &[]);
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    let instance = world.instance(id).expect("the created instance is in the world");
    assert_eq!(instance.module, "catalogue");
}

#[test]
fn an_instance_of_a_type_the_spec_does_not_declare_falls_back_to_the_rule() {
    // Nothing better is knowable. The point is that it is the rule's own module
    // rather than a guess or a blank.
    let (effects, _, world) = apply(&creation("Postcard", vec![]), &[]);
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("it was created").module, "lending");
}

// --- quoting the clause the author wrote ----------------------------------

/// Apply `clause` against `source`, so spans have text to slice.
fn apply_over(clause: &Expr, source: &str) -> Vec<Effect> {
    let graph = spec();
    let (mut world, _) = world();
    let mut application = Application::new(
        Against { spec: &graph, module: "lending", source, derived: &NOTHING },
        &mut world,
        BTreeMap::new(),
    );
    application.apply(clause).effects
}

#[test]
fn a_noted_clause_is_quoted_from_the_source_on_one_line() {
    // The panel shows this to a person who is looking at the same file. A
    // paraphrase, or the placeholder, would send them hunting for text that is
    // not there — and the spec wraps clauses across lines, so it is one line
    // here and the author's words either way.
    let source = "ensures not exists\n    Loan where loan.copy = copy\n";
    let spanned = not_exists_at(ident("Loan"), Span { start: 8, end: 51 });
    let effects = apply_over(&spanned, source);
    assert_eq!(
        effects,
        vec![Effect::Noted { description: "not exists Loan where loan.copy = copy".to_owned() }]
    );
}

#[test]
fn a_clause_with_no_source_behind_it_says_what_kind_of_clause_it_was() {
    // A span that points outside the text is not a reason to show nothing.
    let effects = apply_over(&not_exists_at(ident("Loan"), Span { start: 900, end: 999 }), "short");
    assert_eq!(
        effects,
        vec![Effect::Noted { description: "an assertion about what exists".to_owned() }]
    );
}

// --- `let` ---------------------------------------------------------------

// A rule that creates two things and joins them writes them with a `let`,
// because that is the only way to write it:
//
//     ensures:
//         let group = Group.created(name: name)
//         Membership.created(group: group, member: founder)
//
// Without this the whole postcondition was silently not applied: the rule
// fired, reported success, and left an empty world.

fn let_binding(name: &str, value: Expr) -> Expr {
    Expr::LetExpr {
        span: NOWHERE,
        name: allium_parser::ast::Ident { span: NOWHERE, name: name.to_owned() },
        value: Box::new(value),
    }
}

#[test]
fn a_let_applies_the_creation_it_binds() {
    let graph = spec();
    let mut world = World::new().at(1_000);
    let mut application = Application::new(
        Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
        &mut world,
        BTreeMap::new(),
    );

    let applied = application.apply(&let_binding("l", creation("Loan", vec![])));

    assert!(
        applied.effects.iter().any(|effect| matches!(effect, Effect::Created { .. })),
        "{applied:#?}"
    );
    assert_eq!(world.count_of("Loan"), 1, "the loan was not created");
}

/// The point of writing one: the next line refers to it by the author's name.
#[test]
fn a_let_binds_what_it_created_under_the_name_it_was_given() {
    let graph = spec();
    let mut world = World::new().at(1_000);
    let mut application = Application::new(
        Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
        &mut world,
        BTreeMap::new(),
    );

    application.apply(&let_binding("mine", creation("Loan", vec![])));
    let bindings = application.into_bindings();

    let bound = bindings.get("mine").expect("the name the rule gave it");
    assert!(matches!(bound, Value::Ref(id) if id.entity() == "Loan"), "{bound:?}");
}

/// The whole block, which is the shape a real rule has: create, bind, and
/// point the second creation at the first.
#[test]
fn a_second_creation_can_point_at_what_the_let_bound() {
    let graph = spec();
    let mut world = World::new().at(1_000);
    let mut application = Application::new(
        Against { spec: &graph, module: "lending", source: "", derived: &NOTHING },
        &mut world,
        BTreeMap::new(),
    );

    application.apply(&Expr::Block {
        span: NOWHERE,
        items: vec![
            let_binding("mine", creation("Loan", vec![])),
            creation("Reservation", vec![("member", ident("mine"))]),
        ],
    });

    let reservation =
        world.instances_of("Reservation").next().expect("the reservation was created").clone();
    let member = reservation.fields.get("member").expect("it points at something");
    assert!(
        matches!(member, Value::Ref(id) if id.entity() == "Loan"),
        "the `let` binding did not reach the second creation: {member:?}"
    );
}
