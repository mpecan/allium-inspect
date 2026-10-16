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
static NOTHING: std::sync::LazyLock<inspect_model::Derivations> =
    std::sync::LazyLock::new(inspect_model::Derivations::default);
/// No module's text: these postconditions read no definition another module wrote.
static NO_SOURCES: inspect_sim::step::Sources = inspect_sim::step::Sources::new();

use std::collections::BTreeMap;

use allium_parser::{
    Span,
    ast::{CallArg, ComparisonOp, Expr, Ident, LogicalOp, NamedArg},
};
use inspect_model::{
    Node, NodeDetail, NodeKind, SpecGraph,
    graph::{
        EntityDetail, EntityField, EntityKind, EnumDetail, TransitionEdge, TransitionGraph,
        TriggerDetail, TriggerSource,
    },
};
use inspect_sim::{
    Applied, Effect, Value,
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

    // The trigger an emission names, so a positional argument has a parameter
    // to land under. What a rule emits is what the next rule is fired with, and
    // the next rule reads its arguments by the names declared here.
    graph.nodes.push(Node::new("lending", NodeKind::Trigger, "CopyBorrowed").with(
        NodeDetail::Trigger(TriggerDetail {
            source: TriggerSource::External,
            parameters: vec!["loan".to_owned()],
            optional: Vec::new(),
            condition: None,
            entity: None,
        }),
    ));

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
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
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
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
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
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
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
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
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
    let Effect::Emitted { trigger, module, .. } = &effects[0] else {
        panic!("expected an emission")
    };
    assert_eq!(trigger, "CopyBorrowed");
    assert_eq!(module, "lending");
    assert_eq!(world.entities.len(), 1, "the world is untouched");
}

/// What an emission carries, because the rule waiting on it needs it.
///
/// `MessageSent(message: message)` hands the message on, and `QueueOnSend`
/// reads it to file the outbox entry. Recording the trigger's name alone left
/// every chained rule firing against nothing.
///
/// Positional, in order and unnamed: which name it goes by is each listener's
/// own `when` clause's to say (issue #12). Named here after the trigger's
/// first-listed listener, it reached that listener and no other.
#[test]
fn an_emission_carries_a_positional_argument_in_its_place_and_unnamed() {
    let loan = EntityId::new("Loan", 7);
    let (effects, _, _) =
        apply(&emission("CopyBorrowed"), &[("whatever", Value::Ref(loan.clone()))]);
    let Effect::Emitted { arguments, positional, .. } = &effects[0] else {
        panic!("expected an emission")
    };
    assert_eq!(positional, &[Value::Ref(loan)]);
    assert!(arguments.is_empty(), "named after nobody: {arguments:?}");
}

#[test]
fn a_named_argument_keeps_the_name_it_was_written_with() {
    let loan = EntityId::new("Loan", 7);
    let clause = Expr::Call {
        span: Span { start: 0, end: 0 },
        function: Box::new(ident("CopyBorrowed")),
        args: vec![CallArg::Named(NamedArg {
            span: Span { start: 0, end: 0 },
            name: Ident { span: Span { start: 0, end: 0 }, name: "loan".into() },
            value: ident("whatever"),
        })],
    };
    let (effects, _, _) = apply(&clause, &[("whatever", Value::Ref(loan.clone()))]);
    let Effect::Emitted { arguments, positional, .. } = &effects[0] else {
        panic!("expected an emission")
    };
    assert_eq!(arguments.get("loan"), Some(&Value::Ref(loan)));
    assert!(positional.is_empty(), "{positional:?}");
}

/// More positional arguments than any listener names are still carried, each
/// in its place: whether one has nowhere to go is the listener's to find out,
/// and a listener taking two is entitled to the second.
#[test]
fn every_positional_argument_is_carried_in_order() {
    let clause = Expr::Call {
        span: Span { start: 0, end: 0 },
        function: Box::new(ident("CopyBorrowed")),
        args: vec![CallArg::Positional(ident("first")), CallArg::Positional(ident("second"))],
    };
    let (effects, _, _) = apply(&clause, &[("first", Value::Int(1)), ("second", Value::Int(2))]);
    let Effect::Emitted { positional, .. } = &effects[0] else { panic!("expected an emission") };
    assert_eq!(positional, &[Value::Int(1), Value::Int(2)]);
}

/// `CopyBorrowed(word)`, one positional argument the clause names bare.
fn emitting(word: &str) -> Expr {
    Expr::Call {
        span: Span { start: 0, end: 0 },
        function: Box::new(ident("CopyBorrowed")),
        args: vec![CallArg::Positional(ident(word))],
    }
}

