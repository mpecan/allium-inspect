//! Reading `allium plan`: what each rule touches, and what the spec owes tests.
//!
//! Two different things come out of this document.
//!
//! The first is the **flow graph**. Every `rule_*` obligation carries a
//! `dependencies` block naming the entities the rule creates, the entities it
//! reads and the triggers it emits. That is the trigger → rule → entity →
//! trigger chain, already computed, and it is what the flow and journey views
//! draw. Deriving the same thing by interpreting `ensures` clauses would mean
//! reimplementing analysis the CLI has already done and can do better.
//!
//! The second is the **obligations overlay**: the test each construct owes. It
//! is shown against the node it belongs to, so a rule's card can say what would
//! have to be asserted for it to be considered covered.

use serde_json::Value;

use crate::{
    graph::{
        Edge, EdgeKind, Node, NodeDetail, NodeId, NodeKind, Obligation, SpecGraph, TriggerDetail,
        TriggerSource,
    },
    ingest::json,
};

/// Add everything `allium plan` reports for `module` to `graph`.
pub fn ingest(document: &Value, module: &str, graph: &mut SpecGraph) {
    for obligation in json::array(document, "obligations") {
        let Some(id) = json::string(obligation, "id") else { continue };
        let category = json::string_or_empty(obligation, "category");
        let construct = json::string_or_empty(obligation, "source_construct");

        if category.starts_with("rule_") {
            apply_dependencies(obligation, &construct, module, graph);
        }

        graph.obligations.push(Obligation {
            id,
            category,
            description: json::string_or_empty(obligation, "description"),
            construct,
            module: module.to_owned(),
            span: json::span(obligation, "source_span"),
        });
    }
}

/// Record what a rule creates, reads and emits, as detail and as edges.
fn apply_dependencies(obligation: &Value, rule: &str, module: &str, graph: &mut SpecGraph) {
    let Some(dependencies) = obligation.get("dependencies") else { return };
    if rule.is_empty() {
        return;
    }
    let rule_id = NodeId::new(module, NodeKind::Rule, rule);

    let created = json::strings(dependencies, "entities_created");
    let read = json::strings(dependencies, "entities_read");
    let emitted = json::strings(dependencies, "trigger_emissions");

    for entity in &created {
        graph.edges.push(Edge::new(
            rule_id.clone(),
            NodeId::new(module, NodeKind::Entity, entity),
            EdgeKind::Creates,
            entity.clone(),
        ));
    }
    for entity in &read {
        graph.edges.push(Edge::new(
            rule_id.clone(),
            NodeId::new(module, NodeKind::Entity, entity),
            EdgeKind::Reads,
            entity.clone(),
        ));
    }
    for trigger in &emitted {
        let trigger_id = NodeId::new(module, NodeKind::Trigger, trigger);
        // Trigger nodes are otherwise created only from the `when` clause that
        // waits for one, so a trigger this spec emits and nothing consumes had
        // no node and resolved to an unresolved external reference. It is not
        // unresolved — the spec declares it right here, by emitting it — and
        // "emitted, with nothing listening" is a fact worth seeing on the
        // canvas rather than a rendering accident.
        if !graph.nodes.iter().any(|node| node.id == trigger_id) {
            graph.nodes.push(Node::new(module, NodeKind::Trigger, trigger).with(
                NodeDetail::Trigger(TriggerDetail {
                    source: TriggerSource::External,
                    parameters: Vec::new(),
                    condition: None,
                    entity: None,
                }),
            ));
        }
        graph.edges.push(Edge::new(rule_id.clone(), trigger_id, EdgeKind::Emits, trigger.clone()));
    }

    // Several obligations describe the same rule — one success, one per failing
    // precondition, one per entity created — and each repeats the whole
    // dependency block. Merging rather than assigning keeps the rule's lists
    // stable no matter which obligation is read last.
    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == rule_id) else { return };
    let NodeDetail::Rule(detail) = &mut node.detail else { return };
    merge(&mut detail.creates, created);
    merge(&mut detail.emits, emitted);
}

