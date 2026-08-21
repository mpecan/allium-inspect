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
                // First only. A rule with two `when` clauses is not something
                // the language allows, and taking the last would quietly
                // disagree with the trigger the graph pass named.
                "when" if ast.when.is_none() => ast.when = Some(value.clone()),
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
