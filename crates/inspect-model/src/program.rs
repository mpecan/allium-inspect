//! The expression trees, kept beside the graph rather than inside it.
//!
//! The graph answers "what does this spec contain and how does it connect?",
//! and every clause in it is the text the author wrote. That is what a reader
//! wants and it is all the browser needs — the whole graph for a five-module
//! spec set is under half a megabyte because of it.
//!
//! A simulator needs something else: the parsed form of those clauses, so it can
//! evaluate them rather than display them. That is an order of magnitude more
//! data — the AST for one real spec set is measured in megabytes — and shipping
//! it to a browser that renders four views and never evaluates anything would be
//! paying the whole cost for none of the benefit.
//!
//! So it stays here. The server holds the [`Program`] alongside the graph, the
//! simulator runs against it in process, and what crosses the wire is a world
//! and a step outcome.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

/// The parsed clauses of one rule.
///
/// Allium's own tree, not a copy of it. The parser hands these over typed and
/// they stay typed all the way to the evaluator: an expression form the
/// language gains is then a non-exhaustive `match` at compile time rather than
/// a tag nobody wrote a branch for, which is the failure this used to have and
/// which reported itself as `unknown` at run time if it reported itself at all.
///
/// No `PartialEq`: `allium_parser::ast::Expr` does not derive it, and a
/// structural comparison of two expression trees is not a question anything
/// here asks.
#[derive(Debug, Clone, Default)]
pub struct RuleAst {
    /// The `when` clause: a trigger call, or a state condition.
    pub when: Option<Expr>,
    /// One entry per `requires` clause, in the order the spec declares them.
    ///
    /// Order matters for reporting rather than for logic: preconditions are
    /// conjunctive, but a reader looking at why a rule did not fire reads them
    /// top to bottom against the file.
    pub requires: Vec<Expr>,
    /// One entry per `ensures` clause, in declaration order.
    ///
    /// Order matters here for real: `Message.created(...)` binds `message`, and
    /// a later clause emitting `MessageSent(message: message)` depends on the
    /// earlier one having run.
    pub ensures: Vec<Expr>,
    /// The `for x in collection` clause, when the rule iterates.
    pub iterate: Option<Expr>,
}

impl RuleAst {
    /// Whether there is anything here to evaluate.
    ///
    /// A rule whose clauses all failed to parse is reported as unsimulatable
    /// rather than as a rule that trivially succeeds — the second is a claim
    /// nothing checked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.when.is_none() && self.requires.is_empty() && self.ensures.is_empty()
    }
}

/// Every expression tree the simulator can evaluate, keyed by node id.
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// Rule node id to its clauses.
    rules: BTreeMap<String, RuleAst>,
    /// Invariant node id to its condition.
    invariants: BTreeMap<String, Expr>,
}

impl Program {
    /// An empty program.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `ast` as the clauses of the rule with `id`.
    pub fn add_rule(&mut self, id: impl Into<String>, ast: RuleAst) {
        self.rules.insert(id.into(), ast);
    }

    /// Record `condition` as the body of the invariant with `id`.
    pub fn add_invariant(&mut self, id: impl Into<String>, condition: Expr) {
        self.invariants.insert(id.into(), condition);
    }

    /// The clauses of the rule with `id`.
    #[must_use]
    pub fn rule(&self, id: &str) -> Option<&RuleAst> {
        self.rules.get(id)
    }

    /// The condition of the invariant with `id`.
    #[must_use]
    pub fn invariant(&self, id: &str) -> Option<&Expr> {
        self.invariants.get(id)
    }

    /// Every rule, in id order.
    pub fn rules(&self) -> impl Iterator<Item = (&str, &RuleAst)> {
        self.rules.iter().map(|(id, ast)| (id.as_str(), ast))
    }

    /// Every invariant, in id order.
    pub fn invariants(&self) -> impl Iterator<Item = (&str, &Expr)> {
        self.invariants.iter().map(|(id, condition)| (id.as_str(), condition))
    }

