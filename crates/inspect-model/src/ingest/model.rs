//! Reading `allium model`: entities, enums and configuration.
//!
//! This is the structural half of the graph. `model` answers "what does this
//! spec hold?" — every entity with its fields, relationships, projections,
//! derived values and lifecycle, plus the enumerations and the config block.
//!
//! What it does not answer is *where*: the document carries no spans at all.
//! Entity nodes are therefore created here without one and given theirs later,
//! from `parse`, which is the only command that reports position.
//!
//! One shape is worth knowing before reading this. `fields` already contains
//! the derived values and projections, typed by the expression that computes
//! them (`copy_count: copies.count`), while `derived_values` and `projections`
//! list their names again separately. Reading only the latter would drop the
//! expression; reading only the former would leave a computed field looking
//! stored. Both are read, and the second pass marks what the first created.

use serde_json::Value;

use crate::{
    graph::{
        ConfigDetail, ConfigParameter, Edge, EdgeKind, EntityDetail, EntityField, EntityKind,
        EnumDetail, Node, NodeDetail, NodeId, NodeKind, SpecGraph, TransitionEdge, TransitionGraph,
    },
    ingest::json,
};

/// Add everything `allium model` reports for `module` to `graph`.
pub fn ingest(document: &Value, module: &str, graph: &mut SpecGraph) {
    for entity in json::array(document, "entities") {
        ingest_entity(entity, module, graph);
    }
    for enumeration in json::array(document, "enums") {
        ingest_enum(enumeration, module, graph);
    }
    ingest_config(document, module, graph);
}

fn ingest_entity(value: &Value, module: &str, graph: &mut SpecGraph) {
    let Some(name) = json::string(value, "name") else { return };

    let mut fields: Vec<EntityField> = json::array(value, "fields")
        .iter()
        .filter_map(|field| {
            let field_name = json::string(field, "name")?;
            let mut parsed =
                EntityField::new(field_name, json::string_or_empty(field, "type_expr"));
            parsed.enum_values = json::strings(field, "enum_values");
            Some(parsed)
        })
        .collect();

    // Relationships and projections are fields too, and the UI draws them in
    // the same list. They are reported separately because they are *navigable*
    // in a way a stored field is not, which is exactly what the flags record.
    for relationship in json::array(value, "relationships") {
        let Some(field_name) = json::string(relationship, "name") else { continue };
        // `model` reads one file, so a relationship crossing a module boundary
        // comes back as the literal target `unknown`. That is the CLI reporting
        // a limit of its own scope, not a type of that name — and resolving it
        // is the whole reason this tool reads the spec set as a set. The name is
        // dropped here and the parse pass supplies the real one.
        let target = match json::string_or_empty(relationship, "target").as_str() {
            "unknown" | "" => String::new(),
            named => named.to_owned(),
        };
        let mut field = EntityField::new(field_name, target.clone());
        field.relationship = true;
        upsert_field(&mut fields, field);

        if !target.is_empty() {
            graph.edges.push(Edge::new(
                NodeId::new(module, NodeKind::Entity, &name),
                NodeId::new(module, NodeKind::Entity, &target),
                EdgeKind::Relationship,
                json::string_or_empty(relationship, "name"),
            ));
        }
    }

    for projection in json::array(value, "projections") {
        let Some(field_name) = json::string(projection, "name") else { continue };
        let mut field = EntityField::new(field_name, json::string_or_empty(projection, "source"));
        field.relationship = true;
        field.derived = true;
        upsert_field(&mut fields, field);
    }

    // `model` reports `derived_values` inconsistently: `Member.is_at_limit` is
    // listed, `Book.copy_count` is not, and both are computed. What separates
    // them in the document is the type expression — a derived field is typed by
    // the expression that produces it, `copies.count`, which names no type. So
    // the shape is what decides, and the explicit list below only confirms it.
    for field in &mut fields {
        if !field.relationship && !field.is_status() && !names_a_type(&field.type_expr) {
            field.derived = true;
        }
    }

    // Marks, rather than inserts: `fields` already carries these with the
    // expression that computes them, and `derived_values` reports only names.
    for derived in json::array(value, "derived_values") {
        let Some(field_name) = json::string(derived, "name") else { continue };
        match fields.iter_mut().find(|field| field.name == field_name) {
            Some(field) => field.derived = true,
            None => {
                let mut field = EntityField::new(field_name, String::new());
                field.derived = true;
                fields.push(field);
            }
        }
    }

    let transitions: Vec<TransitionGraph> = json::array(value, "transition_graphs")
        .iter()
        .map(|graph| TransitionGraph {
            field: json::string_or_empty(graph, "field"),
            states: json::strings(graph, "states"),
            edges: json::array(graph, "edges")
                .iter()
                .filter_map(|edge| {
                    Some(TransitionEdge {
                        from: json::string(edge, "from")?,
                        to: json::string(edge, "to")?,
                    })
                })
                .collect(),
            terminal: json::strings(graph, "terminal"),
        })
        .collect();

    let kind = match json::string_or_empty(value, "kind").as_str() {
        "external" => EntityKind::External,
        "value" => EntityKind::Value,
        _ => EntityKind::Internal,
    };
    let node_kind = if kind == EntityKind::Value { NodeKind::Value } else { NodeKind::Entity };

    graph.nodes.push(Node::new(module, node_kind, &name).with(NodeDetail::Entity(EntityDetail {
        kind,
        fields,
        transitions,
        parent: None,
    })));
}

