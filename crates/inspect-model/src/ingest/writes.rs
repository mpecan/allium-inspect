//! Which entity fields a rule's postconditions assign.
//!
//! Priya's question — "before I change how this field is written, which rules
//! write it?" — had no answer anywhere in the tool, and her fallback was reading
//! a hundred and twelve rules or grepping. The CLI does not say; the graph
//! carried `creates` and `reads` at entity granularity and nothing at all at
//! field granularity.
//!
//! It is knowable, but only sometimes, and this pass is deliberately built to
//! say nothing rather than to guess:
//!
//! ```text
//! ensures: OutboxEntry.created(status: queued, …)   -- certain: the type is named
//! ensures: entry.status = settled                   -- certain when `when` bound
//!                                                      `entry` to an entity
//! ensures: entry.message.status = tombstoned        -- not certain: the field is
//!                                                      on whatever `message` is
//! ```
//!
//! The third is skipped. Working out what `entry.message` is means resolving a
//! field's type, and a wrong answer here would tell a reader that a rule writes
//! a field it does not touch — which is worse for them than the grep they were
//! going to do anyway. What comes out is sound and incomplete, and the panel
//! that shows it says so.

use allium_parser::ast::{CallArg, ComparisonOp, Expr};

/// One field a rule assigns, and the entity it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Write {
    /// The entity as the spec named it, which may be qualified: `messaging/Message`.
    pub entity: String,
    pub field: String,
}

/// The fields `ensures` assigns, as far as they can be known for certain.
///
/// `bound` is the name a state-condition `when` binds and the entity it binds
/// it to — `("entry", "OutboxEntry")` for `when: entry: OutboxEntry.status = …`.
/// Without one, only assignments that name their entity outright are read.
#[must_use]
pub fn writes(ensures: &[Expr], bound: Option<(&str, &str)>) -> Vec<Write> {
    let mut found = Vec::new();
    for clause in ensures {
        collect(clause, bound, &mut found);
    }
    // Sorted and deduplicated: a rule that sets a field in two branches of a
    // conditional writes it once, and the order must not depend on which branch
    // the parser happened to emit first.
    found.sort();
    found.dedup();
    found
}

/// Walk everything, because an assignment can be inside a block, a loop or a
/// branch and all three are ordinary Allium.
fn collect(expr: &Expr, bound: Option<(&str, &str)>, found: &mut Vec<Write>) {
    match expr {
        Expr::Call { function, args, .. } => creation(function, args, found),
        Expr::Comparison { left, op, .. } => assignment(left, *op, bound, found),
        _ => {}
    }
    for child in children(expr) {
        collect(child, bound, found);
    }
}

/// `Entity.created(field: value, …)` — the one shape that names both halves.
fn creation(function: &Expr, args: &[CallArg], found: &mut Vec<Write>) {
    let Expr::MemberAccess { object, field, .. } = function else { return };
    if field.name != "created" {
        return;
    }
    let Some(entity) = direct_identifier(object) else { return };
    if !names_a_type(&entity) {
        return;
    }
    for argument in args {
        if let CallArg::Named(named) = argument {
            found.push(Write { entity: entity.clone(), field: named.name.name.clone() });
        }
    }
}

/// `something.field = value`, when `something` is an entity this pass can name.
fn assignment(left: &Expr, op: ComparisonOp, bound: Option<(&str, &str)>, found: &mut Vec<Write>) {
    if op != ComparisonOp::Eq {
        return;
    }
    let Expr::MemberAccess { object, field, .. } = left else { return };
    // A direct identifier only. `entry.message.status` is an assignment to a
    // field of whatever `message` is, and this pass does not resolve types.
    let Some(object) = direct_identifier(object) else { return };

    let entity = if names_a_type(&object) {
        object
    } else {
        match bound {
            Some((binding, entity)) if binding == object => entity.to_owned(),
            _ => return,
        }
    };
    found.push(Write { entity, field: field.name.clone() });
}