    /// How many rules carry clauses.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// How many invariants carry a condition.
    #[must_use]
    pub fn invariant_count(&self) -> usize {
        self.invariants.len()
    }
}

#[cfg(test)]
mod tests {

    use allium_parser::{Span, ast::Ident};

    use super::*;

    /// A distinguishable expression; what it *is* does not matter here, only
    /// that two of them can be told apart.
    fn expr(name: &str) -> Expr {
        Expr::Ident(Ident { span: Span { start: 0, end: 0 }, name: name.to_owned() })
    }

    #[test]
    fn a_new_program_holds_nothing() {
        let program = Program::new();
        assert_eq!(program.rule_count(), 0);
        assert_eq!(program.invariant_count(), 0);
        assert!(program.rule("anything").is_none());
        assert!(program.invariant("anything").is_none());
    }

    #[test]
    fn a_rule_is_found_by_its_node_id() {
        let mut program = Program::new();
        program.add_rule(
            "lending::rule::BorrowCopy",
            RuleAst {
                when: Some(expr("when")),
                requires: vec![expr("requires")],
                ensures: vec![expr("first"), expr("second")],
                iterate: None,
            },
        );
        let ast = program.rule("lending::rule::BorrowCopy").expect("the rule");
        assert_eq!(ast.requires.len(), 1);
        assert_eq!(ast.ensures.len(), 2);
        assert!(ast.when.is_some());
    }

    #[test]
    fn an_invariant_is_found_by_its_node_id() {
        let mut program = Program::new();
        program.add_invariant("m::invariant::Bounded", expr("condition"));
        assert!(program.invariant("m::invariant::Bounded").is_some());
        assert_eq!(program.invariant_count(), 1);
    }

    #[test]
    fn adding_a_rule_twice_keeps_the_later_one() {
        // Ingestion is re-run wholesale on a file change, so the second answer
        // is the current one.
        let mut program = Program::new();
        program.add_rule("r", RuleAst { requires: vec![expr("first")], ..RuleAst::default() });
        program.add_rule("r", RuleAst { requires: vec![expr("second")], ..RuleAst::default() });
        let kept = &program.rule("r").expect("the rule").requires;
        assert!(matches!(&kept[..], [Expr::Ident(name)] if name.name == "second"), "{kept:?}");
        assert_eq!(program.rule_count(), 1);
    }

    #[test]
    fn a_rule_with_no_clauses_is_empty() {
        // Distinguished from a rule with no preconditions, which succeeds
        // whenever its trigger fires. An unparsed rule is reported as
        // unsimulatable instead, because "it succeeded" would be a claim
        // nothing checked.
        assert!(RuleAst::default().is_empty());
        assert!(!RuleAst { when: Some(expr("w")), ..RuleAst::default() }.is_empty());
        assert!(!RuleAst { ensures: vec![expr("e")], ..RuleAst::default() }.is_empty());
    }

    #[test]
    fn iteration_is_listed_in_id_order() {
        // Every enumeration the simulator does has to be ordered, or two
        // identical runs produce different traces.
        let mut program = Program::new();
        for id in ["m::rule::Zebra", "m::rule::Aardvark", "m::rule::Moose"] {
            program.add_rule(id, RuleAst::default());
        }
        let ids: Vec<&str> = program.rules().map(|(id, _)| id).collect();
        assert_eq!(ids, ["m::rule::Aardvark", "m::rule::Moose", "m::rule::Zebra"]);
    }

    #[test]
    fn invariants_are_listed_in_id_order_too() {
        let mut program = Program::new();
        program.add_invariant("m::invariant::B", expr("b"));
        program.add_invariant("m::invariant::A", expr("a"));
        let ids: Vec<&str> = program.invariants().map(|(id, _)| id).collect();
        assert_eq!(ids, ["m::invariant::A", "m::invariant::B"]);
    }
}
