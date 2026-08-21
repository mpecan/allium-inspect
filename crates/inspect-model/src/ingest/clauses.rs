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
    NodeKind,
    graph::NodeId,
    program::{Program, RuleAst},
};

/// Add every rule and invariant `ast` declares to `program`.
pub fn ingest(ast: &Ast, module: &str, program: &mut Program) {
    for declaration in &ast.declarations {
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
            // `for x in collection:` wrapping the rule's body. The clauses
            // inside it are the rule's own, so they are collected as if they
            // had been written at the top — the iteration is recorded
            // separately and the simulator applies it to all of them.
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
        let mut program = Program::new();
        ingest(&allium_parser::parse(source).module, "lending", &mut program);
        program
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