fn carried(word: &str, bindings: &[(&str, Value)]) -> Option<Value> {
    let (effects, _, _) = apply(&emitting(word), bindings);
    let Effect::Emitted { positional, .. } = &effects[0] else { panic!("expected an emission") };
    positional.first().cloned()
}

/// Issue #10. `Finish(repo, cancel)` passes a word nothing binds, and the rule
/// waiting on it was handed nothing — so its `how = cancel` was undecided
/// however plainly the spec had said which. A word the spec declares as a state
/// is that state.
#[test]
fn a_declared_state_an_emission_names_bare_is_carried_as_that_state() {
    assert_eq!(carried("audio", &[]), Some(Value::Enum("audio".to_owned())));
    // A field's inline states count as declared too.
    assert_eq!(carried("on_loan", &[]), Some(Value::Enum("on_loan".to_owned())));
}

/// A trigger's parameters carry no types, so nothing else says an undeclared
/// word is a state. Carried as unknown, and the rule reading it says so.
#[test]
fn an_undeclared_word_an_emission_names_is_carried_undecided() {
    assert_eq!(carried("abort", &[]), Some(Value::Unknown));
}

/// A bound name is its binding, even when the binding is unknown and its name
/// happens to be spelled like a state.
#[test]
fn a_bound_name_is_carried_as_its_binding_whatever_it_is_spelled() {
    assert_eq!(carried("audio", &[("audio", Value::Unknown)]), Some(Value::Unknown));
    assert_eq!(carried("audio", &[("audio", Value::Int(3))]), Some(Value::Int(3)));
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
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
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
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
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

/// A collection nothing settled is named as unsettled, quoted from the
/// source, not described as "an unknown" the reader cannot find in the file.
#[test]
fn an_iteration_over_an_unsettled_collection_quotes_it_as_never_settled() {
    let source = "for x in nobody";
    let collection = Expr::Ident(Ident { span: Span::new(9, 15), name: "nobody".to_owned() });
    let clause = iteration("x", collection, emission("Seen"));
    let graph = spec();
    let mut world = World::new();
    let mut application = Application::new(
        Against {
            spec: &graph,
            module: "lending",
            source,
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
        &mut world,
        BTreeMap::new(),
    );
    let applied = application.apply(&clause);
    assert_eq!(
        unapplied(&applied),
        vec![
            "`for x in nobody`: `nobody` was never settled, so the clauses inside it were not applied"
        ]
    );
}

/// An instance whose module is not where its entity is declared still has
/// that entity's fields found, by name — and only that entity's. Another
/// entity ahead of it in the graph, declaring nothing, is not it.
#[test]
fn a_member_from_another_module_is_still_read_against_its_own_entity() {
    let mut graph = spec();
    graph.nodes.insert(
        0,
        Node::new("circulation", NodeKind::Entity, "Shelf").with(NodeDetail::Entity(
            EntityDetail {
                kind: EntityKind::Internal,
                fields: Vec::new(),
                transitions: Vec::new(),
                parent: None,
            },
        )),
    );
    let mut world = World::new();
    let unset = world.create("Copy", "elsewhere");
    let bound = BTreeMap::from([("status".to_owned(), Value::Enum("available".to_owned()))]);
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let mut application = Application::new(
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
        &mut world,
        bound,
    );
    let applied = application.apply(&clause);
    assert!(emitted_for(&applied.effects).is_empty(), "{:?}", applied.effects);
    assert!(
        unapplied(&applied).iter().any(|reason| reason.contains(&unset.to_string())),
        "{:?}",
        unapplied(&applied)
    );
}

/// Apply `clause` against `world`, with nothing bound beforehand.
fn apply_in(world: &mut World, clause: &Expr) -> (Vec<Effect>, Vec<String>) {
    let applied = applied_in(world, BTreeMap::new(), clause);
    (applied.effects, applied.unresolved.into_iter().map(|note| note.reason).collect())
}

/// Apply `clause` against `world` with `bound` in scope, and keep everything.
fn applied_in(world: &mut World, bound: BTreeMap<String, Value>, clause: &Expr) -> Applied {
    let graph = spec();
    let mut application = Application::new(
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
        world,
        bound,
    );
    application.apply(clause)
}

/// What a clause left unapplied, by reason.
fn unapplied(applied: &Applied) -> Vec<&str> {
    applied.unapplied.iter().map(|note| note.reason.as_str()).collect()
}

/// A trigger argument named like a field the member has not set is not the
/// member's field. `when: Raised(status)` beside `for c in Copies where status
/// = available` asks about each copy's status; reading the argument instead
/// decided the filter about the wrong thing and applied the body to a copy
/// whose status nobody said.
#[test]
fn a_filter_does_not_read_a_rule_binding_in_place_of_a_field_the_member_has_not_set() {
    let mut world = World::new();
    let unset = world.create("Copy", "catalogue");
    let bound = BTreeMap::from([("status".to_owned(), Value::Enum("available".to_owned()))]);
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let applied = applied_in(&mut world, bound, &clause);
    assert!(emitted_for(&applied.effects).is_empty(), "{:?}", applied.effects);
    let undone = unapplied(&applied);
    assert!(
        undone.iter().any(|reason| reason.contains(&unset.to_string())),
        "the copy nobody gave a status is left unapplied, and named: {undone:?}"
    );
}

/// A field the member *has* set is still read bare, whatever the rule bound
/// under the same name.
#[test]
fn a_filter_reads_a_field_the_member_has_set_over_a_rule_binding_of_the_same_name() {
    let (mut world, available, _, _) = shelf();
    let bound = BTreeMap::from([("status".to_owned(), Value::Enum("lost".to_owned()))]);
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let applied = applied_in(&mut world, bound, &clause);
    assert_eq!(emitted_for(&applied.effects), vec![Value::Ref(available)]);
}

/// `let flag = c.shelfmark = "QA76"` inside a `for` is a value bound per
/// member, not an assignment: it writes nothing and leaves nothing unresolved.
/// A field with no lifecycle, so a write would not be refused on its way in.
#[test]
fn a_let_inside_an_iteration_binds_a_value_and_writes_nothing() {
    let mut world = World::new();
    let copy = world.create("Copy", "catalogue");
    world.set_field(&copy, "shelfmark", Value::Str("PR6".to_owned()));
    let body = block(vec![
        Expr::LetExpr {
            span: NOWHERE,
            name: name("flag"),
            value: Box::new(compare(
                field(ident("c"), "shelfmark"),
                ComparisonOp::Eq,
                text("QA76"),
            )),
        },
        seen_for("c"),
    ]);
    let clause = iteration("c", ident("Copies"), body);
    let applied = applied_in(&mut world, BTreeMap::new(), &clause);
    assert_eq!(
        world.instance(&copy).map(|it| it.field("shelfmark")),
        Some(Value::Str("PR6".to_owned())),
        "the let did not write"
    );
    assert!(
        !applied.effects.iter().any(|effect| matches!(effect, Effect::Assigned { .. })),
        "{:?}",
        applied.effects
    );
    assert_eq!(emitted_for(&applied.effects), vec![Value::Ref(copy)]);
    assert!(applied.unresolved.is_empty(), "{:?}", applied.unresolved);
}

/// What was applied in full is not marked unapplied, whatever notes it
/// carries: an `if` whose `or` its left side settled was taken.
#[test]
fn a_decided_conditional_carrying_notes_is_not_marked_unapplied() {
    let (mut world, available, _, _) = shelf();
    let clause = conditional(
        logical(
            compare(field(ident("it"), "status"), ComparisonOp::Eq, ident("available")),
            LogicalOp::Or,
            compare(field(ident("it"), "shelfmark"), ComparisonOp::Eq, text("x")),
        ),
        assign(field(ident("it"), "shelfmark"), text("QA76")),
    );
    let bound = BTreeMap::from([("it".to_owned(), Value::Ref(available))]);
    let applied = applied_in(&mut world, bound, &clause);
    assert!(applied.unapplied.is_empty(), "{:?}", applied.unapplied);
    assert!(
        applied.effects.iter().any(|effect| matches!(effect, Effect::Assigned { .. })),
        "{:?}",
        applied.effects
    );
}

/// Every path that leaves something undone says so in `unapplied`: a member
/// the filter could not decide about, a `for` over something that is not a
/// collection, a branch nobody could choose, an assignment to nothing, and a
/// form the simulator does not model.
#[test]
fn every_path_that_leaves_something_undone_marks_it_unapplied() {
    let (mut world, _, _, unset) = shelf();
    let undecided_member = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let applied = applied_in(&mut world, BTreeMap::new(), &undecided_member);
    assert!(
        unapplied(&applied).iter().any(|it| it.contains(&unset.to_string())),
        "{:?}",
        applied.unapplied
    );

    let cases = [
        ("not a collection", iteration("x", ident("nobody"), emission("Seen"))),
        (
            "a branch nobody could choose",
            conditional(
                compare(field(ident("nobody"), "status"), ComparisonOp::Eq, ident("lost")),
                emission("Seen"),
            ),
        ),
        ("an assignment to nothing", assign(field(ident("nobody"), "status"), ident("lost"))),
        ("unmodelled", unmodelled()),
    ];
    for (what, clause) in cases {
        let applied = applied_in(&mut world, BTreeMap::new(), &clause);
        assert!(!applied.unapplied.is_empty(), "{what}: {applied:?}");
        assert!(
            applied.unapplied.iter().all(|note| applied.unresolved.contains(note)),
            "{what}: what is unapplied is among the notes too: {applied:?}"
        );
    }
}

/// A world of three copies: one available, one lost, and one whose status
/// nobody set.
fn shelf() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    let available = world.create("Copy", "catalogue");
    world.set_field(&available, "status", Value::Enum("available".to_owned()));
    let lost = world.create("Copy", "catalogue");
    world.set_field(&lost, "status", Value::Enum("lost".to_owned()));
    let unset = world.create("Copy", "catalogue");
    (world, available, lost, unset)
}

fn emitted_for(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Emitted { arguments, .. } => arguments.get("copy").cloned(),
            _ => None,
        })
        .collect()
}