/// The name, when the object is an identifier and not a nested expression.
fn direct_identifier(object: &Expr) -> Option<String> {
    match object {
        Expr::Ident(ident) => Some(ident.name.clone()),
        Expr::QualifiedName(qualified) => Some(match &qualified.qualifier {
            Some(module) => format!("{module}/{}", qualified.name),
            None => qualified.name.clone(),
        }),
        _ => None,
    }
}

/// Every sub-expression of `expr`, in source order.
///
/// Exhaustive on purpose, and the reason this pass is worth having typed: an
/// expression form the language gains stops compiling here instead of being
/// walked past. The old version recursed over any JSON object it met, which
/// never failed to compile and never failed to miss something either.
fn children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        // Leaves.
        Expr::Ident(_)
        | Expr::StringLiteral(_)
        | Expr::BacktickLiteral { .. }
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::Null { .. }
        | Expr::Now { .. }
        | Expr::This { .. }
        | Expr::Within { .. }
        | Expr::DurationLiteral { .. }
        | Expr::QualifiedName(_) => Vec::new(),

        // One child.
        Expr::Not { operand, .. }
        | Expr::Exists { operand, .. }
        | Expr::NotExists { operand, .. }
        | Expr::TypeOptional { inner: operand, .. } => vec![operand],
        Expr::MemberAccess { object, .. } | Expr::OptionalAccess { object, .. } => vec![object],
        Expr::ProjectionMap { source, .. } => vec![source],
        Expr::Binding { value, .. } | Expr::LetExpr { value, .. } => vec![value],

        // Two.
        Expr::NullCoalesce { left, right, .. }
        | Expr::BinaryOp { left, right, .. }
        | Expr::Comparison { left, right, .. }
        | Expr::LogicalOp { left, right, .. }
        | Expr::Pipe { left, right, .. } => vec![left, right],
        Expr::In { element, collection, .. } | Expr::NotIn { element, collection, .. } => {
            vec![element, collection]
        }
        Expr::Where { source, condition, .. } => vec![source, condition],
        Expr::With { source, predicate, .. } => vec![source, predicate],
        Expr::Lambda { param, body, .. } => vec![param, body],
        Expr::TransitionsTo { subject, new_state, .. }
        | Expr::Becomes { subject, new_state, .. } => vec![subject, new_state],
        Expr::WhenGuard { action, condition, .. } => vec![action, condition],

        // Sequences.
        Expr::SetLiteral { elements, .. }
        | Expr::ListLiteral { elements, .. }
        | Expr::Block { items: elements, .. } => elements.iter().collect(),
        Expr::ObjectLiteral { fields, .. } => fields.iter().map(|field| &field.value).collect(),
        Expr::GenericType { name, args, .. } => {
            std::iter::once(&**name).chain(args.iter()).collect()
        }
        Expr::Call { function, args, .. } => std::iter::once(&**function)
            .chain(args.iter().map(|arg| match arg {
                CallArg::Positional(value) => value,
                CallArg::Named(named) => &named.value,
            }))
            .collect(),
        Expr::JoinLookup { entity, fields, .. } => std::iter::once(&**entity)
            .chain(fields.iter().filter_map(|field| field.value.as_ref()))
            .collect(),
        Expr::Conditional { branches, else_body, .. } => branches
            .iter()
            .flat_map(|branch| [&branch.condition, &branch.body])
            .chain(else_body.as_deref())
            .collect(),
        Expr::For { collection, filter, body, .. } => std::iter::once(&**collection)
            .chain(filter.as_deref())
            .chain(std::iter::once(&**body))
            .collect(),
    }
}

/// Whether a name is a type rather than a binding.
///
/// Allium capitalises type names and lower-cases bindings, and a qualified name
/// is capitalised after the slash. This is the language's own convention rather
/// than a heuristic — but it is checked on the last segment, because
/// `messaging/Message` is a type and `messaging` is not.
fn names_a_type(name: &str) -> bool {
    name.rsplit('/').next().is_some_and(|last| last.chars().next().is_some_and(char::is_uppercase))
}

