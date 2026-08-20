//! Reading surfaces, actors and invariants out of the `parse` AST.
//!
//! These three are grouped because they are what the spec says about its
//! *edges* and its *limits*, as opposed to its contents:
//!
//! - a **surface** is a boundary — who is on the other side, what they can see,
//!   and what they can do;
//! - an **actor** is who that party is;
//! - an **invariant** is what must remain true no matter which rule ran.
//!
//! Surfaces matter more here than their size suggests. A surface's `provides`
//! list is the only place an Allium spec says which triggers a person can
//! actually fire, which makes it the entry point for the journey view and the
//! list of opening moves in the simulator. Everything else in the flow graph is
//! the system reacting to itself.

use serde_json::Value;

use crate::{
    graph::{
        ActorDetail, Edge, EdgeKind, InvariantDetail, Node, NodeDetail, NodeId, NodeKind,
        SpecGraph, SurfaceDetail, SurfaceOperation,
    },
    ingest::{Ingestion, json, text},
    span::Span,
};

/// Add the surface declared by `block` to `graph`.
pub fn ingest_surface(block: &Value, module: &str, source: &str, into: &mut Ingestion) {
    let Some(name) = json::declared_name(block) else { return };
    let surface_id = NodeId::new(module, NodeKind::Surface, &name);

    let mut detail = SurfaceDetail {
        actor: None,
        actor_binding: None,
        context: None,
        exposes: Vec::new(),
        provides: Vec::new(),
        guarantees: Vec::new(),
    };
    let mut edges = Vec::new();

    for item in json::array(block, "items") {
        let Some(kind) = item.get("kind") else { continue };

        if let Some(annotation) = kind.get("Annotation") {
            // `@guarantee` is a named promise written as prose. It is part of
            // what the boundary claims, so it is shown; nothing can check it.
            if json::string_or_empty(annotation, "kind") == "Guarantee"
                && let Some(name) = json::declared_name(annotation)
            {
                detail.guarantees.push(name);
            }
            continue;
        }

        let Some(clause) = kind.get("Clause") else { continue };
        let value = clause.get("value");
        match json::string_or_empty(clause, "keyword").as_str() {
            "facing" => {
                let (binding, party) = binding_and_type(value);
                detail.actor_binding = binding;
                if let Some(party) = party {
                    // The party may be an actor or an entity used directly. The
                    // edge is drawn to the actor either way and the linker
                    // rewrites it if only an entity of that name exists.
                    edges.push(Edge::new(
                        surface_id.clone(),
                        NodeId::new(module, NodeKind::Actor, &party),
                        EdgeKind::Facing,
                        party.clone(),
                    ));
                    detail.actor = Some(party);
                }
            }
            "context" => detail.context = binding_and_type(value).1,
            "exposes" => detail.exposes.extend(block_item_texts(value, source)),
            "provides" => {
                for operation in provided_operations(value, source) {
                    edges.push(Edge::new(
                        surface_id.clone(),
                        NodeId::new(module, NodeKind::Trigger, &operation.trigger),
                        EdgeKind::Provides,
                        operation.trigger.clone(),
                    ));
                    detail.provides.push(operation);
                }
            }
            _ => {}
        }
    }

    into.graph.edges.extend(edges);
    into.graph.nodes.push(
        Node::new(module, NodeKind::Surface, &name)
            .at(json::span(block, "span"))
            .with(NodeDetail::Surface(detail)),
    );
}