/// Add anything not already present, keeping the list sorted.
fn merge(target: &mut Vec<String>, extra: Vec<String>) {
    target.extend(extra);
    target.sort();
    target.dedup();
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::graph::{Node, RuleDetail, TriggerSource};

    fn graph_with_rule(name: &str) -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        graph.nodes.push(Node::new("lending", NodeKind::Rule, name).with(NodeDetail::Rule(
            RuleDetail {
                trigger: "MemberBorrows".to_owned(),
                source: TriggerSource::External,
                clauses: Vec::new(),
                creates: Vec::new(),
                emits: Vec::new(),
            },
        )));
        graph
    }

    fn rule_detail<'a>(graph: &'a SpecGraph, name: &str) -> &'a RuleDetail {
        graph
            .node(&NodeId::new("lending", NodeKind::Rule, name))
            .and_then(|node| node.detail.as_rule())
            .expect("the rule exists")
    }

    fn success_obligation() -> Value {
        json!({"obligations": [{
            "category": "rule_success",
            "id": "rule-success.BorrowCopy",
            "description": "Verify rule BorrowCopy succeeds when all preconditions are met",
            "source_construct": "BorrowCopy",
            "source_span": {"start": 1899, "end": 2205},
            "dependencies": {
                "entities_created": ["Loan"],
                "entities_read": ["Member"],
                "trigger_emissions": ["CopyBorrowed"],
                "trigger_source": "external",
            },
        }]})
    }

    #[test]
    fn an_obligation_is_recorded_against_its_construct() {
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&success_obligation(), "lending", &mut graph);
        assert_eq!(graph.obligations.len(), 1);
        let obligation = &graph.obligations[0];
        assert_eq!(obligation.id, "rule-success.BorrowCopy");
        assert_eq!(obligation.category, "rule_success");
        assert_eq!(obligation.construct, "BorrowCopy");
        assert_eq!(obligation.module, "lending");
        assert!(obligation.span.is_some());
    }

    #[test]
    fn a_rule_learns_what_it_creates_and_emits() {
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&success_obligation(), "lending", &mut graph);
        let detail = rule_detail(&graph, "BorrowCopy");
        assert_eq!(detail.creates, ["Loan"]);
        assert_eq!(detail.emits, ["CopyBorrowed"]);
    }

    #[test]
    fn a_rules_dependencies_become_the_flow_edges() {
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&success_obligation(), "lending", &mut graph);
        let rule = NodeId::new("lending", NodeKind::Rule, "BorrowCopy");

        let of = |kind: EdgeKind| -> Vec<String> {
            graph
                .edges_from(&rule)
                .filter(|edge| edge.kind == kind)
                .map(|edge| edge.to.to_string())
                .collect()
        };
        assert_eq!(of(EdgeKind::Creates), ["lending::entity::Loan"]);
        assert_eq!(of(EdgeKind::Reads), ["lending::entity::Member"]);
        assert_eq!(of(EdgeKind::Emits), ["lending::trigger::CopyBorrowed"]);
    }

    #[test]
    fn several_obligations_for_one_rule_merge_rather_than_overwrite() {
        // A rule has one success obligation, one failure obligation per
        // precondition and one per entity created, and each repeats the whole
        // dependency block. Assigning instead of merging would make the rule's
        // lists depend on which obligation happened to be read last.
        let document = json!({"obligations": [
            {
                "category": "rule_success",
                "id": "rule-success.BorrowCopy",
                "source_construct": "BorrowCopy",
                "dependencies": {"entities_created": ["Loan"], "trigger_emissions": ["CopyBorrowed"]},
            },
            {
                "category": "rule_entity_creation",
                "id": "rule-entity-creation.BorrowCopy.Receipt",
                "source_construct": "BorrowCopy",
                "dependencies": {"entities_created": ["Receipt"], "trigger_emissions": []},
            },
        ]});
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&document, "lending", &mut graph);
        let detail = rule_detail(&graph, "BorrowCopy");
        assert_eq!(detail.creates, ["Loan", "Receipt"], "merged and sorted");
        assert_eq!(detail.emits, ["CopyBorrowed"]);
    }

    #[test]
    fn a_repeated_dependency_is_recorded_once() {
        let document = json!({"obligations": [
            {"category": "rule_success", "id": "a", "source_construct": "BorrowCopy",
             "dependencies": {"entities_created": ["Loan"]}},
            {"category": "rule_failure", "id": "b", "source_construct": "BorrowCopy",
             "dependencies": {"entities_created": ["Loan"]}},
        ]});
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&document, "lending", &mut graph);
        assert_eq!(rule_detail(&graph, "BorrowCopy").creates, ["Loan"]);
    }

    #[test]
    fn a_non_rule_obligation_is_recorded_but_adds_no_edges() {
        let document = json!({"obligations": [{
            "category": "transition_edge",
            "id": "transition-edge.Loan.open.returned",
            "description": "Verify transition open -> returned on Loan.status is reachable",
            "source_construct": "Loan.status",
            "detail": {"entity": "Loan", "field": "status", "from": "open", "to": "returned"},
        }]});
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&document, "lending", &mut graph);
        assert_eq!(graph.obligations.len(), 1);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn an_obligation_with_no_id_is_dropped() {
        let mut graph = SpecGraph::new("test");
        ingest(&json!({"obligations": [{"category": "rule_success"}]}), "m", &mut graph);
        assert!(graph.obligations.is_empty());
    }

    #[test]
    fn a_rule_obligation_naming_no_construct_adds_no_edges() {
        // An edge from `lending::rule::` would be an arrow out of nothing.
        let document = json!({"obligations": [{
            "category": "rule_success",
            "id": "rule-success.",
            "dependencies": {"entities_created": ["Loan"]},
        }]});
        let mut graph = SpecGraph::new("test");
        ingest(&document, "lending", &mut graph);
        assert!(graph.edges.is_empty());
        assert_eq!(graph.obligations.len(), 1, "the obligation is still listed");
    }

    #[test]
    fn dependencies_for_a_rule_not_in_the_graph_still_produce_edges() {
        // The edge is the useful half and the node it points from is created by
        // the parse pass. Requiring the node first would make ingestion order
        // load-bearing for no benefit; `normalise` sorts it out either way.
        let mut graph = SpecGraph::new("test");
        ingest(&success_obligation(), "lending", &mut graph);
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn an_obligation_with_no_dependencies_block_changes_nothing() {
        let document = json!({"obligations": [
            {"category": "rule_success", "id": "a", "source_construct": "BorrowCopy"},
        ]});
        let mut graph = graph_with_rule("BorrowCopy");
        ingest(&document, "lending", &mut graph);
        assert!(graph.edges.is_empty());
        assert!(rule_detail(&graph, "BorrowCopy").creates.is_empty());
    }

    #[test]
    fn an_empty_document_records_nothing() {
        let mut graph = SpecGraph::new("test");
        ingest(&json!({}), "m", &mut graph);
        assert!(graph.obligations.is_empty());
        assert!(graph.edges.is_empty());
    }
}
