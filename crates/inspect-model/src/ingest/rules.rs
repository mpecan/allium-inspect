//! Reading rules and the triggers they wait for, out of the `parse` AST.
//!
//! A rule is a trigger, some preconditions and some postconditions, and the
//! interesting question is which of two kinds of trigger it waits for:
//!
//! ```text
//! when: MemberBorrows(member, copy)          -- external stimulus
//! when: loan: Loan.window.due_at <= now      -- state condition
//! ```
//!
//! The first is something a person does, so the simulator lets the user fire
//! it. The second is something that *becomes* true, so the simulator offers it
//! once a change makes it hold — and walking that chain from an actor's first
//! action to a terminal state is the closest an Allium spec gets to describing a
//! user journey. Everything downstream of this module depends on the two being
//! told apart, which is what [`trigger_from_when`] does.
//!
//! Clause text is sliced from the source rather than printed back from the AST.
//! The spec's author wrote `attachment_size <= config.max_attachment_bytes`, and
//! that is what they should read in the inspector — not this crate's idea of how
//! to spell it.

use serde_json::Value;

use crate::{
    graph::{
        Edge, EdgeKind, Node, NodeDetail, NodeId, NodeKind, RuleClause, RuleDetail, SpecGraph,
        TriggerDetail, TriggerSource,
    },
    ingest::json,
    span::Span,
};

/// Add the rule declared by `block` to `graph`, along with its trigger.
pub fn ingest(block: &Value, module: &str, source: &str, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(block) else { return };
    let rule_id = NodeId::new(module, NodeKind::Rule, &name);

    let mut clauses = Vec::new();
    let mut trigger = None;

    for item in json::array(block, "items") {
        let Some(clause) = item.get("kind").and_then(|kind| kind.get("Clause")) else { continue };
        let keyword = json::string_or_empty(clause, "keyword");
        let span = json::span(item, "span");
        clauses.push(RuleClause {
            keyword: keyword.clone(),
            text: clause_text(clause, span, source),
            span,
        });
        if keyword == "when" && trigger.is_none() {
            trigger = clause.get("value").map(trigger_from_when);
        }
    }

    let (trigger_name, detail) = trigger.unwrap_or_else(|| {
        // A rule with no `when` cannot be fired by anything. It is kept rather
        // than dropped — a rule that exists and can never run is exactly the
        // sort of thing someone opens this tool to notice.
        (
            String::new(),
            TriggerDetail {
                source: TriggerSource::External,
                parameters: Vec::new(),
                condition: None,
                entity: None,
            },
        )
    });

    if !trigger_name.is_empty() {
        let trigger_id = NodeId::new(module, NodeKind::Trigger, &trigger_name);
        graph.nodes.push(
            Node::new(module, NodeKind::Trigger, &trigger_name)
                .with(NodeDetail::Trigger(detail.clone())),
        );
        graph.edges.push(
            Edge::new(trigger_id, rule_id.clone(), EdgeKind::Triggers, trigger_name.clone())
                .at(json::span(block, "span")),
        );
    }

    graph.nodes.push(Node::new(module, NodeKind::Rule, &name).at(json::span(block, "span")).with(
        NodeDetail::Rule(RuleDetail {
            trigger: trigger_name,
            source: detail.source,
            clauses,
            creates: Vec::new(),
            emits: Vec::new(),
        }),
    ));
}

/// The text of a clause's value, as the spec wrote it.
///
/// The clause's own span covers the keyword too, so the value's span is
/// preferred when it has one. Falling back to the whole clause is better than
/// showing nothing, and showing nothing is better than showing a reconstruction
/// the author would not recognise.
fn clause_text(clause: &Value, clause_span: Option<Span>, source: &str) -> String {
    let value_span = clause.get("value").and_then(expression_span);
    value_span.or(clause_span).and_then(|span| span.slice(source)).map(collapse).unwrap_or_default()
}

/// The span of an expression node, whatever its tag.
fn expression_span(value: &Value) -> Option<Span> {
    let (_, inner) = json::tagged(value)?;
    json::span(inner, "span")
}

/// Squeeze a multi-line clause onto one line for a label.
///
/// Multi-line `ensures` blocks are common and their indentation is meaningful
/// in the file and meaningless in a 200-pixel node. The full text with its
/// original layout is still reachable through the clause's span.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut spaced = true;
    for character in text.chars() {
        if character.is_whitespace() {
            if !spaced {
                out.push(' ');
                spaced = true;
            }
        } else {
            out.push(character);
            spaced = false;
        }
    }
    out.trim_end().to_owned()
}