#[cfg(test)]
mod tests {
    use allium_parser::{
        Span,
        ast::{CondBranch, ForBinding, Ident, NamedArg},
    };

    use super::*;

    const NOWHERE: Span = Span { start: 0, end: 0 };

    fn name(text: &str) -> Ident {
        Ident { span: NOWHERE, name: text.to_owned() }
    }

    fn ident(text: &str) -> Expr {
        Expr::Ident(name(text))
    }

    fn access(object: Expr, field: &str) -> Expr {
        Expr::MemberAccess { span: NOWHERE, object: Box::new(object), field: name(field) }
    }

    fn assign(left: Expr, op: ComparisonOp) -> Expr {
        Expr::Comparison {
            span: NOWHERE,
            left: Box::new(left),
            op,
            right: Box::new(ident("whatever")),
        }
    }

    fn created(entity: &str, fields: &[&str]) -> Expr {
        Expr::Call {
            span: NOWHERE,
            function: Box::new(access(ident(entity), "created")),
            args: fields
                .iter()
                .map(|field| {
                    CallArg::Named(NamedArg { span: NOWHERE, name: name(field), value: ident("x") })
                })
                .collect(),
        }
    }

    fn pairs(writes: &[Write]) -> Vec<String> {
        writes.iter().map(|write| format!("{}.{}", write.entity, write.field)).collect()
    }

    #[test]
    fn a_creation_names_every_field_it_sets() {
        // The one shape that names both halves outright, and the commonest.
        let found = writes(&[created("Archive", &["owner", "prepared_at", "covers"])], None);
        assert_eq!(pairs(&found), ["Archive.covers", "Archive.owner", "Archive.prepared_at"]);
    }

    #[test]
    fn an_assignment_through_the_when_binding_is_resolved() {
        // `when: entry: OutboxEntry.is_settled` / `ensures: entry.status = settled`
        // is a whole third of the rules in a real spec, and without the binding
        // there is nothing in the clause that says which entity it is about.
        let found = writes(
            &[assign(access(ident("entry"), "status"), ComparisonOp::Eq)],
            Some(("entry", "OutboxEntry")),
        );
        assert_eq!(pairs(&found), ["OutboxEntry.status"]);
    }

    #[test]
    fn an_assignment_naming_its_entity_needs_no_binding() {
        let found = writes(&[assign(access(ident("Loan"), "status"), ComparisonOp::Eq)], None);
        assert_eq!(pairs(&found), ["Loan.status"]);
    }

