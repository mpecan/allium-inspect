//! The expression trees, taken from allium's parser without going through JSON.
//!
//! A pass of its own, and deliberately not folded into the ones beside it. The
//! passes that build the *graph* read a JSON document, because that is the
//! shape three of the four sources arrive in and because what the graph wants
//! from a clause is the text the author wrote. What the *simulator* wants is
//! the tree, and allium already has it typed — so this walks the typed module
//! once and hands the trees straight over.
//!
//! Keyed by node id, which is how [`crate::Program`] is asked for them and how
//! the graph pass refers to the same rule.

use allium_parser::ast::{BlockItemKind, BlockKind, Decl, Expr, Module as Ast};

use crate::{
    NodeKind, SpecGraph,
    graph::NodeId,
    program::{Boundary, Program, RuleAst},
};

#[cfg(test)]
use crate::{Node, NodeDetail, program::derived_key};

/// Add every rule, invariant and computed field `ast` declares to `program`.
///
/// `graph` is read, never written: it already holds this module's entities,
/// because the model pass runs first, and it is the only thing that knows which
/// of an entity's assignments are *computed* rather than declared. `status:
/// draft | live` and `active: devices where status = live` are the same shape
/// in the tree and different things entirely, and guessing between them by
/// looking at the expression would mean re-deciding, worse, something allium
/// has already decided.
pub fn ingest(ast: &Ast, module: &str, graph: &SpecGraph, program: &mut Program) {
    for declaration in &ast.declarations {
        if let Decl::Block(block) = declaration
            && let Some(name) = &block.name
        {
            match block.kind {
                BlockKind::Entity | BlockKind::Value => {
                    derived(block, module, &name.name, graph, program);
                }
                BlockKind::Surface => {
                    program.add_boundary(
                        NodeId::new(module, NodeKind::Surface, &name.name).as_str(),
                        boundary(block),
                    );
                }
                _ => {}
            }
        }

        match declaration {
            Decl::Block(block) if block.kind == BlockKind::Rule => {
                let Some(name) = &block.name else { continue };
                program.add_rule(
                    NodeId::new(module, NodeKind::Rule, &name.name).as_str(),
                    rule(block),
                );
            }
            // `invariant Name { … }` at the top level.
            Decl::Invariant(invariant) => {
                program.add_invariant(
                    NodeId::new(module, NodeKind::Invariant, &invariant.name.name).as_str(),
                    invariant.body.clone(),
                );
            }
            // And the same written inside an entity or value block, which is
            // where a constraint about one type usually lives.
            Decl::Block(block) => {
                for item in &block.items {
                    if let BlockItemKind::InvariantBlock { name, body } = &item.kind {
                        program.add_invariant(
                            NodeId::new(module, NodeKind::Invariant, &name.name).as_str(),
                            body.clone(),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// What one surface shows, as expressions.
///
/// Two clauses out of the block and nothing else: `context`, because the
/// `exposes` items refer to it by name, and `exposes` itself. `provides` and
/// `@guarantee` are the graph's business — a panel draws them and nothing
/// evaluates them.
fn boundary(block: &allium_parser::ast::BlockDecl) -> Boundary {
    let mut boundary = Boundary::default();

    for item in &block.items {
        let BlockItemKind::Clause { keyword, value } = &item.kind else { continue };
        match keyword.as_str() {
            // `context identity: Identity` is a binding: the name the exposes
            // clause uses, and the type an actor has to be to stand in it.
            "context" => {
                if let Expr::Binding { name, value, .. } = value
                    && let Some(entity) = entity_named(value)
                {
                    boundary.context = Some((name.name.clone(), entity));
                }
            }
            "exposes" => boundary.exposes = Some(value.clone()),
            _ => {}
        }
    }

    boundary
}

/// The entity a `context` binding names, unqualified.
///
/// `context group: membership/Group` and `context group: Group` are the same
/// declaration written from two distances, and the parser gives them two
/// shapes: an `Ident` for the near one and a `QualifiedName` for the far one.
/// Reading only the near shape dropped every cross-module context on the
/// floor, and a surface whose context was dropped exposes its whole boundary
/// off a name nothing binds — so `Conversation`, which is scoped to a
/// `membership/Group` and shows a group's messages, reported "nothing is bound
/// to `group`" about every field it carries. A sentence about this pass,
/// presented as a fact about the specification.
///
/// The qualifier is dropped rather than kept because what this is compared
/// against is an instance's entity, and those are bare — `world::create` takes
/// the tail of `identity/Identity`. Two bare names, or two qualified ones;
/// one of each is the comparison that silently never matches.
fn entity_named(value: &Expr) -> Option<String> {
    match value {
        Expr::Ident(entity) => Some(entity.name.clone()),
        Expr::QualifiedName(entity) => Some(entity.name.clone()),
        _ => None,
    }
}

/// The computed fields of one entity.
///
/// A relationship and a derived value are both recorded, because the simulator
/// reaches them the same way: a field nobody wrote is answered by evaluating
/// its definition. A *stored* field's assignment is its type, and evaluating
/// that would turn "nobody set `expires_at`" into whatever `Timestamp` happens
/// to evaluate to — a confident wrong answer in place of an honest unknown.
fn derived(
    block: &allium_parser::ast::BlockDecl,
    module: &str,
    entity: &str,
    graph: &SpecGraph,
    program: &mut Program,
) {
    let Some(detail) = graph
        .nodes_of(NodeKind::Entity)
        .chain(graph.nodes_of(NodeKind::Value))
        .find(|node| node.module == module && node.name == entity)
        .and_then(|node| node.detail.as_entity())
    else {
        return;
    };

    for item in &block.items {
        // `ParamAssignment` — `safety_number_of(this)` — is deliberately not
        // here. It takes arguments, so it is a function rather than a field,
        // and nothing reads it by name.
        let BlockItemKind::Assignment { name, value } = &item.kind else { continue };
        if detail.field(&name.name).is_some_and(|field| field.derived || field.relationship) {
            program.add_derived(module, entity, &name.name, value.clone());
        }
    }
}

/// One rule's clauses, in the order the spec declares them.
fn rule(block: &allium_parser::ast::BlockDecl) -> RuleAst {
    let mut ast = RuleAst::default();
    for item in &block.items {
        match &item.kind {
            BlockItemKind::Clause { keyword, value } => match keyword.as_str() {
                // No "first only" guard: a rule with two `when` clauses does
                // not parse — allium reports "expected expression, found ':'"
                // and the second never reaches here as a clause at all. A guard
                // against it would be a branch nothing can take.
                "when" => ast.when = Some(value.clone()),
                "requires" => ast.requires.push(value.clone()),
                "ensures" => ast.ensures.push(value.clone()),
                _ => {}
            },
            // `let existing = ContactName{…}` — the rule's own name for
            // something its arguments determine. Its own item kind, not a
            // clause, which is why it fell through the match above and out of
            // the program entirely.
            BlockItemKind::Let { name, value } => {
                ast.lets.push((name.name.clone(), value.clone()));
            }
            // `for x in collection:` wrapping the rule's body. The clauses
            // inside it are the rule's own, so they are collected as if they
            // had been written at the top, and the iteration is recorded
            // beside them.
            //
            // Nothing applies it yet. A hoisted clause mentions the loop's
            // binding, which is unbound outside the loop, so the rule comes
            // back undecided naming that binding — which is the honest answer
            // and not a useful one. Recording the iteration is what a future
            // implementation needs; see `RuleAst::iterate`.
            BlockItemKind::ForBlock { binding, collection, filter, items } => {
                ast.iterate = Some(Expr::For {
                    span: item.span,
                    binding: binding.clone(),
                    collection: Box::new(collection.clone()),
                    filter: filter.clone().map(Box::new),
                    body: Box::new(Expr::Block { span: item.span, items: Vec::new() }),
                });
                for inner in items {
                    if let BlockItemKind::Clause { keyword, value } = &inner.kind {
                        match keyword.as_str() {
                            "requires" => ast.requires.push(value.clone()),
                            "ensures" => ast.ensures.push(value.clone()),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    ast
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The program a spec's text produces.
    ///
    /// Parsed rather than hand-built. This pass exists to read allium's tree,
    /// so a fixture that is not one proves nothing about it — and the source
    /// below is what a person would actually write.
    fn program_of(source: &str) -> Program {
        against(source, &SpecGraph::new("test"))
    }

    fn against(source: &str, graph: &SpecGraph) -> Program {
        let mut program = Program::new();
        ingest(&allium_parser::parse(source).module, "lending", graph, &mut program);
        program
    }

    /// A graph that says which of `Member`'s fields are computed.
    ///
    /// Hand-built here rather than ingested, because what this pass needs from
    /// the graph is exactly those two flags and nothing else — and a fixture
    /// that ran the CLI to get them would be testing the model pass instead.
    fn member_graph() -> SpecGraph {
        use crate::graph::{EntityDetail, EntityField, EntityKind};

        fn field(name: &str, derived: bool, relationship: bool) -> EntityField {
            EntityField {
                name: name.to_owned(),
                type_expr: String::new(),
                enum_values: Vec::new(),
                derived,
                relationship,
                when: None,
                note: Vec::new(),
            }
        }

        fn entity(module: &str, name: &str, fields: Vec<EntityField>) -> Node {
            let mut node = Node::new(module, NodeKind::Entity, name);
            node.detail = NodeDetail::Entity(EntityDetail {
                kind: EntityKind::Internal,
                fields,
                transitions: Vec::new(),
                parent: None,
            });
            node
        }

        let mut graph = SpecGraph::new("test");
        // Two decoys, and both are here to be *not* found. One shares the
        // module and one shares the name, each with the same fields declared
        // stored — so a lookup that matched on either half alone would take a
        // decoy's word for it and record nothing, which is a wrong answer that
        // looks exactly like a spec with no computed fields.
        graph.nodes.push(entity(
            "lending",
            "Copy",
            vec![field("loans", false, false), field("open_loans", false, false)],
        ));
        graph.nodes.push(entity(
            "catalogue",
            "Member",
            vec![field("loans", false, false), field("open_loans", false, false)],
        ));
        graph.nodes.push(entity(
            "lending",
            "Member",
            vec![
                field("name", false, false),
                field("loans", false, true),
                field("open_loans", true, false),
            ],
        ));
        graph
    }

    const MEMBER: &str = "
entity Member {
    name: String
    loans: Loan with member = this
    open_loans: loans where status = open
}
";

    #[test]
    fn a_computed_field_is_kept_with_the_expression_that_computes_it() {
        let program = against(MEMBER, &member_graph());
        assert!(
            program.derivations().contains_key(&derived_key("lending", "Member", "open_loans")),
            "{:?}",
            program.derivations().keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_relationship_is_kept_too() {
        let program = against(MEMBER, &member_graph());
        assert!(program.derivations().contains_key(&derived_key("lending", "Member", "loans")));
    }

    /// The one that matters. `name: String` is an assignment in the tree, the
    /// same shape as the two above, and evaluating it would turn "nobody set
    /// `name`" into whatever `String` happens to evaluate to.
    #[test]
    fn a_stored_field_is_not_kept() {
        let program = against(MEMBER, &member_graph());
        assert!(!program.derivations().contains_key(&derived_key("lending", "Member", "name")));
        assert_eq!(program.derivations().len(), 2);
    }

    /// The graph is what knows. Without it nothing is computed, which is the
    /// honest answer rather than a guess from the expression's shape.
    #[test]
    fn nothing_is_kept_for_an_entity_the_graph_does_not_have() {
        assert!(against(MEMBER, &SpecGraph::new("test")).derivations().is_empty());
    }

    #[test]
    fn a_computed_field_is_filed_under_its_own_module_and_entity() {
        let program = against(MEMBER, &member_graph());
        assert_eq!(derived_key("lending", "Member", "loans"), "lending::Member.loans");
        assert!(!program.derivations().contains_key("catalogue::Member.loans"));
        assert!(!program.derivations().contains_key("lending::Copy.loans"));
    }

    /// A rule's assignments are not an entity's, and a rule block reaching this
    /// would file its `let` bindings as fields of something.
    const SHELF: &str = "
surface MyLoans {
    facing reader: Reader
    context borrower: Member

    exposes:
        borrower.name
        for loan in borrower.open_loans:
            loan.status

    provides:
        MemberReturns(loan)
}
";

    fn boundary_of(source: &str) -> crate::Boundary {
        program_of(source)
            .boundary(NodeId::new("lending", NodeKind::Surface, "MyLoans").as_str())
            .cloned()
            .expect("the surface")
    }

    #[test]
    fn a_surface_keeps_the_binding_its_exposes_clause_refers_to() {
        // Both halves. The name is what `borrower.open_loans` means, and the
        // type is what decides whether a given actor can stand in it.
        assert_eq!(boundary_of(SHELF).context, Some(("borrower".to_owned(), "Member".to_owned())));
    }

    #[test]
    fn a_surface_keeps_its_exposes_clause_as_an_expression() {
        let exposes = boundary_of(SHELF).exposes.expect("the clause");
        let Expr::Block { items, .. } = exposes else { panic!("expected a block: {exposes:?}") };

        assert_eq!(items.len(), 2, "a path and an iteration");
        assert!(matches!(items[0], Expr::MemberAccess { .. }));
        assert!(matches!(items[1], Expr::For { .. }));
    }

    /// `provides` and `@guarantee` are the graph's business — a panel draws
    /// them and nothing evaluates them — so they must not arrive here as
    /// exposures that would then be matched against.
    #[test]
    fn nothing_but_context_and_exposes_is_kept() {
        let boundary = boundary_of(SHELF);
        let printed = format!("{:?}", boundary.exposes);
        assert!(!printed.contains("MemberReturns"), "{printed}");
    }

    /// A context declared across a module boundary is the same declaration.
    ///
    /// The parser hands back a `QualifiedName` rather than an `Ident` for it,
    /// and reading only the second dropped the context of every surface scoped
    /// to something another module declares — which then reported "nothing is
    /// bound to `group`" about every field it carries.
    #[test]
    fn a_context_from_another_module_is_kept_under_its_bare_name() {
        let boundary = boundary_of(
            "\nsurface MyLoans {\n    facing reader: people/Reader\n    context borrower: \
             people/Member\n\n    exposes:\n        borrower.name\n}\n",
        );
        // Bare, because what this is compared against is an instance's entity
        // and those carry the tail of `people/Member`.
        assert_eq!(boundary.context, Some(("borrower".to_owned(), "Member".to_owned())));
    }

    #[test]
    fn a_surface_with_no_context_keeps_none() {
        let boundary = program_of(
            "surface Open {\n    facing reader: Reader\n\n    exposes:\n        Loan.status\n}\n",
        )
        .boundary(NodeId::new("lending", NodeKind::Surface, "Open").as_str())
        .cloned()
        .expect("the surface");

        assert_eq!(boundary.context, None);
        assert!(boundary.exposes.is_some());
    }

    /// A rule is not a surface, and one arriving here would file its clauses
    /// as somebody's boundary.
    #[test]
    fn only_surfaces_get_a_boundary() {
        assert!(
            program_of(BORROW)
                .boundary(NodeId::new("lending", NodeKind::Surface, "BorrowCopy").as_str())
                .is_none()
        );
    }

    #[test]
    fn only_entity_and_value_blocks_contribute() {
        let program = against(BORROW, &member_graph());
        assert!(program.derivations().is_empty());
    }

    fn rule_of(source: &str) -> RuleAst {
        program_of(source).rule("lending::rule::BorrowCopy").cloned().expect("the rule")
    }

    const BORROW: &str = "
rule BorrowCopy {
    when: MemberBorrows(member, copy)

    requires: copy.status = available
    requires: not member.is_at_limit

    ensures: Loan.created(copy: copy, member: member)
    ensures: copy.status = on_loan
    ensures: CopyBorrowed(copy: copy)
}
";

    #[test]
    fn each_clause_lands_under_the_keyword_it_was_written_with() {
        // Not interchangeable: a precondition applied as a postcondition would
        // write a field the rule only meant to read.
        let ast = rule_of(BORROW);
        assert!(ast.when.is_some());
        assert_eq!(ast.requires.len(), 2, "{:#?}", ast.requires);
        assert_eq!(ast.ensures.len(), 3, "{:#?}", ast.ensures);
    }

    /// A rule's own name for something its arguments determine.
    ///
    /// `Let` is its own item kind rather than a clause, so it fell through the
    /// keyword match and out of the program entirely — and `requires: exists
    /// existing` then evaluated a name nothing had bound, leaving the act
    /// undecided. Three of `friend-mesh`'s four contact-naming rules are that
    /// shape, so a whole surface's worth of acts was undecidable.
    #[test]
    fn a_rules_own_let_is_kept_with_its_clauses() {
        let ast = rule_of(
            "\nrule BorrowCopy {\n    when: MemberBorrows(member, copy)\n\n    let standing = \
             Reservation{member: member, copy: copy}\n\n    requires: exists standing\n}\n",
        );
        assert_eq!(ast.lets.len(), 1, "{:#?}", ast.lets);
        assert_eq!(ast.lets[0].0, "standing");
    }

    /// In declaration order, because a later one may read an earlier one — the
    /// same reason `ensures` clauses keep theirs.
    #[test]
    fn two_lets_keep_the_order_they_were_written_in() {
        let ast = rule_of(
            "\nrule BorrowCopy {\n    when: MemberBorrows(member, copy)\n\n    let held = \
             Reservation{member: member}\n    let owner = held.member\n\n    requires: exists \
             owner\n}\n",
        );
        let names: Vec<&str> = ast.lets.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["held", "owner"]);
    }

    /// And a rule without one has none, so the absence is a fact rather than a
    /// default that would hide a dropped binding.
    #[test]
    fn a_rule_with_no_let_has_none() {
        assert!(rule_of(BORROW).lets.is_empty());
    }

    #[test]
    fn the_clauses_keep_the_order_they_were_declared_in() {
        // Load-bearing for `ensures`: `Loan.created(…)` binds `loan`, and the
        // clause after it emits a trigger naming what the first one made.
        let ast = rule_of(BORROW);
        let first = &ast.ensures[0];
        assert!(
            matches!(first, Expr::Call { function, .. } if matches!(
                &**function, Expr::MemberAccess { field, .. } if field.name == "created"
            )),
            "the creation comes first: {first:?}"
        );
    }

    #[test]
    fn the_when_clause_is_the_trigger_the_rule_waits_for() {
        let ast = rule_of(BORROW);
        let Some(Expr::Call { function, .. }) = &ast.when else {
            panic!("a trigger call: {:?}", ast.when)
        };
        assert!(
            matches!(&**function, Expr::Ident(name) if name.name == "MemberBorrows"),
            "{function:?}"
        );
    }

    #[test]
    fn a_state_condition_when_is_a_binding_rather_than_a_call() {
        // What the simulator matches on to decide a rule waits on the world
        // rather than on somebody. Read as anything else, every state rule
        // becomes one nothing can ever fire.
        let ast = program_of(
            "
rule LoanFallsOverdue {
    when: loan: Loan.window.due_at <= now
    ensures: loan.status = overdue
}
",
        )
        .rule("lending::rule::LoanFallsOverdue")
        .cloned()
        .expect("the rule");
        let Some(Expr::Binding { name, .. }) = &ast.when else {
            panic!("a binding: {:?}", ast.when)
        };
        assert_eq!(name.name, "loan");
    }

    #[test]
    fn a_block_that_is_not_a_rule_does_not_become_one() {
        // Every construct in a spec is a block of some kind. Without the guard
        // an entity would arrive in the program with no clauses, and the
        // simulator reports a clause-less rule as unsimulatable — so a spec
        // would appear to be full of rules nobody could run.
        let program = program_of(
            "
entity Loan {
    status: open | returned
}

surface MemberShelf {
    facing reader: Reader
}
",
        );
        assert_eq!(program.rule_count(), 0, "{:?}", program.rules().collect::<Vec<_>>());
    }

    #[test]
    fn an_invariant_written_inside_an_entity_is_found() {
        // Where a constraint about one type usually lives. Read only from the
        // top level, every one of them would go unchecked — and an invariant
        // nothing evaluates is the failure this tool exists to prevent.
        let program = program_of(
            "
entity Loan {
    status: open | returned

    invariant AReturnedLoanIsClosed {
        status = returned implies closed_at != null
    }
}
",
        );
        assert!(program.invariant("lending::invariant::AReturnedLoanIsClosed").is_some());
        assert_eq!(program.invariant_count(), 1);
    }

    #[test]
    fn an_invariant_written_at_the_top_level_is_found_too() {
        let program = program_of(
            "
invariant OpenLoansAreWithinTheLimit {
    for m in Members: m.open_loan_count <= config.loan_limit
}
",
        );
        assert!(program.invariant("lending::invariant::OpenLoansAreWithinTheLimit").is_some());
    }

    #[test]
    fn a_rule_that_iterates_keeps_both_the_iteration_and_the_clauses_inside_it() {
        // `for d in member.devices:` wraps the body rather than replacing it.
        // Dropping the inner clauses leaves a rule that iterates over nothing
        // and is reported as trivially succeeding.
        let ast = rule_of(
            "
rule BorrowCopy {
    when: MemberBorrows(member, copy)

    for d in member.devices:
        requires: d.status = active
        ensures: d.notified = true
}
",
        );
        assert!(ast.iterate.is_some(), "the iteration is recorded");
        assert_eq!(ast.requires.len(), 1, "{:#?}", ast.requires);
        assert_eq!(ast.ensures.len(), 1, "{:#?}", ast.ensures);
    }

    #[test]
    fn a_rule_with_nothing_in_it_is_empty_rather_than_absent() {
        // Distinguished by the simulator: an unparsed rule is unsimulatable,
        // and a rule with no preconditions succeeds whenever its trigger fires.
        let program = program_of("rule BorrowCopy {\n}\n");
        assert!(program.rule("lending::rule::BorrowCopy").expect("present").is_empty());
    }
}