/// The trigger a `when` clause names, and how it happens.
///
/// Returns the trigger's name and its detail. A state condition has no name of
/// its own in the spec, so it is named after the entity it watches — `Loan` for
/// `when: loan: Loan.window.due_at <= now` — which is how it appears in the flow
/// view and how the simulator lists it.
pub fn trigger_from_when(value: &Value) -> (String, TriggerDetail) {
    match json::tagged(value) {
        Some(("Call", call)) => {
            let name = call
                .get("function")
                .and_then(|function| json::tagged(function))
                .and_then(|(_, inner)| json::string(inner, "name"))
                .unwrap_or_default();
            (
                name,
                TriggerDetail {
                    source: TriggerSource::External,
                    parameters: call_parameters(call),
                    condition: None,
                    entity: None,
                },
            )
        }
        Some(("Binding", binding)) => {
            let condition = binding.get("value");
            let entity = condition.and_then(root_identifier);
            // The clock is what separates a state condition from a temporal
            // one, and the simulator treats them differently: a temporal rule
            // only becomes enabled when `now` is advanced.
            let source = if condition.is_some_and(mentions_now) {
                TriggerSource::Temporal
            } else {
                TriggerSource::State
            };
            (
                entity.clone().unwrap_or_default(),
                TriggerDetail {
                    source,
                    parameters: json::declared_name(binding).into_iter().collect(),
                    condition: None,
                    entity,
                },
            )
        }
        _ => (
            String::new(),
            TriggerDetail {
                source: TriggerSource::External,
                parameters: Vec::new(),
                condition: None,
                entity: None,
            },
        ),
    }
}

/// The positional and named argument names of a call.
fn call_parameters(call: &Value) -> Vec<String> {
    json::array(call, "args")
        .iter()
        .filter_map(|argument| match json::tagged(argument)? {
            ("Positional", inner) => {
                json::tagged(inner).and_then(|(_, node)| json::string(node, "name"))
            }
            ("Named", inner) => json::declared_name(inner),
            _ => None,
        })
        .collect()
}

/// The leftmost identifier of a navigation chain.
///
/// `Loan.window.due_at` nests as `MemberAccess(MemberAccess(Ident(Loan)))`, and
/// the entity being watched is at the bottom of it.
fn root_identifier(value: &Value) -> Option<String> {
    match json::tagged(value)? {
        ("Ident", inner) => json::string(inner, "name"),
        ("MemberAccess", inner) => root_identifier(inner.get("object")?),
        ("Comparison" | "BinaryOp" | "LogicalOp", inner) => root_identifier(inner.get("left")?),
        ("Not" | "Exists" | "NotExists", inner) => {
            root_identifier(inner.get("operand").or_else(|| inner.get("value"))?)
        }
        ("Where", inner) => root_identifier(inner.get("source")?),
        _ => None,
    }
}