    #[test]
    fn an_assignment_through_a_name_nobody_bound_is_left_alone() {
        // Saying nothing is the point. A reader checking which rules write a
        // field is deciding whether it is safe to change how it is written, and
        // a rule listed there wrongly is worse than the grep they would have
        // done instead.
        let found = writes(
            &[assign(access(ident("somebody"), "status"), ComparisonOp::Eq)],
            Some(("entry", "OutboxEntry")),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_field_of_a_field_is_left_alone() {
        // `entry.message.status` assigns a field of whatever `message` is, and
        // working that out means resolving a type. This pass does not.
        let nested = access(access(ident("entry"), "message"), "status");
        let found = writes(&[assign(nested, ComparisonOp::Eq)], Some(("entry", "OutboxEntry")));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_comparison_that_is_not_an_assignment_writes_nothing() {
        // `ensures: entry.status != queued` asserts about the end state rather
        // than setting it, which is how the simulator reads it too.
        let found = writes(
            &[assign(access(ident("entry"), "status"), ComparisonOp::NotEq)],
            Some(("entry", "Loan")),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_assignment_inside_a_block_is_found() {
        // Multi-statement `ensures` is ordinary Allium and the commonest way a
        // rule sets more than one field.
        let block = Expr::Block {
            span: NOWHERE,
            items: vec![
                assign(access(ident("entry"), "status"), ComparisonOp::Eq),
                assign(access(ident("entry"), "settled_at"), ComparisonOp::Eq),
            ],
        };
        let found = writes(&[block], Some(("entry", "OutboxEntry")));
        assert_eq!(pairs(&found), ["OutboxEntry.settled_at", "OutboxEntry.status"]);
    }

    #[test]
    fn an_assignment_inside_a_loop_or_a_branch_is_found() {
        let branch = Expr::Conditional {
            span: NOWHERE,
            branches: vec![CondBranch {
                span: NOWHERE,
                condition: ident("something"),
                body: assign(access(ident("entry"), "hub_holds"), ComparisonOp::Eq),
            }],
            else_body: None,
        };
        let inside = Expr::For {
            span: NOWHERE,
            binding: ForBinding::Single(name("device")),
            collection: Box::new(ident("Devices")),
            filter: None,
            body: Box::new(branch),
        };
        let found = writes(&[inside], Some(("entry", "OutboxEntry")));
        assert_eq!(pairs(&found), ["OutboxEntry.hub_holds"]);
    }

    #[test]
    fn a_field_set_in_two_branches_is_one_write() {
        // Otherwise the panel lists the same rule twice for the same field.
        let both = Expr::Conditional {
            span: NOWHERE,
            branches: vec![CondBranch {
                span: NOWHERE,
                condition: ident("something"),
                body: assign(access(ident("entry"), "status"), ComparisonOp::Eq),
            }],
            else_body: Some(Box::new(assign(access(ident("entry"), "status"), ComparisonOp::Eq))),
        };
        let found = writes(&[both], Some(("entry", "Loan")));
        assert_eq!(pairs(&found), ["Loan.status"]);
    }

    #[test]
    fn a_qualified_entity_keeps_the_namespace_the_author_wrote() {
        // `link` resolves it from there, and the capitalisation that says it is
        // a type is on the last segment rather than the first.
        let qualified = Expr::QualifiedName(allium_parser::ast::QualifiedName {
            span: NOWHERE,
            qualifier: Some("messaging".to_owned()),
            name: "Message".to_owned(),
        });
        let call = Expr::Call {
            span: NOWHERE,
            function: Box::new(access(qualified, "created")),
            args: vec![CallArg::Named(NamedArg {
                span: NOWHERE,
                name: name("status"),
                value: ident("x"),
            })],
        };
        assert_eq!(pairs(&writes(&[call], None)), ["messaging/Message.status"]);
    }

    #[test]
    fn a_call_that_is_not_a_creation_writes_nothing() {
        // A rule emitting a trigger is `MessageSent(message)`, and a call to
        // something else entirely is `groups_visible_to(device)`.
        let emission = Expr::Call {
            span: NOWHERE,
            function: Box::new(ident("MessageSent")),
            args: vec![CallArg::Positional(ident("message"))],
        };
        let other = Expr::Call {
            span: NOWHERE,
            function: Box::new(access(ident("entry"), "recalculate")),
            args: Vec::new(),
        };
        assert!(writes(&[emission, other], Some(("entry", "Loan"))).is_empty());
    }

    #[test]
    fn a_lower_case_object_that_is_not_the_binding_writes_nothing() {
        // `config.retention = …` is not an entity field, and neither is any
        // other lower-case name this rule did not bind.
        let found = writes(&[assign(access(ident("config"), "retention"), ComparisonOp::Eq)], None);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn nothing_at_all_comes_of_an_empty_rule() {
        assert!(writes(&[], None).is_empty());
        assert!(writes(&[ident("nothing")], Some(("entry", "Loan"))).is_empty());
    }

    #[test]
    fn every_form_the_language_has_can_be_walked_into() {
        // The exhaustive `children` match is what makes that true, and this is
        // the case that would have been missed without it: an assignment buried
        // under a form nobody thought to recurse through.
        let buried = Expr::Not {
            span: NOWHERE,
            operand: Box::new(Expr::Where {
                span: NOWHERE,
                source: Box::new(ident("Loans")),
                condition: Box::new(assign(access(ident("entry"), "status"), ComparisonOp::Eq)),
            }),
        };
        let found = writes(&[buried], Some(("entry", "Loan")));
        assert_eq!(pairs(&found), ["Loan.status"]);
    }
}
