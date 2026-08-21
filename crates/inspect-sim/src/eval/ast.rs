//! Small readers over allium's expression tree, and the truth/value bridge.
//!
//! Separated from the evaluator because they are about the *shape* of an
//! expression rather than about what it means.
//!
//! This module used to be twice as long and was mostly about JSON: which key
//! holds the tag, whether a name is a bare string or a spanned identifier,
//! whether a span survived being nested one level deeper than expected. None of
//! those are questions any more. The tree arrives typed from `allium_parser`,
//! so the shape is the compiler's problem and what is left is the two things
//! that are genuinely about meaning.

use allium_parser::ast::Expr;
use inspect_model::Span;

use crate::{truth::Truth, value::Value};

/// Where `expr` is in the source.
///
/// Exhaustive, and that is the point: every undecided note this evaluator emits
/// quotes the sub-expression it could not settle, and a form the language gains
/// stops compiling here rather than producing a note with nothing to point at.
#[must_use]
pub fn span_of(expr: &Expr) -> Option<Span> {
    let span = match expr {
        Expr::Ident(ident) => &ident.span,
        Expr::StringLiteral(literal) => &literal.span,
        Expr::QualifiedName(qualified) => &qualified.span,
        Expr::BacktickLiteral { span, .. }
        | Expr::NumberLiteral { span, .. }
        | Expr::BoolLiteral { span, .. }
        | Expr::Null { span }
        | Expr::Now { span }
        | Expr::This { span }
        | Expr::Within { span }
        | Expr::DurationLiteral { span, .. }
        | Expr::SetLiteral { span, .. }
        | Expr::ListLiteral { span, .. }
        | Expr::ObjectLiteral { span, .. }
        | Expr::GenericType { span, .. }
        | Expr::MemberAccess { span, .. }
        | Expr::OptionalAccess { span, .. }
        | Expr::NullCoalesce { span, .. }
        | Expr::Call { span, .. }
        | Expr::JoinLookup { span, .. }
        | Expr::BinaryOp { span, .. }
        | Expr::Comparison { span, .. }
        | Expr::LogicalOp { span, .. }
        | Expr::Not { span, .. }
        | Expr::In { span, .. }
        | Expr::NotIn { span, .. }
        | Expr::Exists { span, .. }
        | Expr::NotExists { span, .. }
        | Expr::Where { span, .. }
        | Expr::With { span, .. }
        | Expr::Pipe { span, .. }
        | Expr::Lambda { span, .. }
        | Expr::Conditional { span, .. }
        | Expr::For { span, .. }
        | Expr::ProjectionMap { span, .. }
        | Expr::TransitionsTo { span, .. }
        | Expr::Becomes { span, .. }
        | Expr::Binding { span, .. }
        | Expr::WhenGuard { span, .. }
        | Expr::TypeOptional { span, .. }
        | Expr::LetExpr { span, .. }
        | Expr::Block { span, .. } => span,
    };
    Some(Span::new(span.start, span.end))
}

/// The name of a bare identifier, when that is what `expr` is.
#[must_use]
pub fn bare_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// Whether `expr` is the identifier `name`.
#[must_use]
pub fn is_ident_named(expr: &Expr, name: &str) -> bool {
    bare_name(expr) == Some(name)
}

/// The text of a string literal, with any interpolation left out.
///
/// A spec writes `"borrowed by {member.name}"` and this evaluator has no way to
/// fill the hole in — so it keeps the parts it can read. Nothing compares
/// against an interpolated string in a real spec; what they are for is prose in
/// a guidance annotation.
#[must_use]
pub fn literal_text(literal: &allium_parser::ast::StringLiteral) -> String {
    use allium_parser::ast::StringPart;
    literal
        .parts
        .iter()
        .map(|part| match part {
            StringPart::Text(text) => text.as_str(),
            StringPart::Interpolation(_) => "",
        })
        .collect()
}

/// A truth as the value an expression yields.
///
/// Undecided becomes [`Value::Unknown`] rather than `Bool(false)`, which is the
/// whole reason the two types exist separately.
#[must_use]
pub fn truth_value(truth: Truth) -> Value {
    match truth {
        Truth::True => Value::Bool(true),
        Truth::False => Value::Bool(false),
        Truth::Unknown => Value::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use allium_parser::{Span as AstSpan, ast::Ident};

    use super::*;

    fn ident(name: &str) -> Expr {
        Expr::Ident(Ident { span: AstSpan::new(3, 9), name: name.to_owned() })
    }

    #[test]
    fn every_expression_can_say_where_it_is() {
        // The undecided note quotes the source it could not settle, so an
        // expression with no span is one a reader cannot be shown.
        assert_eq!(span_of(&ident("book")), Some(Span::new(3, 9)));
        assert_eq!(span_of(&Expr::Now { span: AstSpan::new(1, 4) }), Some(Span::new(1, 4)));
        assert_eq!(
            span_of(&Expr::Not { span: AstSpan::new(0, 12), operand: Box::new(ident("x")) }),
            Some(Span::new(0, 12))
        );
    }

    #[test]
    fn a_bare_identifier_is_recognised_and_anything_else_is_not() {
        assert_eq!(bare_name(&ident("available")), Some("available"));
        assert_eq!(bare_name(&Expr::Now { span: AstSpan::new(0, 3) }), None);
        assert!(is_ident_named(&ident("config"), "config"));
        assert!(!is_ident_named(&ident("other"), "config"));
        assert!(!is_ident_named(&Expr::Now { span: AstSpan::new(0, 3) }, "config"));
    }

    #[test]
    fn undecided_becomes_unknown_rather_than_false() {
        // The whole reason Truth and Value are separate types.
        assert_eq!(truth_value(Truth::True), Value::Bool(true));
        assert_eq!(truth_value(Truth::False), Value::Bool(false));
        assert_eq!(truth_value(Truth::Unknown), Value::Unknown);
    }
}