/// Add the actor declared by `block` to `graph`.
pub fn ingest_actor(block: &Value, module: &str, source: &str, into: &mut Ingestion) {
    let Some(name) = json::declared_name(block) else { return };
    let mut detail = ActorDetail { entity: None, condition: None, within: None };

    for item in json::array(block, "items") {
        let Some(clause) = item.get("kind").and_then(|kind| kind.get("Clause")) else { continue };
        let value = clause.get("value");
        match json::string_or_empty(clause, "keyword").as_str() {
            "identified_by" => {
                // `Staff where role = librarian`: the entity is the filter's
                // source and the condition is the rest, kept as written.
                if let Some(("Where", inner)) = value.and_then(json::tagged) {
                    detail.entity = inner.get("source").and_then(json::type_reference);
                    detail.condition =
                        inner.get("condition").and_then(|c| expression_text(c, source));
                } else {
                    detail.entity = value.and_then(json::type_reference);
                }
            }
            "within" => detail.within = value.and_then(json::type_reference),
            _ => {}
        }
    }

    if let Some(entity) = &detail.entity {
        into.graph.edges.push(Edge::new(
            NodeId::new(module, NodeKind::Actor, &name),
            NodeId::new(module, NodeKind::Entity, entity),
            EdgeKind::IdentifiedBy,
            entity.clone(),
        ));
    }

    into.graph.nodes.push(
        Node::new(module, NodeKind::Actor, &name)
            .at(json::span(block, "span"))
            .with(NodeDetail::Actor(detail)),
    );
}

/// Add the invariant declared by `declaration` to `graph`.
pub fn ingest_invariant(declaration: &Value, module: &str, source: &str, into: &mut Ingestion) {
    let Some(name) = json::declared_name(declaration) else { return };
    let body = declaration.get("body");
    if let Some(condition) = body {
        into.program.add_invariant(
            NodeId::new(module, NodeKind::Invariant, &name).as_str(),
            condition.clone(),
        );
    }
    let expression = body.and_then(|body| expression_text(body, source));
    let entities = constrained_entities(body, module, &into.graph);

    for entity in &entities {
        into.graph.edges.push(Edge::new(
            NodeId::new(module, NodeKind::Invariant, &name),
            NodeId::new(module, NodeKind::Entity, entity),
            EdgeKind::Constrains,
            entity.clone(),
        ));
    }

    into.graph.nodes.push(
        Node::new(module, NodeKind::Invariant, &name)
            .at(json::span(declaration, "span"))
            .with(NodeDetail::Invariant(InvariantDetail { expression, entities })),
    );
}

/// The entities an invariant quantifies over.
///
/// `for l in Loans: …` names a collection, and a collection is an entity's name
/// pluralised. Recovering the singular by suffix rules alone is guesswork —
/// `Status` would reduce to `Statu` — so the candidates are checked against the
/// entities this module actually declares, and only a name that matches one is
/// used. The model pass runs before this one, so those nodes are already there.
///
/// A collection matching nothing is dropped rather than guessed at. An edge to
/// an entity that does not exist is worse than no edge: it puts a node on the
/// canvas that the spec never declared.
fn constrained_entities(body: Option<&Value>, module: &str, graph: &SpecGraph) -> Vec<String> {
    let mut collections = Vec::new();
    collect_iterated(body.unwrap_or(&Value::Null), &mut collections);
    let mut entities: Vec<String> = collections
        .iter()
        .filter_map(|collection| resolve_collection(collection, module, graph))
        .collect();
    entities.sort();
    entities.dedup();
    entities
}

/// The entity a collection name refers to, if this module declares one.
///
/// The collection name itself is tried first: an entity whose name is already
/// plural (`Staff`) is written the same way in both positions.
fn resolve_collection(collection: &str, module: &str, graph: &SpecGraph) -> Option<String> {
    [collection.to_owned(), singular(collection)].into_iter().find(|candidate| {
        [NodeKind::Entity, NodeKind::Value, NodeKind::Variant]
            .iter()
            .any(|kind| graph.node(&NodeId::new(module, *kind, candidate)).is_some())
    })
}

fn collect_iterated(value: &Value, out: &mut Vec<String>) {
    if let Some(("For", inner)) = json::tagged(value)
        && let Some(collection) =
            inner.get("collection").or_else(|| inner.get("source")).and_then(json::type_reference)
    {
        out.push(singular(&collection));
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|inner| collect_iterated(inner, out)),
        Value::Array(items) => items.iter().for_each(|inner| collect_iterated(inner, out)),
        _ => {}
    }
}