/// Whether an expression reads the clock anywhere inside it.
fn mentions_now(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.contains_key("Now") || fields.values().any(mentions_now),
        Value::Array(items) => items.iter().any(mentions_now),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ident(name: &str) -> Value {
        json!({"Ident": {"span": {"start": 0, "end": 0}, "name": name}})
    }

    fn member(object: Value, field: &str) -> Value {
        json!({"MemberAccess": {
            "span": {"start": 0, "end": 0},
            "object": object,
            "field": {"span": {"start": 0, "end": 0}, "name": field},
        }})
    }

    #[test]
    fn a_call_when_clause_is_an_external_stimulus() {
        let when = json!({"Call": {
            "span": {"start": 0, "end": 0},
            "function": ident("MemberBorrows"),
            "args": [
                {"Positional": ident("member")},
                {"Positional": ident("copy")},
            ],
        }});
        let (name, detail) = trigger_from_when(&when);
        assert_eq!(name, "MemberBorrows");
        assert_eq!(detail.source, TriggerSource::External);
        assert_eq!(detail.parameters, ["member", "copy"]);
    }

    #[test]
    fn a_binding_when_clause_is_a_state_condition_named_for_its_entity() {
        // `when: copy: Copy.status = lost`
        let when = json!({"Binding": {
            "span": {"start": 0, "end": 0},
            "name": {"span": {"start": 0, "end": 0}, "name": "copy"},
            "value": {"Comparison": {
                "span": {"start": 0, "end": 0},
                "left": member(ident("Copy"), "status"),
                "op": "Eq",
                "right": ident("lost"),
            }},
        }});
        let (name, detail) = trigger_from_when(&when);
        assert_eq!(name, "Copy", "a state condition is named for what it watches");
        assert_eq!(detail.source, TriggerSource::State);
        assert_eq!(detail.entity.as_deref(), Some("Copy"));
        assert_eq!(detail.parameters, ["copy"], "the binding name is the parameter");
    }

    #[test]
    fn a_state_condition_that_reads_the_clock_is_temporal() {
        // `when: loan: Loan.window.due_at <= now`. The distinction matters:
        // a temporal rule only becomes enabled when the user advances `now`,
        // so treating it as an ordinary state condition would make it appear
        // enabled the moment its entity exists.
        let when = json!({"Binding": {
            "span": {"start": 0, "end": 0},
            "name": {"span": {"start": 0, "end": 0}, "name": "loan"},
            "value": {"Comparison": {
                "span": {"start": 0, "end": 0},
                "left": member(member(ident("Loan"), "window"), "due_at"),
                "op": "LtEq",
                "right": {"Now": {"span": {"start": 0, "end": 0}}},
            }},
        }});
        let (name, detail) = trigger_from_when(&when);
        assert_eq!(name, "Loan");
        assert_eq!(detail.source, TriggerSource::Temporal);
    }

    #[test]
    fn the_root_identifier_is_found_through_a_nesting_chain() {
        assert_eq!(
            root_identifier(&member(member(ident("Loan"), "window"), "due_at")).as_deref(),
            Some("Loan")
        );
    }

    #[test]
    fn the_root_identifier_is_found_through_a_negation_and_a_filter() {
        let negated = json!({"Not": {"span": {"start": 0, "end": 0}, "operand": ident("Member")}});
        assert_eq!(root_identifier(&negated).as_deref(), Some("Member"));

        let filtered = json!({"Where": {
            "span": {"start": 0, "end": 0},
            "source": ident("Loan"),
            "condition": ident("x"),
        }});
        assert_eq!(root_identifier(&filtered).as_deref(), Some("Loan"));
    }

    #[test]
    fn an_expression_with_no_identifier_root_yields_none() {
        assert_eq!(root_identifier(&json!({"Now": {}})), None);
        assert_eq!(root_identifier(&json!({"a": 1, "b": 2})), None);
    }

    #[test]
    fn mentions_now_finds_the_clock_at_any_depth() {
        assert!(mentions_now(&json!({"Now": {}})));
        assert!(mentions_now(&json!({"a": {"b": [{"Now": {}}]}})));
        assert!(!mentions_now(&json!({"a": {"b": [{"Ident": {"name": "now_ish"}}]}})));
        assert!(!mentions_now(&json!("now")), "a string is not the clock");
    }

    #[test]
    fn a_when_clause_of_an_unknown_shape_yields_an_unnamed_external_trigger() {
        let (name, detail) = trigger_from_when(&json!({"SomethingNew": {}}));
        assert!(name.is_empty());
        assert_eq!(detail.source, TriggerSource::External);
    }

    #[test]
    fn collapse_puts_a_multi_line_clause_on_one_line() {
        let text = "Loan.created(\n    copy: copy,\n    member: member\n)";
        assert_eq!(collapse(text), "Loan.created( copy: copy, member: member )");
    }

    #[test]
    fn collapse_leaves_a_single_line_alone() {
        assert_eq!(collapse("copy.status = available"), "copy.status = available");
    }

    #[test]
    fn collapse_of_whitespace_only_text_is_empty() {
        assert_eq!(collapse("   \n\t "), "");
    }

    // --- whole-rule ingestion -------------------------------------------

    const SOURCE: &str = "rule BorrowCopy {\n    when: MemberBorrows(member, copy)\n\n    requires: copy.status = available\n\n    ensures: copy.status = on_loan\n}\n";

    fn clause_item(keyword: &str, value: Value, start: usize, end: usize) -> Value {
        json!({
            "span": {"start": start, "end": end},
            "kind": {"Clause": {"keyword": keyword, "value": value}},
        })
    }

    /// The byte range of `needle` in [`SOURCE`].
    ///
    /// Derived rather than written out: hand-counted offsets in a fixture are
    /// wrong as soon as the fixture is edited, and a slicing test whose spans
    /// do not match its own source proves nothing about slicing.
    fn at(needle: &str) -> (usize, usize) {
        let start = SOURCE.find(needle).unwrap_or_else(|| panic!("SOURCE contains {needle:?}"));
        (start, start + needle.len())
    }

    fn borrow_block() -> Value {
        let (when_start, when_end) = at("MemberBorrows(member, copy)");
        let (requires_start, requires_end) = at("copy.status = available");
        let (when_clause_start, _) = at("when: MemberBorrows(member, copy)");
        let (requires_clause_start, _) = at("requires: copy.status = available");

        let when_value = json!({"Call": {
            "span": {"start": when_start, "end": when_end},
            "function": ident("MemberBorrows"),
            "args": [{"Positional": ident("member")}, {"Positional": ident("copy")}],
        }});
        let requires_value = json!({"Comparison": {
            "span": {"start": requires_start, "end": requires_end},
            "left": member(ident("copy"), "status"),
            "op": "Eq",
            "right": ident("available"),
        }});
        json!({
            "span": {"start": 0, "end": SOURCE.len()},
            "kind": "Rule",
            "name": {"span": {"start": 5, "end": 15}, "name": "BorrowCopy"},
            "items": [
                clause_item("when", when_value, when_clause_start, when_end),
                clause_item("requires", requires_value, requires_clause_start, requires_end),
            ],
        })
    }

    fn ingested() -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        ingest(&borrow_block(), "lending", SOURCE, &mut graph);
        graph
    }

    #[test]
    fn a_rule_becomes_a_node_carrying_its_clauses() {
        let graph = ingested();
        let node =
            graph.node(&NodeId::new("lending", NodeKind::Rule, "BorrowCopy")).expect("the rule");
        let detail = node.detail.as_rule().expect("a rule detail");
        assert_eq!(detail.trigger, "MemberBorrows");
        assert_eq!(detail.source, TriggerSource::External);
        assert_eq!(detail.clauses.len(), 2);
        assert_eq!(node.span, Some(Span::new(0, SOURCE.len())));
    }

    #[test]
    fn clause_text_is_sliced_from_the_source_the_author_wrote() {
        let graph = ingested();
        let detail = graph
            .node(&NodeId::new("lending", NodeKind::Rule, "BorrowCopy"))
            .and_then(|node| node.detail.as_rule())
            .expect("the rule");
        let requires = detail.clauses_of("requires").next().expect("one requires clause");
        assert_eq!(requires.text, "copy.status = available");
    }

    #[test]
    fn a_rule_creates_its_trigger_node_and_the_edge_into_itself() {
        let graph = ingested();
        let trigger_id = NodeId::new("lending", NodeKind::Trigger, "MemberBorrows");
        assert!(graph.node(&trigger_id).is_some(), "the trigger is a node of its own");

        let edge = graph.edges.iter().find(|edge| edge.kind == EdgeKind::Triggers);
        let edge = edge.expect("a triggers edge");
        assert_eq!(edge.from, trigger_id);
        assert_eq!(edge.to, NodeId::new("lending", NodeKind::Rule, "BorrowCopy"));
    }

    #[test]
    fn a_rule_with_no_when_clause_is_kept_rather_than_dropped() {
        // A rule nothing can fire is exactly the sort of thing this tool exists
        // to make visible. Dropping it would hide the problem.
        let block = json!({
            "span": {"start": 0, "end": 10},
            "kind": "Rule",
            "name": {"span": {"start": 0, "end": 0}, "name": "Unfireable"},
            "items": [],
        });
        let mut graph = SpecGraph::new("test");
        ingest(&block, "lending", "", &mut graph);
        let detail = graph
            .node(&NodeId::new("lending", NodeKind::Rule, "Unfireable"))
            .and_then(|node| node.detail.as_rule())
            .expect("the rule survives");
        assert_eq!(detail.trigger, "", "with no trigger to name");
        assert!(graph.edges.is_empty(), "and nothing pointing at it");
    }

    #[test]
    fn a_rule_with_no_name_is_dropped() {
        let mut graph = SpecGraph::new("test");
        ingest(&json!({"kind": "Rule", "items": []}), "lending", "", &mut graph);
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn only_the_first_when_clause_names_the_trigger() {
        // Two `when` clauses is not valid Allium, but the AST can carry it and
        // picking the last one would silently change which rule fires.
        let first = json!({"Call": {"span": {"start": 0, "end": 0}, "function": ident("First"), "args": []}});
        let second = json!({"Call": {"span": {"start": 0, "end": 0}, "function": ident("Second"), "args": []}});
        let block = json!({
            "span": {"start": 0, "end": 1},
            "kind": "Rule",
            "name": {"span": {"start": 0, "end": 0}, "name": "Twice"},
            "items": [clause_item("when", first, 0, 1), clause_item("when", second, 0, 1)],
        });
        let mut graph = SpecGraph::new("test");
        ingest(&block, "m", "", &mut graph);
        let detail = graph
            .node(&NodeId::new("m", NodeKind::Rule, "Twice"))
            .and_then(|node| node.detail.as_rule())
            .expect("the rule");
        assert_eq!(detail.trigger, "First");
    }

    #[test]
    fn a_named_argument_is_read_as_a_parameter() {
        let when = json!({"Call": {
            "span": {"start": 0, "end": 0},
            "function": ident("Emit"),
            "args": [{"Named": {"name": {"span": {"start": 0, "end": 0}, "name": "loan"}, "value": ident("l")}}],
        }});
        let (_, detail) = trigger_from_when(&when);
        assert_eq!(detail.parameters, ["loan"]);
    }
}
