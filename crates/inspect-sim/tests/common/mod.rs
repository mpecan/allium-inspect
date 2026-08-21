//! Building allium expressions by hand, for tests.
//!
//! The parser is the only thing that builds these in anger, and a whole module
//! of source per assertion would bury what each case is about. So these are the
//! shapes the tests need, spelled once — and spelled in allium's own types, so
//! a case here is a case the evaluator could actually be handed.

#![allow(dead_code, reason = "each test file uses the subset it needs")]

use allium_parser::{
    Span,
    ast::{
        BinaryOp, CallArg, ComparisonOp, CondBranch, Expr, ForBinding, Ident, LogicalOp, NamedArg,
        StringLiteral, StringPart,
    },
};

/// Spans are irrelevant to what these cases assert; the ones that care build
/// their own.
pub const NOWHERE: Span = Span { start: 0, end: 0 };

pub fn name(text: &str) -> Ident {
    Ident { span: NOWHERE, name: text.to_owned() }
}

pub fn ident(text: &str) -> Expr {
    Expr::Ident(name(text))
}

pub fn field(object: Expr, called: &str) -> Expr {
    Expr::MemberAccess { span: NOWHERE, object: Box::new(object), field: name(called) }
}

pub fn number(text: &str) -> Expr {
    Expr::NumberLiteral { span: NOWHERE, value: text.to_owned() }
}

pub fn text(value: &str) -> Expr {
    Expr::StringLiteral(StringLiteral {
        span: NOWHERE,
        parts: vec![StringPart::Text(value.to_owned())],
    })
}

pub fn boolean(value: bool) -> Expr {
    Expr::BoolLiteral { span: NOWHERE, value }
}

pub fn compare(left: Expr, op: ComparisonOp, right: Expr) -> Expr {
    Expr::Comparison { span: NOWHERE, left: Box::new(left), op, right: Box::new(right) }
}

pub fn assign(left: Expr, right: Expr) -> Expr {
    compare(left, ComparisonOp::Eq, right)
}

pub fn logical(left: Expr, op: LogicalOp, right: Expr) -> Expr {
    Expr::LogicalOp { span: NOWHERE, left: Box::new(left), op, right: Box::new(right) }
}

pub fn arithmetic(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    Expr::BinaryOp { span: NOWHERE, left: Box::new(left), op, right: Box::new(right) }
}

/// `Entity.created(field: value, …)`.
pub fn creation(entity: &str, fields: Vec<(&str, Expr)>) -> Expr {
    Expr::Call {
        span: NOWHERE,
        function: Box::new(field(ident(entity), "created")),
        args: fields
            .into_iter()
            .map(|(called, value)| {
                CallArg::Named(NamedArg { span: NOWHERE, name: name(called), value })
            })
            .collect(),
    }
}

/// `TriggerName(argument)`.
pub fn emission(trigger: &str) -> Expr {
    Expr::Call {
        span: NOWHERE,
        function: Box::new(ident(trigger)),
        args: vec![CallArg::Positional(ident("whatever"))],
    }
}

pub fn block(items: Vec<Expr>) -> Expr {
    Expr::Block { span: NOWHERE, items }
}

/// `if condition: body`, with no else branch.
pub fn conditional(condition: Expr, body: Expr) -> Expr {
    Expr::Conditional {
        span: NOWHERE,
        branches: vec![CondBranch { span: NOWHERE, condition, body }],
        else_body: None,
    }
}

/// `for binding in collection: body`.
pub fn iteration(binding: &str, collection: Expr, body: Expr) -> Expr {
    Expr::For {
        span: NOWHERE,
        binding: ForBinding::Single(name(binding)),
        collection: Box::new(collection),
        filter: None,
        body: Box::new(body),
    }
}

/// `not exists X`, at `span` so a note can quote it.
pub fn not_exists_at(operand: Expr, span: Span) -> Expr {
    Expr::NotExists { span, operand: Box::new(operand) }
}

pub fn not_exists(operand: Expr) -> Expr {
    not_exists_at(operand, NOWHERE)
}

/// Something real that this evaluator does not model.
pub fn unmodelled() -> Expr {
    Expr::Lambda { span: NOWHERE, param: Box::new(ident("x")), body: Box::new(ident("x")) }
}