/// `Loans` -> `Loan`, `Copies` -> `Copy`, `Staff` -> `Staff`.
///
/// A candidate, not an answer: [`resolve_collection`] only uses the result when
/// an entity of that name exists, so over-eager reduction here is harmless.
fn singular(collection: &str) -> String {
    if let Some(stem) = collection.strip_suffix("ies") {
        return format!("{stem}y");
    }
    match collection.strip_suffix('s') {
        Some(stem) if !stem.is_empty() => stem.to_owned(),
        _ => collection.to_owned(),
    }
}

/// The `name: Type` of a `facing` or `context` clause.
fn binding_and_type(value: Option<&Value>) -> (Option<String>, Option<String>) {
    match value.and_then(json::tagged) {
        Some(("Binding", inner)) => {
            (json::declared_name(inner), inner.get("value").and_then(json::type_reference))
        }
        // A `context` clause may narrow with `where`, and a `facing` clause
        // occasionally does too. Both fall through here rather than getting an
        // arm of their own: `type_reference` already unwraps a filter to its
        // source, so a separate arm would be a second copy of that rule waiting
        // to disagree with the first.
        _ => (None, value.and_then(json::type_reference)),
    }
}

/// The source text of each statement in a clause's block.
fn block_item_texts(value: Option<&Value>, source: &str) -> Vec<String> {
    let Some(("Block", block)) = value.and_then(json::tagged) else {
        return value.and_then(|value| expression_text(value, source)).into_iter().collect();
    };
    json::array(block, "items").iter().filter_map(|item| expression_text(item, source)).collect()
}

/// The operations a `provides` clause offers.
fn provided_operations(value: Option<&Value>, source: &str) -> Vec<SurfaceOperation> {
    let Some(("Block", block)) = value.and_then(json::tagged) else { return Vec::new() };
    json::array(block, "items").iter().filter_map(|item| operation(item, source)).collect()
}

fn operation(item: &Value, source: &str) -> Option<SurfaceOperation> {
    // An operation may be guarded: `LibrarianWithdrawsBook(book) when …`.
    let (call, guard) = match json::tagged(item)? {
        ("WhenGuard", inner) => {
            (inner.get("action")?, inner.get("condition").and_then(|c| expression_text(c, source)))
        }
        _ => (item, None),
    };
    let ("Call", inner) = json::tagged(call)? else { return None };
    let trigger = inner
        .get("function")
        .and_then(json::tagged)
        .and_then(|(_, function)| json::string(function, "name"))?;

    let parameters = json::array(inner, "args")
        .iter()
        .filter_map(|argument| match json::tagged(argument)? {
            ("Positional", node) => json::tagged(node).and_then(|(_, n)| json::string(n, "name")),
            ("Named", node) => json::declared_name(node),
            _ => None,
        })
        .collect();

    Some(SurfaceOperation { trigger, parameters, when: guard })
}

/// The source text an expression node covers, comments removed.
fn expression_text(value: &Value, source: &str) -> Option<String> {
    let span = expression_span(value)?;
    span.slice(source).map(text::one_line).filter(|line| !line.is_empty())
}