/// Whether a type expression names a type rather than computing a value.
///
/// A stored field is typed `String`, `Timestamp` or `catalogue/Copy`; a derived
/// one is typed by its expression — `copies.count`, `attachment != null`,
/// `receipts where kind = read -> reporter`. Only a bare, capitalised,
/// optionally qualified name is a type, and only a type belongs in the resolver.
fn names_a_type(type_expr: &str) -> bool {
    let trimmed = type_expr.trim_end_matches('?').trim();
    let inner = match (trimmed.find('<'), trimmed.strip_suffix('>')) {
        (Some(open), Some(_)) => trimmed.get(open + 1..trimmed.len() - 1).unwrap_or("").trim(),
        _ => trimmed,
    };
    if inner.is_empty() {
        // No type expression at all. Not evidence either way, and calling it
        // derived would mark every field the CLI declined to type.
        return true;
    }
    let bare = inner.rsplit('/').next().unwrap_or(inner);
    inner.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '/')
        && inner.matches('/').count() <= 1
        && bare.chars().next().is_some_and(|first| first.is_ascii_uppercase())
}

/// Replace the field of the same name, or append it.
///
/// The same name can arrive twice — once from `fields` and once from
/// `relationships` — and the second arrival is the one that knows it navigates.
/// Appending both would show the field twice in the inspector.
fn upsert_field(fields: &mut Vec<EntityField>, field: EntityField) {
    match fields.iter_mut().find(|existing| existing.name == field.name) {
        Some(existing) => {
            // Keep the richer type expression: `fields` types a relationship by
            // its target entity, which is what the UI wants to show.
            let type_expr = if field.type_expr.is_empty() {
                existing.type_expr.clone()
            } else {
                field.type_expr.clone()
            };
            *existing = EntityField { type_expr, ..field };
        }
        None => fields.push(field),
    }
}

fn ingest_enum(value: &Value, module: &str, graph: &mut SpecGraph) {
    let Some(name) = json::string(value, "name") else { return };
    graph.nodes.push(
        Node::new(module, NodeKind::Enum, &name)
            .with(NodeDetail::Enum(EnumDetail { values: json::strings(value, "values") })),
    );
}