/// `Seen(copy: c)`, so each emission says which element it was for.
fn seen_for(binding: &str) -> Expr {
    Expr::Call {
        span: Span::new(0, 0),
        function: Box::new(ident("Seen")),
        args: vec![CallArg::Named(NamedArg {
            span: Span::new(0, 0),
            name: Ident { span: Span::new(0, 0), name: "copy".to_owned() },
            value: ident(binding),
        })],
    }
}

/// Issue #6: a `where` on an iteration was dropped, so every member was
/// iterated whatever the filter said.
#[test]
fn an_iteration_applies_its_body_only_to_the_members_its_filter_keeps() {
    let mut world = World::new();
    let available = world.create("Copy", "catalogue");
    world.set_field(&available, "status", Value::Enum("available".to_owned()));
    let lost = world.create("Copy", "catalogue");
    world.set_field(&lost, "status", Value::Enum("lost".to_owned()));

    // `for c in Copies where status = available` — the element's field bare.
    let bare = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let (effects, reasons) = apply_in(&mut world, &bare);
    assert_eq!(emitted_for(&effects), vec![Value::Ref(available.clone())], "{effects:?}");
    assert!(reasons.is_empty(), "{reasons:?}");

    // `for c in Copies where c.status = available` — through the loop's name.
    let named = filtered_iteration(
        "c",
        ident("Copies"),
        compare(field(ident("c"), "status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let (effects, reasons) = apply_in(&mut world, &named);
    assert_eq!(emitted_for(&effects), vec![Value::Ref(available)], "{effects:?}");
    assert!(reasons.is_empty(), "{reasons:?}");
}

/// A member the filter cannot decide about is neither iterated nor quietly
/// left out: it is skipped, and the reason names the iteration.
#[test]
fn a_member_the_filter_cannot_decide_about_is_skipped_and_said_so() {
    let (mut world, available, _, _) = shelf();
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        compare(ident("status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let (effects, reasons) = apply_in(&mut world, &clause);
    assert_eq!(emitted_for(&effects), vec![Value::Ref(available)], "{effects:?}");
    assert!(!reasons.is_empty(), "the unset copy is reported: {effects:?}");
    assert!(
        reasons.iter().any(|reason| reason.contains("Copy#3") && reason.contains("`for c in`")),
        "names the member and the iteration: {reasons:?}"
    );
}

/// A filter that decided — `or` settled by its left side — keeps or drops the
/// member and leaves nothing unresolved behind it, even though the right side
/// could not be read. A note there would read as a member left unapplied.
#[test]
fn a_filter_that_decided_leaves_nothing_unresolved_whatever_it_could_not_read() {
    let mut world = World::new();
    let available = world.create("Copy", "catalogue");
    world.set_field(&available, "status", Value::Enum("available".to_owned()));
    let lost = world.create("Copy", "catalogue");
    world.set_field(&lost, "status", Value::Enum("lost".to_owned()));
    world.set_field(&lost, "shelfmark", Value::Str("QA76".to_owned()));

    // available: `status = available` is true, the `shelfmark` nobody set does
    // not matter. lost: `status = available` false, `shelfmark = "x"` false.
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        logical(
            compare(ident("status"), ComparisonOp::Eq, ident("available")),
            LogicalOp::Or,
            compare(field(ident("c"), "shelfmark"), ComparisonOp::Eq, text("x")),
        ),
        seen_for("c"),
    );
    let (effects, reasons) = apply_in(&mut world, &clause);
    assert_eq!(emitted_for(&effects), vec![Value::Ref(available)], "{effects:?}");
    assert!(reasons.is_empty(), "{reasons:?}");
}

/// Issue #6: a collection with no members is iterated zero times, and that is
/// a decided answer — nothing left unresolved behind it.
#[test]
fn an_iteration_over_a_type_with_no_instances_applies_nothing_and_is_decided() {
    let mut world = World::new();
    let clause = filtered_iteration(
        "w",
        ident("Hooks"),
        compare(ident("active"), ComparisonOp::Eq, boolean(true)),
        seen_for("w"),
    );
    let (effects, reasons) = apply_in(&mut world, &clause);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(reasons.is_empty(), "{reasons:?}");
}

/// The filter sees the loop's name, but the binding still does not leak past
/// the loop.
#[test]
fn a_filtered_iteration_restores_the_binding_it_shadowed() {
    let graph = spec();
    let (mut world, _, _, _) = shelf();
    let scope = BTreeMap::from([("c".to_owned(), Value::Str("outer".to_owned()))]);
    let clause = filtered_iteration(
        "c",
        ident("Copies"),
        compare(field(ident("c"), "status"), ComparisonOp::Eq, ident("available")),
        seen_for("c"),
    );
    let bindings = {
        let mut application = Application::new(
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
            &mut world,
            scope,
        );
        application.apply(&clause);
        application.into_bindings()
    };
    assert_eq!(bindings.get("c"), Some(&Value::Str("outer".to_owned())));
}

/// `not exists presence` names one instance, and afterwards it is gone —
/// which is what the language says `not exists` means as an outcome.
#[test]
fn a_removal_of_one_instance_removes_it() {
    let copy = EntityId::new("Copy", 1);
    let (effects, reasons, world) =
        apply(&not_exists(ident("copy")), &[("copy", Value::Ref(copy.clone()))]);
    assert_eq!(effects, vec![Effect::Removed { id: copy.clone(), entity: "Copy".to_owned() }]);
    assert!(reasons.is_empty());
    assert!(world.instance(&copy).is_none());
}

/// One nothing settled is skipped with its reason, not removed and not kept
/// quietly: which instance it was is exactly what is not known.
#[test]
fn a_removal_of_something_undecided_is_skipped_and_says_why() {
    let (effects, reasons, world) = apply(&not_exists(ident("nobody")), &[]);
    assert_eq!(world.count_of("Copy"), 1, "nothing was removed");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Noted { .. })), "{effects:?}");
    assert!(reasons.iter().any(|reason| reason.contains("nobody")), "{reasons:?}");
}

/// Already gone, or never there, is the end state the clause asks for.
#[test]
fn a_removal_of_nothing_changes_nothing() {
    let (effects, _, world) = apply(&not_exists(ident("gone")), &[("gone", Value::Null)]);
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(world.count_of("Copy"), 1);
}

#[test]
fn a_removal_assertion_about_a_collection_is_noted_rather_than_performed() {
    // `not exists Copy` names a type where an instance was meant; removing
    // every copy on the strength of it would invent which ones were meant.
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

/// An instance filed in a module that does not declare its entity — a world
/// written by hand, or one placed before its module was known — still has its
/// states read from the entity of that name. Looking only in the instance's
/// own module would leave every bare state assigned to it undecided.
#[test]
fn a_state_is_read_by_name_when_the_instances_module_does_not_declare_it() {
    let graph = spec();
    let (mut world, _) = world();
    let loan = world.create("Loan", "elsewhere");
    world.set_field(&loan, "status", Value::Enum("open".to_owned()));

    let scope = BTreeMap::from([("loan".to_owned(), Value::Ref(loan.clone()))]);
    let applied = {
        let mut application = Application::new(
            Against {
                spec: &graph,
                module: "lending",
                source: "",
                sources: &NO_SOURCES,
                derived: &NOTHING,
            },
            &mut world,
            scope,
        );
        application.apply(&assign(field(ident("loan"), "status"), ident("returned")))
    };
    assert!(applied.unresolved.is_empty(), "{:?}", applied.unresolved);
    assert_eq!(
        world.instance(&loan).expect("the loan").field("status"),
        Value::Enum("returned".to_owned())
    );
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

// --- a creation through a qualified name (issue #9) ------------------------

/// `alias/Entity.created(field: value, …)`.
fn qualified_creation(alias: &str, entity: &str, fields: Vec<(&str, Expr)>) -> Expr {
    let Expr::Call { span, args, .. } = creation(entity, fields) else { unreachable!() };
    let qualified = Expr::QualifiedName(allium_parser::ast::QualifiedName {
        span: NOWHERE,
        qualifier: Some(alias.to_owned()),
        name: entity.to_owned(),
    });
    Expr::Call { span, function: Box::new(field(qualified, "created")), args }
}

/// `lending` imports `catalogue` as `cat`, and a second module, `archive`,
/// declares a `Copy` of its own ahead of it in the graph — so finding the
/// module by the entity's name alone would pick the wrong one.
fn spec_with_alias() -> SpecGraph {
    let mut graph = spec();
    graph.nodes.insert(
        0,
        Node::new("archive", NodeKind::Entity, "Copy").with(NodeDetail::Entity(EntityDetail {
            kind: EntityKind::Internal,
            fields: Vec::new(),
            transitions: Vec::new(),
            parent: None,
        })),
    );
    graph.modules.push(inspect_model::graph::Module {
        name: "lending".to_owned(),
        path: "lending.allium".to_owned(),
        imports: vec![inspect_model::graph::Import {
            alias: "cat".to_owned(),
            path: "./catalogue.allium".to_owned(),
            target: Some("catalogue".to_owned()),
            span: None,
        }],
        language_version: Some(3),
    });
    graph
}

fn apply_qualified(graph: &SpecGraph, clause: &Expr) -> (Vec<Effect>, World) {
    let (mut world, _) = world();
    let mut application = Application::new(
        Against {
            spec: graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
        &mut world,
        BTreeMap::new(),
    );
    let effects = application.apply(clause).effects;
    (effects, world)
}

/// What applying `clause` in `lending` left unresolved.
fn unresolved_of(graph: &SpecGraph, clause: &Expr) -> Vec<String> {
    let (mut world, _) = world();
    let mut application = Application::new(
        Against {
            spec: graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
        &mut world,
        BTreeMap::new(),
    );
    application.apply(clause).unresolved.into_iter().map(|note| note.reason).collect()
}

#[test]
fn a_qualified_creation_is_a_creation() {
    // It was read as neither a creation nor a trigger: the rule fired and the
    // world held nothing new.
    let (effects, world) = apply_qualified(
        &spec(),
        &qualified_creation("cat", "Copy", vec![("shelfmark", text("QA76"))]),
    );
    let Some(Effect::Created { id, entity }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(entity, "Copy");
    let copy = world.instance(id).expect("created");
    assert_eq!(copy.field("shelfmark"), Value::Str("QA76".to_owned()));
}

#[test]
fn a_qualified_creation_lands_in_the_module_its_alias_names() {
    let (effects, world) =
        apply_qualified(&spec_with_alias(), &qualified_creation("cat", "Copy", vec![]));
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("created").module, "catalogue");
}

/// The alias is the creating module's: another module that imports something
/// else `as cat` says nothing about what `cat` means in `lending`.
#[test]
fn the_alias_is_read_from_the_imports_of_the_module_creating() {
    let mut graph = spec_with_alias();
    graph.modules.insert(
        0,
        inspect_model::graph::Module {
            name: "archive".to_owned(),
            path: "archive.allium".to_owned(),
            imports: vec![inspect_model::graph::Import {
                alias: "cat".to_owned(),
                path: "./archive.allium".to_owned(),
                target: Some("archive".to_owned()),
                span: None,
            }],
            language_version: Some(3),
        },
    );
    let (effects, world) = apply_qualified(&graph, &qualified_creation("cat", "Copy", vec![]));
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("created").module, "catalogue");
}

#[test]
fn an_alias_that_resolves_leaves_nothing_unresolved() {
    let notes = unresolved_of(&spec_with_alias(), &qualified_creation("cat", "Copy", vec![]));
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn an_unqualified_creation_is_still_placed_by_name() {
    // The alias is consulted only when one was written.
    let (effects, world) = apply_qualified(&spec_with_alias(), &creation("Copy", vec![]));
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("created").module, "archive");
}

#[test]
fn an_alias_this_module_does_not_import_falls_back_to_the_name() {
    let (effects, world) =
        apply_qualified(&spec_with_alias(), &qualified_creation("nowhere", "Copy", vec![]));
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("created").module, "archive");
    // Placed, and said to be a guess: the spec gave no home for it.
    let notes = unresolved_of(&spec_with_alias(), &qualified_creation("nowhere", "Copy", vec![]));
    assert!(notes.iter().any(|note| note.contains("`nowhere/Copy`")), "{notes:?}");
}

#[test]
fn an_alias_whose_module_does_not_declare_the_entity_falls_back_to_the_name() {
    // `cat/Loan`: `catalogue` declares no `Loan`, so the alias is no help and
    // the entity is placed where it is declared rather than nowhere.
    let (effects, world) =
        apply_qualified(&spec_with_alias(), &qualified_creation("cat", "Loan", vec![]));
    let Some(Effect::Created { id, .. }) = effects.first() else {
        panic!("expected a creation, got {effects:?}");
    };
    assert_eq!(world.instance(id).expect("created").module, "lending");
    let notes = unresolved_of(&spec_with_alias(), &qualified_creation("cat", "Loan", vec![]));
    assert!(notes.iter().any(|note| note.contains("`cat/Loan`")), "{notes:?}");
}

// --- quoting the clause the author wrote ----------------------------------

/// Apply `clause` against `source`, so spans have text to slice.
fn apply_over(clause: &Expr, source: &str) -> Vec<Effect> {
    let graph = spec();
    let (mut world, _) = world();
    let mut application = Application::new(
        Against {
            spec: &graph,
            module: "lending",
            source,
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
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
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
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
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
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
        Against {
            spec: &graph,
            module: "lending",
            source: "",
            sources: &NO_SOURCES,
            derived: &NOTHING,
        },
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

/// `ensures: exists copy` asserts rather than does, and is noted as an
/// assertion about what exists — not reported as a form nobody modelled.
#[test]
fn an_existence_assertion_is_noted_as_one() {
    let source = "ensures: exists copy\n";
    let clause =
        Expr::Exists { span: Span { start: 9, end: 20 }, operand: Box::new(ident("copy")) };
    assert_eq!(
        apply_over(&clause, source),
        vec![Effect::Noted { description: "exists copy".to_owned() }]
    );
}

// --- a field written with a value nothing settled (issue #27) ------------

/// What reading `object.field` back says, once the clause has run.
fn read_back(world: &World, id: &EntityId, called: &str) -> (Value, Vec<String>) {
    let env = inspect_sim::eval::Env::new(world, "lending", "").bind("it", Value::Ref(id.clone()));
    let read = inspect_sim::eval::eval(&field(ident("it"), called), &env);
    (read.value, read.unresolved.into_iter().map(|note| note.reason).collect())
}

#[test]
fn a_field_assigned_something_undecided_says_why_when_read_back() {
    // The reason belonged to the write, and reading the field back said only
    // that it was "set to a value nothing settled" — and a journey's `then`
    // said nothing at all.
    let copy = EntityId::new("Copy", 1);
    let (_, _, world) = apply(
        &assign(field(ident("copy"), "shelfmark"), ident("nobody")),
        &[("copy", Value::Ref(copy.clone()))],
    );
    let (value, reasons) = read_back(&world, &copy, "shelfmark");
    assert_eq!(value, Value::Unknown);
    assert!(reasons.first().is_some_and(|reason| reason.contains("`nobody`")), "{reasons:?}");
}

#[test]
fn a_creation_argument_nothing_settled_says_why_when_read_back() {
    let (_, _, world) = apply(&creation("Loan", vec![("copy", ident("nobody"))]), &[]);
    let (value, reasons) = read_back(&world, &EntityId::new("Loan", 1), "copy");
    assert_eq!(value, Value::Unknown);
    assert!(reasons.first().is_some_and(|reason| reason.contains("`nobody`")), "{reasons:?}");
}

#[test]
fn settling_a_field_later_forgets_why_it_was_not() {
    let copy = EntityId::new("Copy", 1);
    let (_, _, mut world) = apply(
        &assign(field(ident("copy"), "shelfmark"), ident("nobody")),
        &[("copy", Value::Ref(copy.clone()))],
    );
    world.set_field(&copy, "shelfmark", Value::Str("QA78".to_owned()));
    let (value, reasons) = read_back(&world, &copy, "shelfmark");
    assert_eq!(value, Value::Str("QA78".to_owned()));
    assert!(reasons.is_empty(), "{reasons:?}");
    let instance = world.instance(&copy).expect("the copy");
    assert!(instance.why("shelfmark").is_empty());
}

#[test]
fn an_instance_with_nothing_undecided_serialises_as_it_always_has() {
    // Recordings, snapshots and shared links hold worlds; a field that is
    // always written would change every one of them for nothing.
    let (world, copy) = world();
    let json = serde_json::to_string(world.instance(&copy).expect("the copy")).expect("JSON");
    assert!(!json.contains("why"), "{json}");
}

#[test]
fn why_a_field_was_not_settled_survives_a_round_trip() {
    // The browser holds the world between steps and sends it back.
    let copy = EntityId::new("Copy", 1);
    let (_, _, world) = apply(
        &assign(field(ident("copy"), "shelfmark"), ident("nobody")),
        &[("copy", Value::Ref(copy.clone()))],
    );
    let json = serde_json::to_string(&world).expect("JSON");
    let back: World = serde_json::from_str(&json).expect("the world reads back");
    assert_eq!(back, world);
    let (_, reasons) = read_back(&back, &copy, "shelfmark");
    assert!(reasons.first().is_some_and(|reason| reason.contains("`nobody`")), "{reasons:?}");
}

/// Read `it.<called>` back from a source with an em-dash ahead of the read, so
/// the note's anchor is checked by byte offset (stipulation 4).
fn read_at(world: &World, id: &EntityId, called: &str) -> (Value, Vec<inspect_sim::Unresolved>) {
    let source = format!("-- — \nrequires: it.{called} = 1");
    let at = source.find("it.").expect("present");
    let end = at + "it.".len() + called.len();
    let node = Expr::MemberAccess {
        span: Span { start: at, end },
        object: Box::new(Expr::Ident(Ident {
            span: Span { start: at, end: at + 2 },
            name: "it".into(),
        })),
        field: Ident { span: Span { start: at + 3, end }, name: called.to_owned() },
    };
    let env =
        inspect_sim::eval::Env::new(world, "lending", &source).bind("it", Value::Ref(id.clone()));
    let read = inspect_sim::eval::eval(&node, &env);
    (read.value, read.unresolved)
}

#[test]
fn a_field_read_back_names_the_read_and_says_what_the_write_could_not_settle() {
    // The write's own note named a sub-expression of another rule, at another
    // time. Read back, the note is about *this* read — `it.shelfmark` — and
    // the write's reason comes along as what was so when it was written. Once,
    // not beside a second, generic note saying less.
    let copy = EntityId::new("Copy", 1);
    let (_, _, world) = apply(
        &assign(field(ident("copy"), "shelfmark"), ident("nobody")),
        &[("copy", Value::Ref(copy.clone()))],
    );
    let (value, notes) = read_at(&world, &copy, "shelfmark");
    assert_eq!(value, Value::Unknown);
    assert_eq!(notes.len(), 1, "{notes:#?}");
    let note = &notes[0];
    assert_eq!(note.expression.as_deref(), Some("it.shelfmark"), "{note:?}");
    assert!(note.reason.starts_with("`Copy#1.shelfmark` was set to a value nothing settled"));
    assert!(note.reason.contains("when it was written: nothing is bound to `nobody`"), "{note:?}");
}

#[test]
fn a_field_set_undecided_with_no_reason_still_says_so_when_read_back() {
    let copy = EntityId::new("Copy", 1);
    let (mut world, _) = world();
    world.set_field(&copy, "shelfmark", Value::Unknown);
    let (value, notes) = read_at(&world, &copy, "shelfmark");
    assert_eq!(value, Value::Unknown);
    let said: Vec<_> = notes.iter().map(|note| note.reason.as_str()).collect();
    assert_eq!(said, ["`Copy#1.shelfmark` was set to a value nothing settled"]);
    assert_eq!(notes[0].expression.as_deref(), Some("it.shelfmark"));
}

#[test]
fn a_let_undecided_says_why_where_a_later_clause_reads_it() {
    // `let s = nobody` then `copy.shelfmark = s`: the binding was a bare
    // value, so the field was written with no reason, and read back said only
    // that it was undecided.
    let copy = EntityId::new("Copy", 1);
    let block = Expr::Block {
        span: NOWHERE,
        items: vec![
            Expr::LetExpr { span: NOWHERE, name: name("s"), value: Box::new(ident("nobody")) },
            assign(field(ident("copy"), "shelfmark"), ident("s")),
        ],
    };
    let (_, reasons, world) = apply(&block, &[("copy", Value::Ref(copy.clone()))]);
    // Said once at the rule, however many clauses read it.
    assert_eq!(reasons, ["nothing is bound to `nobody`"]);
    let (_, notes) = read_at(&world, &copy, "shelfmark");
    assert!(notes[0].reason.contains("`nobody`"), "{notes:#?}");
}