fn expression_span(value: &Value) -> Option<Span> {
    json::span(value, "span")
        .or_else(|| json::tagged(value).and_then(|(_, inner)| json::span(inner, "span")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn named(name: &str) -> Value {
        json!({"span": {"start": 0, "end": 0}, "name": name})
    }

    fn spanned(tag: &str, start: usize, end: usize, extra: Value) -> Value {
        let mut inner = json!({"span": {"start": start, "end": end}});
        if let (Some(target), Some(source)) = (inner.as_object_mut(), extra.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        json!({ tag: inner })
    }

    /// An ingestion in which `module` declares each of `entities`.
    fn with_entities(module: &str, entities: &[&str]) -> Ingestion {
        let mut into = Ingestion::empty("test");
        for entity in entities {
            into.graph.nodes.push(Node::new(module, NodeKind::Entity, entity));
        }
        into
    }

    #[test]
    fn singular_proposes_the_usual_reductions() {
        assert_eq!(singular("Loans"), "Loan");
        assert_eq!(singular("Members"), "Member");
        assert_eq!(singular("Copies"), "Copy");
        assert_eq!(singular("Books"), "Book");
    }

    #[test]
    fn singular_leaves_a_name_with_no_plural_suffix_alone() {
        assert_eq!(singular("Staff"), "Staff");
        assert_eq!(singular("s"), "s");
        assert_eq!(singular(""), "");
    }

    #[test]
    fn a_collection_resolves_to_the_entity_the_module_declares() {
        let into = with_entities("lending", &["Loan"]);
        assert_eq!(resolve_collection("Loans", "lending", &into.graph).as_deref(), Some("Loan"));
    }

    #[test]
    fn a_collection_whose_own_name_is_an_entity_is_not_reduced() {
        // `Staff` is already plural. Reducing first would look for `Staf`.
        let into = with_entities("catalogue", &["Staff"]);
        assert_eq!(resolve_collection("Staff", "catalogue", &into.graph).as_deref(), Some("Staff"));
    }

    #[test]
    fn a_name_that_matches_no_entity_resolves_to_nothing() {
        // The case suffix rules alone get wrong: `Status` reduces to `Statu`,
        // and an edge to a `Statu` node would put a construct on the canvas
        // that the spec never declared.
        let into = with_entities("m", &["Loan"]);
        assert_eq!(resolve_collection("Status", "m", &into.graph), None);
        assert_eq!(resolve_collection("Statuses", "m", &into.graph), None);
    }

    #[test]
    fn a_collection_resolves_against_value_types_and_variants_too() {
        let mut into = Ingestion::empty("test");
        into.graph.nodes.push(Node::new("m", NodeKind::Value, "LoanWindow"));
        into.graph.nodes.push(Node::new("m", NodeKind::Variant, "SeenAnnouncement"));
        assert_eq!(
            resolve_collection("LoanWindows", "m", &into.graph).as_deref(),
            Some("LoanWindow")
        );
        assert_eq!(
            resolve_collection("SeenAnnouncements", "m", &into.graph).as_deref(),
            Some("SeenAnnouncement")
        );
    }

    #[test]
    fn a_collection_in_another_module_does_not_resolve_here() {
        let into = with_entities("catalogue", &["Book"]);
        assert_eq!(resolve_collection("Books", "lending", &into.graph), None);
    }

    // --- actors ----------------------------------------------------------

    const ACTOR_SOURCE: &str =
        "actor Reader {\n    identified_by: Member where open_loan_count >= 0\n}\n";

    #[test]
    fn an_actor_records_its_entity_and_its_condition() {
        let condition_start =
            ACTOR_SOURCE.find("open_loan_count >= 0").expect("the fixture contains it");
        let block = json!({
            "span": {"start": 0, "end": ACTOR_SOURCE.len()},
            "kind": "Actor",
            "name": named("Reader"),
            "items": [{"kind": {"Clause": {
                "keyword": "identified_by",
                "value": {"Where": {
                    "span": {"start": 0, "end": 0},
                    "source": {"Ident": {"span": {"start": 0, "end": 0}, "name": "Member"}},
                    "condition": {"Comparison": {
                        "span": {"start": condition_start, "end": condition_start + 20},
                    }},
                }},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_actor(&block, "lending", ACTOR_SOURCE, &mut into);

        let node =
            into.graph.node(&NodeId::new("lending", NodeKind::Actor, "Reader")).expect("actor");
        match &node.detail {
            NodeDetail::Actor(detail) => {
                assert_eq!(detail.entity.as_deref(), Some("Member"));
                assert_eq!(detail.condition.as_deref(), Some("open_loan_count >= 0"));
            }
            other => panic!("expected an actor detail, got {other:?}"),
        }
        let edge = into.graph.edges.iter().find(|edge| edge.kind == EdgeKind::IdentifiedBy);
        assert_eq!(
            edge.map(|edge| edge.to.clone()),
            Some(NodeId::new("lending", NodeKind::Entity, "Member"))
        );
    }

    #[test]
    fn an_actor_identified_without_a_condition_still_names_its_entity() {
        let block = json!({
            "kind": "Actor",
            "name": named("Anyone"),
            "items": [{"kind": {"Clause": {
                "keyword": "identified_by",
                "value": {"Ident": {"span": {"start": 0, "end": 0}, "name": "Member"}},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_actor(&block, "m", "", &mut into);
        match &into.graph.nodes[0].detail {
            NodeDetail::Actor(detail) => {
                assert_eq!(detail.entity.as_deref(), Some("Member"));
                assert_eq!(detail.condition, None);
            }
            other => panic!("expected an actor detail, got {other:?}"),
        }
    }

    #[test]
    fn an_actor_records_the_context_type_it_requires() {
        let block = json!({
            "kind": "Actor",
            "name": named("WorkspaceAdmin"),
            "items": [{"kind": {"Clause": {
                "keyword": "within",
                "value": {"Ident": {"span": {"start": 0, "end": 0}, "name": "Workspace"}},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_actor(&block, "m", "", &mut into);
        match &into.graph.nodes[0].detail {
            NodeDetail::Actor(detail) => assert_eq!(detail.within.as_deref(), Some("Workspace")),
            other => panic!("expected an actor detail, got {other:?}"),
        }
    }

    #[test]
    fn an_actor_with_no_entity_draws_no_edge() {
        let block = json!({"kind": "Actor", "name": named("Vague"), "items": []});
        let mut into = Ingestion::empty("test");
        ingest_actor(&block, "m", "", &mut into);
        assert!(into.graph.edges.is_empty());
        assert_eq!(into.graph.nodes.len(), 1);
    }

    #[test]
    fn an_unnamed_actor_is_dropped() {
        let mut into = Ingestion::empty("test");
        ingest_actor(&json!({"kind": "Actor", "items": []}), "m", "", &mut into);
        assert!(into.graph.nodes.is_empty());
    }

    // --- surfaces --------------------------------------------------------

    const SURFACE_SOURCE: &str = "surface MemberShelf {\n    facing reader: Reader\n    provides:\n        MemberBorrows(reader, copy)\n}\n";

    fn surface_block() -> Value {
        let call_start = SURFACE_SOURCE.find("MemberBorrows(reader, copy)").expect("present");
        json!({
            "span": {"start": 0, "end": SURFACE_SOURCE.len()},
            "kind": "Surface",
            "name": named("MemberShelf"),
            "items": [
                {"kind": {"Clause": {
                    "keyword": "facing",
                    "value": {"Binding": {
                        "span": {"start": 0, "end": 0},
                        "name": named("reader"),
                        "value": {"Ident": {"span": {"start": 0, "end": 0}, "name": "Reader"}},
                    }},
                }}},
                {"kind": {"Clause": {
                    "keyword": "provides",
                    "value": {"Block": {
                        "span": {"start": 0, "end": 0},
                        "items": [{"Call": {
                            "span": {"start": call_start, "end": call_start + 27},
                            "function": {"Ident": {"span": {"start": 0, "end": 0}, "name": "MemberBorrows"}},
                            "args": [
                                {"Positional": {"Ident": {"span": {"start": 0, "end": 0}, "name": "reader"}}},
                                {"Positional": {"Ident": {"span": {"start": 0, "end": 0}, "name": "copy"}}},
                            ],
                        }}],
                    }},
                }}},
                {"kind": {"Annotation": {
                    "kind": "Guarantee",
                    "name": named("ALoanIsVisibleToItsHolderOnly"),
                    "body": ["prose"],
                    "span": {"start": 0, "end": 0},
                }}},
            ],
        })
    }

    fn surface_graph() -> Ingestion {
        let mut into = Ingestion::empty("test");
        ingest_surface(&surface_block(), "lending", SURFACE_SOURCE, &mut into);
        into
    }

    #[test]
    fn a_surface_records_who_it_faces_and_under_what_binding() {
        let into = surface_graph();
        let node = into
            .graph
            .node(&NodeId::new("lending", NodeKind::Surface, "MemberShelf"))
            .expect("surface");
        let detail = node.detail.as_surface().expect("a surface detail");
        assert_eq!(detail.actor.as_deref(), Some("Reader"));
        assert_eq!(detail.actor_binding.as_deref(), Some("reader"));
    }

    #[test]
    fn a_surface_records_the_operations_it_provides() {
        // The only place a spec says which triggers a person can actually
        // fire, so this list is the simulator's opening moves.
        let into = surface_graph();
        let detail = into
            .graph
            .node(&NodeId::new("lending", NodeKind::Surface, "MemberShelf"))
            .and_then(|node| node.detail.as_surface())
            .expect("the surface");
        assert_eq!(detail.provides.len(), 1);
        assert_eq!(detail.provides[0].trigger, "MemberBorrows");
        assert_eq!(detail.provides[0].parameters, ["reader", "copy"]);
        assert_eq!(detail.provides[0].when, None);
    }

    #[test]
    fn a_surface_draws_edges_to_its_actor_and_its_triggers() {
        let into = surface_graph();
        let surface = NodeId::new("lending", NodeKind::Surface, "MemberShelf");
        let facing: Vec<_> =
            into.graph.edges_from(&surface).filter(|e| e.kind == EdgeKind::Facing).collect();
        assert_eq!(facing.len(), 1);
        assert_eq!(facing[0].to, NodeId::new("lending", NodeKind::Actor, "Reader"));

        let provides: Vec<_> =
            into.graph.edges_from(&surface).filter(|e| e.kind == EdgeKind::Provides).collect();
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].to, NodeId::new("lending", NodeKind::Trigger, "MemberBorrows"));
    }

    #[test]
    fn a_surface_records_its_named_guarantees() {
        let into = surface_graph();
        let detail = into
            .graph
            .node(&NodeId::new("lending", NodeKind::Surface, "MemberShelf"))
            .and_then(|node| node.detail.as_surface())
            .expect("the surface");
        assert_eq!(detail.guarantees, ["ALoanIsVisibleToItsHolderOnly"]);
    }

    #[test]
    fn a_guidance_annotation_is_not_recorded_as_a_guarantee() {
        // `@guidance` is advice to an implementer, not a promise to the party
        // on the other side of the boundary.
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Annotation": {"kind": "Guidance", "body": ["advice"]}}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", "", &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert!(detail.guarantees.is_empty());
    }

    #[test]
    fn a_guarded_operation_keeps_its_condition() {
        let source = "LibrarianWithdrawsBook(book) when book.status = listed";
        let guard_start = source.find("book.status = listed").expect("present");
        let block = json!({
            "kind": "Surface",
            "name": named("Desk"),
            "items": [{"kind": {"Clause": {
                "keyword": "provides",
                "value": {"Block": {"items": [{"WhenGuard": {
                    "span": {"start": 0, "end": 0},
                    "action": {"Call": {
                        "span": {"start": 0, "end": 28},
                        "function": {"Ident": {"name": "LibrarianWithdrawsBook"}},
                        "args": [{"Positional": {"Ident": {"name": "book"}}}],
                    }},
                    "condition": {"Comparison": {"span": {"start": guard_start, "end": source.len()}}},
                }}]}},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", source, &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert_eq!(detail.provides[0].trigger, "LibrarianWithdrawsBook");
        assert_eq!(detail.provides[0].when.as_deref(), Some("book.status = listed"));
    }

    #[test]
    fn a_surface_records_the_entity_it_is_scoped_to() {
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Clause": {
                "keyword": "context",
                "value": {"Binding": {
                    "name": named("loan"),
                    "value": {"Ident": {"name": "Loan"}},
                }},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", "", &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert_eq!(detail.context.as_deref(), Some("Loan"));
    }

    #[test]
    fn a_narrowed_context_still_names_its_entity() {
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Clause": {
                "keyword": "context",
                "value": {"Where": {
                    "source": {"Ident": {"name": "Loan"}},
                    "condition": {"Ident": {"name": "x"}},
                }},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", "", &mut into);
        assert_eq!(
            into.graph.nodes[0].detail.as_surface().expect("a surface").context.as_deref(),
            Some("Loan")
        );
    }

    #[test]
    fn a_surface_exposing_fields_records_them_as_written() {
        let source = "Loan.status\nMember.open_loan_count";
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Clause": {
                "keyword": "exposes",
                "value": {"Block": {"items": [
                    spanned("MemberAccess", 0, 11, json!({})),
                    spanned("MemberAccess", 12, 34, json!({})),
                ]}},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", source, &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert_eq!(detail.exposes, ["Loan.status", "Member.open_loan_count"]);
    }

    #[test]
    fn an_unnamed_surface_is_dropped() {
        let mut into = Ingestion::empty("test");
        ingest_surface(&json!({"kind": "Surface", "items": []}), "m", "", &mut into);
        assert!(into.graph.nodes.is_empty());
    }

    // --- invariants -------------------------------------------------------

    #[test]
    fn an_invariant_records_its_expression_and_what_it_constrains() {
        let source =
            "for l in Loans:\n        l.status = returned implies l.copy.status != on_loan";
        let block = json!({
            "span": {"start": 0, "end": source.len()},
            "name": named("AReturnedLoanFreesItsCopy"),
            "body": {"For": {
                "span": {"start": 0, "end": source.len()},
                "binding": named("l"),
                "collection": {"Ident": {"span": {"start": 9, "end": 14}, "name": "Loans"}},
            }},
        });
        let mut into = with_entities("lending", &["Loan"]);
        ingest_invariant(&block, "lending", source, &mut into);

        let node = into
            .graph
            .node(&NodeId::new("lending", NodeKind::Invariant, "AReturnedLoanFreesItsCopy"))
            .expect("the invariant");
        match &node.detail {
            NodeDetail::Invariant(detail) => {
                assert!(detail.is_checkable());
                assert!(detail.expression.as_deref().expect("text").starts_with("for l in Loans:"));
                assert_eq!(detail.entities, ["Loan"], "the collection is reduced to its entity");
            }
            other => panic!("expected an invariant detail, got {other:?}"),
        }

        let edge = into.graph.edges.iter().find(|edge| edge.kind == EdgeKind::Constrains);
        assert_eq!(
            edge.map(|edge| edge.to.clone()),
            Some(NodeId::new("lending", NodeKind::Entity, "Loan"))
        );
    }

    #[test]
    fn a_prose_only_invariant_is_kept_and_marked_uncheckable() {
        let mut into = Ingestion::empty("test");
        ingest_invariant(&json!({"name": named("SomethingProse")}), "m", "", &mut into);
        match &into.graph.nodes[0].detail {
            NodeDetail::Invariant(detail) => {
                assert!(!detail.is_checkable());
                assert!(detail.entities.is_empty());
            }
            other => panic!("expected an invariant detail, got {other:?}"),
        }
        assert!(into.graph.edges.is_empty(), "nothing to constrain");
    }

    #[test]
    fn an_invariant_over_two_collections_constrains_both_once_each() {
        let source = "for l in Loans: for m in Members: true";
        let block = json!({
            "name": named("Both"),
            "body": {"For": {
                "span": {"start": 0, "end": source.len()},
                "collection": {"Ident": {"span": {"start": 9, "end": 14}, "name": "Loans"}},
                "body": {"For": {
                    "span": {"start": 16, "end": source.len()},
                    "collection": {"Ident": {"span": {"start": 24, "end": 31}, "name": "Members"}},
                }},
            }},
        });
        let mut into = with_entities("m", &["Loan", "Member"]);
        ingest_invariant(&block, "m", source, &mut into);
        let invariant =
            into.graph.node(&NodeId::new("m", NodeKind::Invariant, "Both")).expect("the invariant");
        match &invariant.detail {
            NodeDetail::Invariant(detail) => assert_eq!(detail.entities, ["Loan", "Member"]),
            other => panic!("expected an invariant detail, got {other:?}"),
        }
        assert_eq!(into.graph.edges.len(), 2);
    }

    #[test]
    fn an_invariant_over_an_unrecognised_collection_draws_no_edge() {
        let source = "for s in Statuses: true";
        let block = json!({
            "name": named("Odd"),
            "body": {"For": {
                "span": {"start": 0, "end": source.len()},
                "collection": {"Ident": {"span": {"start": 9, "end": 17}, "name": "Statuses"}},
            }},
        });
        let mut into = with_entities("m", &["Loan"]);
        ingest_invariant(&block, "m", source, &mut into);
        let invariant =
            into.graph.node(&NodeId::new("m", NodeKind::Invariant, "Odd")).expect("the invariant");
        match &invariant.detail {
            NodeDetail::Invariant(detail) => {
                assert!(detail.entities.is_empty());
                assert!(detail.is_checkable(), "the expression is still shown");
            }
            other => panic!("expected an invariant detail, got {other:?}"),
        }
        assert!(into.graph.edges.is_empty());
    }

    #[test]
    fn an_invariant_whose_iteration_sits_inside_a_list_is_still_found() {
        // The AST nests `for` clauses inside arrays of block items, so a walk
        // that only descends through objects finds nothing in a multi-statement
        // invariant — the common shape, not the exotic one.
        let source = "for l in Loans: true";
        let block = json!({
            "name": named("InAList"),
            "body": {"Block": {
                "span": {"start": 0, "end": source.len()},
                "items": [{"For": {
                    "span": {"start": 0, "end": source.len()},
                    "collection": {"Ident": {"span": {"start": 9, "end": 14}, "name": "Loans"}},
                }}],
            }},
        });
        let mut into = with_entities("m", &["Loan"]);
        ingest_invariant(&block, "m", source, &mut into);
        let invariant =
            into.graph.node(&NodeId::new("m", NodeKind::Invariant, "InAList")).expect("invariant");
        match &invariant.detail {
            NodeDetail::Invariant(detail) => assert_eq!(detail.entities, ["Loan"]),
            other => panic!("expected an invariant detail, got {other:?}"),
        }
    }

    #[test]
    fn an_operation_taking_a_named_argument_records_its_name() {
        // Emissions and some operations pass `name: value` rather than a bare
        // identifier. Skipping those would show an operation as taking no
        // parameters, which is a different operation.
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Clause": {
                "keyword": "provides",
                "value": {"Block": {"items": [{"Call": {
                    "span": {"start": 0, "end": 0},
                    "function": {"Ident": {"name": "Report"}},
                    "args": [
                        {"Named": {"name": named("loan"), "value": {"Ident": {"name": "l"}}}},
                        {"Positional": {"Ident": {"name": "reader"}}},
                    ],
                }}]}},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", "", &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert_eq!(detail.provides[0].parameters, ["loan", "reader"]);
    }

    #[test]
    fn a_facing_clause_narrowed_by_where_still_names_its_party() {
        let block = json!({
            "kind": "Surface",
            "name": named("S"),
            "items": [{"kind": {"Clause": {
                "keyword": "facing",
                "value": {"Where": {
                    "source": {"Ident": {"name": "Member"}},
                    "condition": {"Ident": {"name": "active"}},
                }},
            }}}],
        });
        let mut into = Ingestion::empty("test");
        ingest_surface(&block, "m", "", &mut into);
        let detail = into.graph.nodes[0].detail.as_surface().expect("a surface");
        assert_eq!(detail.actor.as_deref(), Some("Member"));
        assert_eq!(detail.actor_binding, None, "a filter binds no name");
    }

    #[test]
    fn an_unnamed_invariant_is_dropped() {
        let mut into = Ingestion::empty("test");
        ingest_invariant(&json!({"body": {}}), "m", "", &mut into);
        assert!(into.graph.nodes.is_empty());
    }
}