fn ingest_config(document: &Value, module: &str, graph: &mut SpecGraph) {
    let parameters: Vec<ConfigParameter> = json::array(document, "config")
        .iter()
        .filter_map(|parameter| {
            Some(ConfigParameter {
                name: json::string(parameter, "name")?,
                type_expr: json::string_or_empty(parameter, "type_expr"),
                default_expr: json::string(parameter, "default_expr"),
            })
        })
        .collect();

    // A module with no config block gets no node. An empty one would render as
    // an empty box in every view, which is noise rather than information.
    if parameters.is_empty() {
        return;
    }
    graph.nodes.push(
        Node::new(module, NodeKind::Config, "config")
            .with(NodeDetail::Config(ConfigDetail { parameters })),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ingested(document: &Value) -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        ingest(document, "catalogue", &mut graph);
        graph
    }

    fn entity<'a>(graph: &'a SpecGraph, name: &str) -> &'a EntityDetail {
        graph
            .node(&NodeId::new("catalogue", NodeKind::Entity, name))
            .and_then(|node| node.detail.as_entity())
            .unwrap_or_else(|| panic!("entity {name} was ingested"))
    }

    // `names_a_type` is what decides whether a field is stored or derived, and
    // therefore whether the linker will try to resolve it. Getting it wrong in
    // one direction litters the canvas with nodes for expressions; in the other
    // it silently drops real relationships. Tested as a table because every
    // case below came from a real spec.
    #[test]
    fn a_bare_capitalised_name_is_a_type() {
        assert!(names_a_type("String"));
        assert!(names_a_type("Timestamp"));
        assert!(names_a_type("Copy"));
    }

    #[test]
    fn a_qualified_name_is_a_type() {
        assert!(names_a_type("catalogue/Copy"));
    }

    #[test]
    fn a_container_is_a_type_and_is_read_through_to_its_argument() {
        // The arithmetic here slices between the angle brackets. Off by one in
        // either direction and `Set<Book>` reads as `Set<Book` or `Book>`,
        // neither of which is a name.
        assert!(names_a_type("Set<Book>"));
        assert!(names_a_type("Set<catalogue/Book>"));
        assert!(!names_a_type("Set<copies.count>"));
    }

    #[test]
    fn an_optional_type_is_still_a_type() {
        assert!(names_a_type("Attachment?"));
    }

    #[test]
    fn a_lowercase_name_is_an_expression_not_a_type() {
        // Types are PascalCase in Allium. A bare lowercase word is a field
        // being projected.
        assert!(!names_a_type("loans"));
        assert!(!names_a_type("open_loans"));
    }

    #[test]
    fn a_computed_expression_is_not_a_type() {
        assert!(!names_a_type("copies.count"));
        assert!(!names_a_type("attachment != null"));
        assert!(!names_a_type("receipts where kind = read -> reporter"));
        assert!(!names_a_type("delivered_to.count"));
    }

    #[test]
    fn a_capitalised_name_holding_punctuation_is_not_a_type() {
        // The character test has to accept `_` specifically rather than reject
        // one particular character: `A-B` is capitalised and unqualified, so
        // this is the only check standing between it and being resolved as a
        // type nothing declares.
        assert!(!names_a_type("A-B"));
        assert!(!names_a_type("A B"));
        assert!(!names_a_type("A.B"));
        assert!(names_a_type("A_B"), "an underscore is part of a name");
    }

    #[test]
    fn a_doubly_qualified_name_is_not_a_type() {
        // Allium namespaces are one level deep; two separators is not a name
        // this crate can resolve, and guessing would point at nothing.
        assert!(!names_a_type("a/b/C"));
    }

    #[test]
    fn a_qualified_name_whose_last_segment_is_lowercase_is_not_a_type() {
        // The capitalisation test has to look at the segment after the slash:
        // the part before it is the module alias, which is lowercase.
        assert!(!names_a_type("catalogue/copies"));
    }

    #[test]
    fn a_missing_type_expression_is_not_evidence_of_being_derived() {
        // Calling an untyped field derived would mark every field the CLI
        // declined to type, which is most of them in a spec that fails to parse.
        assert!(names_a_type(""));
        assert!(names_a_type("   "));
        assert!(names_a_type("Set<>"));
    }

    #[test]
    fn a_relationships_target_type_wins_over_a_blank_one() {
        // `fields` types a relationship by its target entity and the
        // `relationships` entry may not; keeping the richer expression is what
        // puts `Copy` rather than nothing in the inspector.
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [{"name": "copies", "type_expr": "Copy"}],
            "relationships": [{"name": "copies"}],
        }]}));
        let field = entity(&graph, "Book").field("copies").expect("the field");
        assert_eq!(field.type_expr, "Copy", "the type survives the merge");
        assert!(field.relationship, "and it is known to navigate");
    }

    #[test]
    fn an_entity_becomes_a_node_with_its_fields() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "kind": "internal",
            "fields": [
                {"name": "title", "type_expr": "String"},
                {"name": "added_at", "type_expr": "Timestamp"},
            ],
        }]}));
        let detail = entity(&graph, "Book");
        assert_eq!(detail.kind, EntityKind::Internal);
        assert_eq!(detail.fields.len(), 2);
        assert_eq!(detail.field("title").map(|f| f.type_expr.as_str()), Some("String"));
    }

    #[test]
    fn an_inline_status_keeps_its_values() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [{
                "name": "status",
                "type_expr": "listed | withdrawn",
                "enum_values": ["listed", "withdrawn"],
            }],
        }]}));
        let field = entity(&graph, "Book").field("status").expect("status exists");
        assert!(field.is_status());
        assert_eq!(field.enum_values, ["listed", "withdrawn"]);
    }

    #[test]
    fn a_relationship_becomes_a_navigable_field_and_an_edge() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [],
            "relationships": [{"name": "copies", "target": "Copy"}],
        }]}));
        let field = entity(&graph, "Book").field("copies").expect("the relationship is a field");
        assert!(field.relationship);
        assert_eq!(field.type_expr, "Copy");

        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.kind, EdgeKind::Relationship);
        assert_eq!(edge.label, "copies");
        assert_eq!(edge.to, NodeId::new("catalogue", NodeKind::Entity, "Copy"));
    }

    #[test]
    fn a_relationship_with_no_target_makes_no_edge() {
        // An edge to `catalogue::entity::` would be an arrow into nothing.
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "relationships": [{"name": "copies"}],
        }]}));
        assert!(graph.edges.is_empty());
        assert!(entity(&graph, "Book").field("copies").is_some(), "the field still exists");
    }

    #[test]
    fn a_projection_is_both_navigable_and_derived() {
        let graph = ingested(&json!({"entities": [{
            "name": "Member",
            "projections": [{"name": "open_loans", "source": "loans"}],
        }]}));
        let field = entity(&graph, "Member").field("open_loans").expect("the projection");
        assert!(field.relationship, "a projection navigates");
        assert!(field.derived, "and is computed rather than stored");
        assert_eq!(field.type_expr, "loans");
    }

    #[test]
    fn a_derived_value_marks_the_field_that_already_carries_its_expression() {
        // The whole reason both lists are read. `fields` has the expression,
        // `derived_values` has the fact that it is derived; either alone loses
        // something.
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [{"name": "copy_count", "type_expr": "copies.count"}],
            "derived_values": [{"name": "copy_count"}],
        }]}));
        let field = entity(&graph, "Book").field("copy_count").expect("the derived value");
        assert!(field.derived);
        assert_eq!(field.type_expr, "copies.count", "the expression survives");
        assert_eq!(entity(&graph, "Book").fields.len(), 1, "and it is not listed twice");
    }

    #[test]
    fn a_derived_value_with_no_matching_field_is_still_recorded() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "derived_values": [{"name": "orphan"}],
        }]}));
        let field = entity(&graph, "Book").field("orphan").expect("recorded anyway");
        assert!(field.derived);
        assert_eq!(field.type_expr, "");
    }

    #[test]
    fn a_field_named_by_two_lists_appears_once() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [{"name": "copies", "type_expr": "Copy"}],
            "relationships": [{"name": "copies", "target": "Copy"}],
        }]}));
        assert_eq!(entity(&graph, "Book").fields.len(), 1);
        assert!(entity(&graph, "Book").field("copies").expect("present").relationship);
    }

    #[test]
    fn a_transition_graph_is_read_whole() {
        let graph = ingested(&json!({"entities": [{
            "name": "Copy",
            "transition_graphs": [{
                "field": "status",
                "states": ["available", "lost", "on_loan"],
                "edges": [
                    {"from": "available", "to": "on_loan"},
                    {"from": "on_loan", "to": "lost"},
                ],
                "terminal": ["lost"],
            }],
        }]}));
        let lifecycle =
            entity(&graph, "Copy").transitions_for("status").expect("the lifecycle exists");
        assert_eq!(lifecycle.states.len(), 3);
        assert!(lifecycle.allows("available", "on_loan"));
        assert!(!lifecycle.allows("available", "lost"));
        assert!(lifecycle.is_terminal("lost"));
    }

    #[test]
    fn a_transition_edge_missing_an_end_is_dropped_not_half_built() {
        let graph = ingested(&json!({"entities": [{
            "name": "Copy",
            "transition_graphs": [{
                "field": "status",
                "edges": [{"from": "available"}, {"from": "a", "to": "b"}],
            }],
        }]}));
        let lifecycle = entity(&graph, "Copy").transitions_for("status").expect("present");
        assert_eq!(lifecycle.edges.len(), 1, "the half-built edge is dropped");
        assert!(lifecycle.allows("a", "b"));
    }

    #[test]
    fn an_external_entity_is_marked_as_governed_elsewhere() {
        let graph = ingested(&json!({"entities": [{"name": "Staff", "kind": "external"}]}));
        assert_eq!(entity(&graph, "Staff").kind, EntityKind::External);
    }

    #[test]
    fn a_value_type_becomes_a_value_node_not_an_entity() {
        let graph = ingested(&json!({"entities": [{"name": "LoanWindow", "kind": "value"}]}));
        let node = graph
            .node(&NodeId::new("catalogue", NodeKind::Value, "LoanWindow"))
            .expect("a value node");
        assert_eq!(node.kind, NodeKind::Value);
        assert_eq!(node.detail.as_entity().map(|d| d.kind), Some(EntityKind::Value));
    }

    #[test]
    fn an_unrecognised_entity_kind_is_treated_as_internal() {
        let graph = ingested(&json!({"entities": [{"name": "Book", "kind": "something_new"}]}));
        assert_eq!(entity(&graph, "Book").kind, EntityKind::Internal);
    }

    #[test]
    fn an_entity_with_no_name_is_dropped() {
        let graph = ingested(&json!({"entities": [{"kind": "internal"}, {"name": "Book"}]}));
        assert_eq!(graph.nodes.len(), 1);
    }

    #[test]
    fn a_field_with_no_name_is_dropped_but_its_siblings_survive() {
        let graph = ingested(&json!({"entities": [{
            "name": "Book",
            "fields": [{"type_expr": "String"}, {"name": "title", "type_expr": "String"}],
        }]}));
        assert_eq!(entity(&graph, "Book").fields.len(), 1);
    }

    #[test]
    fn an_enum_becomes_a_node_with_its_values() {
        let graph = ingested(&json!({"enums": [{"name": "Medium", "values": ["print", "audio"]}]}));
        let node =
            graph.node(&NodeId::new("catalogue", NodeKind::Enum, "Medium")).expect("the enum");
        match &node.detail {
            NodeDetail::Enum(detail) => assert_eq!(detail.values, ["print", "audio"]),
            other => panic!("expected an enum detail, got {other:?}"),
        }
    }

    #[test]
    fn config_becomes_one_node_holding_every_parameter() {
        let graph = ingested(&json!({"config": [
            {"name": "loan_limit", "type_expr": "Integer", "default_expr": "5"},
            {"name": "loan_period", "type_expr": "Duration", "default_expr": "21.days"},
        ]}));
        let node =
            graph.node(&NodeId::new("catalogue", NodeKind::Config, "config")).expect("config");
        match &node.detail {
            NodeDetail::Config(detail) => {
                assert_eq!(detail.parameters.len(), 2);
                assert_eq!(detail.parameters[1].default_expr.as_deref(), Some("21.days"));
            }
            other => panic!("expected a config detail, got {other:?}"),
        }
    }

    #[test]
    fn a_config_parameter_with_no_default_keeps_that_distinction() {
        let graph = ingested(&json!({"config": [{"name": "limit", "type_expr": "Integer"}]}));
        let node = graph.node(&NodeId::new("catalogue", NodeKind::Config, "config")).expect("cfg");
        match &node.detail {
            NodeDetail::Config(detail) => assert_eq!(detail.parameters[0].default_expr, None),
            other => panic!("expected a config detail, got {other:?}"),
        }
    }

    #[test]
    fn a_module_with_no_config_gets_no_config_node() {
        // An empty config box would appear in every view of every module that
        // does not use config, which is noise rather than information.
        let graph = ingested(&json!({"entities": [{"name": "Book"}]}));
        assert!(graph.node(&NodeId::new("catalogue", NodeKind::Config, "config")).is_none());
    }

    #[test]
    fn an_empty_document_produces_an_empty_graph() {
        let graph = ingested(&json!({}));
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn entity_nodes_carry_no_span_because_this_document_has_none() {
        // Stated as a test so that if `model` ever starts reporting spans, this
        // fails and says where to read them from instead of leaving `parse` as
        // the silent-but-only source.
        let graph = ingested(&json!({"entities": [{"name": "Book"}]}));
        assert_eq!(graph.nodes[0].span, None);
    }
}
