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

use serde_json::Value;

use crate::ingest::json;

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
pub fn writes(ensures: &[Value], bound: Option<(&str, &str)>) -> Vec<Write> {
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
fn collect(value: &Value, bound: Option<(&str, &str)>, found: &mut Vec<Write>) {
    if let Some((tag, inner)) = json::tagged(value) {
        match tag {
            "Call" => creation(inner, found),
            "Comparison" => assignment(inner, bound, found),
            _ => {}
        }
    }
    match value {
        Value::Object(fields) => {
            for nested in fields.values() {
                collect(nested, bound, found);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect(nested, bound, found);
            }
        }
        _ => {}
    }
}

/// `Entity.created(field: value, …)` — the one shape that names both halves.
fn creation(call: &Value, found: &mut Vec<Write>) {
    let Some(function) = call.get("function") else { return };
    let Some(("MemberAccess", access)) = json::tagged(function) else { return };
    if named_field(access).as_deref() != Some("created") {
        return;
    }
    let Some(entity) = direct_identifier(access.get("object")) else { return };
    if !names_a_type(&entity) {
        return;
    }
    for argument in json::array(call, "args") {
        let Some(("Named", named)) = json::tagged(argument) else { continue };
        if let Some(field) = json::declared_name(named) {
            found.push(Write { entity: entity.clone(), field });
        }
    }
}

/// `something.field = value`, when `something` is an entity this pass can name.
fn assignment(comparison: &Value, bound: Option<(&str, &str)>, found: &mut Vec<Write>) {
    if json::string_or_empty(comparison, "op") != "Eq" {
        return;
    }
    let Some(left) = comparison.get("left") else { return };
    let Some(("MemberAccess", access)) = json::tagged(left) else { return };
    let Some(field) = named_field(access) else { return };
    // A direct identifier only. `entry.message.status` is an assignment to a
    // field of whatever `message` is, and this pass does not resolve types.
    let Some(object) = direct_identifier(access.get("object")) else { return };

    let entity = if names_a_type(&object) {
        object
    } else {
        match bound {
            Some((binding, entity)) if binding == object => entity.to_owned(),
            _ => return,
        }
    };
    found.push(Write { entity, field });
}

/// The `field` half of a member access.
fn named_field(access: &Value) -> Option<String> {
    access.get("field").and_then(|field| json::string(field, "name"))
}

/// The name, when the object is an identifier and not a nested expression.
fn direct_identifier(object: Option<&Value>) -> Option<String> {
    let (tag, inner) = json::tagged(object?)?;
    if tag != "Ident" {
        return None;
    }
    json::string(inner, "name")
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
    use serde_json::json;

    use super::*;

    fn ident(name: &str) -> Value {
        json!({ "Ident": { "span": { "start": 0, "end": 0 }, "name": name } })
    }

    fn access(object: Value, field: &str) -> Value {
        json!({ "MemberAccess": {
            "span": { "start": 0, "end": 0 },
            "object": object,
            "field": { "span": { "start": 0, "end": 0 }, "name": field },
        }})
    }

    fn assign(left: Value, op: &str) -> Value {
        json!({ "Comparison": {
            "span": { "start": 0, "end": 0 },
            "left": left,
            "op": op,
            "right": ident("whatever"),
        }})
    }

    fn created(entity: &str, fields: &[&str]) -> Value {
        json!({ "Call": {
            "span": { "start": 0, "end": 0 },
            "function": access(ident(entity), "created"),
            "args": fields
                .iter()
                .map(|field| json!({ "Named": {
                    "name": { "span": { "start": 0, "end": 0 }, "name": field },
                    "value": ident("x"),
                }}))
                .collect::<Vec<_>>(),
        }})
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
            &[assign(access(ident("entry"), "status"), "Eq")],
            Some(("entry", "OutboxEntry")),
        );
        assert_eq!(pairs(&found), ["OutboxEntry.status"]);
    }

    #[test]
    fn an_assignment_naming_its_entity_needs_no_binding() {
        let found = writes(&[assign(access(ident("Loan"), "status"), "Eq")], None);
        assert_eq!(pairs(&found), ["Loan.status"]);
    }

    #[test]
    fn an_assignment_through_a_name_nobody_bound_is_left_alone() {
        // Saying nothing is the point. A reader checking which rules write a
        // field is deciding whether it is safe to change how it is written, and
        // a rule listed there wrongly is worse than the grep they would have
        // done instead.
        let found = writes(
            &[assign(access(ident("somebody"), "status"), "Eq")],
            Some(("entry", "OutboxEntry")),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_field_of_a_field_is_left_alone() {
        // `entry.message.status` assigns a field of whatever `message` is, and
        // working that out means resolving a type. This pass does not.
        let nested = access(access(ident("entry"), "message"), "status");
        let found = writes(&[assign(nested, "Eq")], Some(("entry", "OutboxEntry")));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_comparison_that_is_not_an_assignment_writes_nothing() {
        // `ensures: entry.status != queued` asserts about the end state rather
        // than setting it, which is how the simulator reads it too.
        let found =
            writes(&[assign(access(ident("entry"), "status"), "NotEq")], Some(("entry", "Loan")));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_assignment_inside_a_block_is_found() {
        // Multi-statement `ensures` is ordinary Allium and the commonest way a
        // rule sets more than one field.
        let block = json!({ "Block": { "statements": [
            assign(access(ident("entry"), "status"), "Eq"),
            assign(access(ident("entry"), "settled_at"), "Eq"),
        ]}});
        let found = writes(&[block], Some(("entry", "OutboxEntry")));
        assert_eq!(pairs(&found), ["OutboxEntry.settled_at", "OutboxEntry.status"]);
    }

    #[test]
    fn an_assignment_inside_a_loop_or_a_branch_is_found() {
        let inside = json!({ "For": {
            "binding": { "name": "device" },
            "body": json!({ "Conditional": {
                "condition": ident("something"),
                "then": assign(access(ident("entry"), "hub_holds"), "Eq"),
            }}),
        }});
        let found = writes(&[inside], Some(("entry", "OutboxEntry")));
        assert_eq!(pairs(&found), ["OutboxEntry.hub_holds"]);
    }

    #[test]
    fn a_field_set_in_two_branches_is_one_write() {
        // Otherwise the panel lists the same rule twice for the same field.
        let both = json!({ "Conditional": {
            "then": assign(access(ident("entry"), "status"), "Eq"),
            "otherwise": assign(access(ident("entry"), "status"), "Eq"),
        }});
        let found = writes(&[both], Some(("entry", "Loan")));
        assert_eq!(pairs(&found), ["Loan.status"]);
    }

    #[test]
    fn a_qualified_entity_keeps_the_namespace_the_author_wrote() {
        // `link` resolves it from there, and the capitalisation that says it is
        // a type is on the last segment rather than the first.
        let found = writes(&[created("messaging/Message", &["status"])], None);
        assert_eq!(pairs(&found), ["messaging/Message.status"]);
    }

    #[test]
    fn a_call_that_is_not_a_creation_writes_nothing() {
        // A rule emitting a trigger is `MessageSent(message)`, and a call to
        // something else entirely is `groups_visible_to(device)`.
        let emission = json!({ "Call": {
            "span": { "start": 0, "end": 0 },
            "function": ident("MessageSent"),
            "args": [{ "Positional": ident("message") }],
        }});
        let other = json!({ "Call": {
            "span": { "start": 0, "end": 0 },
            "function": access(ident("entry"), "recalculate"),
            "args": [],
        }});
        assert!(writes(&[emission, other], Some(("entry", "Loan"))).is_empty());
    }

    #[test]
    fn a_lower_case_object_that_is_not_the_binding_writes_nothing() {
        // `config.retention = …` is not an entity field, and neither is any
        // other lower-case name this rule did not bind.
        let found = writes(&[assign(access(ident("config"), "retention"), "Eq")], None);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn nothing_at_all_comes_of_an_empty_rule() {
        assert!(writes(&[], None).is_empty());
        assert!(writes(&[json!({})], Some(("entry", "Loan"))).is_empty());
    }
}
