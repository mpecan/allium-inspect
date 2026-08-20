//! Walking `allium parse`: the declarations `model` does not report, and the
//! spans nothing else reports.
//!
//! `model` describes entities well and says nothing about where they are, nor
//! anything at all about rules, surfaces, actors, invariants, value types or
//! imports. This walk supplies all of that. It runs *after* the model pass, so
//! an entity already in the graph is given its span rather than added twice.
//!
//! The declaration vocabulary is small and closed:
//!
//! ```text
//! Block { kind: Entity | Value | Rule | Surface | Actor | Enum | Config }
//! Invariant | Variant | Use | OpenQuestion
//! ```
//!
//! Anything outside it is skipped rather than guessed at. A declaration kind
//! added in a later CLI release should leave the rest of the graph intact.

use serde_json::Value;

use crate::{
    graph::{Import, Module, Node, NodeDetail, NodeId, NodeKind, SpecGraph},
    ingest::{Ingestion, json, prose, rules, surfaces},
    span::Span,
};

/// Add everything `allium parse` reports for `module` to `graph`.
///
/// `source` is the spec file's text, which clause and expression rendering
/// slices out of rather than reconstructing.
pub fn ingest(document: &Value, module: &str, path: &str, source: &str, into: &mut Ingestion) {
    let ast = document.get("module").unwrap_or(&Value::Null);
    let mut imports = Vec::new();

    for declaration in json::array(ast, "declarations") {
        let Some((tag, inner)) = json::tagged(declaration) else { continue };
        match tag {
            "Block" => ingest_block(inner, module, source, into),
            "Invariant" => {
                surfaces::ingest_invariant(inner, module, source, into);
                attach_declaration(inner, module, NodeKind::Invariant, source, &mut into.graph);
            }
            "Variant" => {
                ingest_variant(inner, module, &mut into.graph);
                attach_declaration(inner, module, NodeKind::Entity, source, &mut into.graph);
            }
            "Use" => {
                if let Some(import) = read_import(inner) {
                    imports.push(import);
                }
            }
            // `OpenQuestion` is prose about what is deliberately undecided. It
            // has no place in a graph of constructs and is not an error either,
            // so it is passed over rather than reported as unrecognised.
            _ => {}
        }
    }

    into.graph.modules.push(Module {
        name: module.to_owned(),
        path: path.to_owned(),
        imports,
        language_version: ast
            .get("version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok()),
    });
}

fn ingest_block(block: &Value, module: &str, source: &str, into: &mut Ingestion) {
    let kind = json::string_or_empty(block, "kind");
    let span = json::span(block, "span");

    match kind.as_str() {
        "Rule" => rules::ingest(block, module, source, into),
        "Surface" => surfaces::ingest_surface(block, module, source, into),
        "Actor" => surfaces::ingest_actor(block, module, source, into),
        // The model pass already built these from a document that describes
        // them far better than the AST does. All that is wanted here is where
        // they are, which is the one thing that document lacks.
        // Two spellings, one node kind. `external entity Staff { … }` parses as
        // its own declaration, and matching only `Entity` left every external
        // entity without a span — so the source panel was blank for exactly the
        // entities whose governing spec a reader most wants to go and find.
        "Entity" | "ExternalEntity" => {
            locate(block, module, NodeKind::Entity, span, &mut into.graph);
            retype(block, module, NodeKind::Entity, &mut into.graph);
        }
        "Enum" => locate(block, module, NodeKind::Enum, span, &mut into.graph),
        "Config" => locate_named(module, NodeKind::Config, "config", span, &mut into.graph),
        "Value" => ingest_value(block, module, span, &mut into.graph),
        _ => {}
    }

    // After the kind-specific pass, because the node has to exist to be given
    // anything. One place for every kind: a paragraph above a declaration means
    // the same thing whatever is declared.
    if let Some(id) = declared_id(block, module, &kind) {
        prose::attach(block, &id, span, source, &mut into.graph);
    }
    if kind == "Entity" || kind == "ExternalEntity" {
        annotate_fields(block, module, source, &mut into.graph);
    }
}

/// Give a declaration that is not a `Block` the writing above it.
fn attach_declaration(
    declaration: &Value,
    module: &str,
    kind: NodeKind,
    source: &str,
    graph: &mut SpecGraph,
) {
    let Some(name) = json::declared_name(declaration) else { return };
    let id = NodeId::new(module, kind, &name);
    prose::attach(declaration, &id, json::span(declaration, "span"), source, graph);
}

/// The node a block declares, where the kind is one that becomes a node.
fn declared_id(block: &Value, module: &str, kind: &str) -> Option<NodeId> {
    let node_kind = match kind {
        "Rule" => NodeKind::Rule,
        "Surface" => NodeKind::Surface,
        "Actor" => NodeKind::Actor,
        "Entity" | "ExternalEntity" => NodeKind::Entity,
        "Enum" => NodeKind::Enum,
        "Value" => NodeKind::Value,
        // A config block declares no name of its own; the node is called after
        // the keyword, which is what the rest of the graph refers to it by.
        "Config" => return Some(NodeId::new(module, NodeKind::Config, "config")),
        _ => return None,
    };
    Some(NodeId::new(module, node_kind, &json::declared_name(block)?))
}

/// Give each of an entity's fields the comment written above it.
///
/// Matched by name rather than by position: the model pass built the field list
/// from a different document, which reports derived values and stored fields in
/// an order of its own.
fn annotate_fields(block: &Value, module: &str, source: &str, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(block) else { return };
    let id = NodeId::new(module, NodeKind::Entity, &name);
    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id) else { return };
    let NodeDetail::Entity(detail) = &mut node.detail else { return };

    for item in json::array(block, "items") {
        let Some(assignment) = item.get("kind").and_then(|kind| kind.get("Assignment")) else {
            continue;
        };
        let Some(field_name) = json::declared_name(assignment) else { continue };
        let Some(span) = json::span(item, "span") else { continue };
        let note = prose::note_above(source, span.start);
        if note.is_empty() {
            continue;
        }
        if let Some(field) = detail.fields.iter_mut().find(|field| field.name == field_name) {
            field.note = note;
        }
    }
}

/// Fill in the declared type of any field the model pass left untyped.
///
/// `allium model` reads one file, so a relationship pointing into another module
/// comes back with the target it cannot name — literally `unknown`, which the
/// model pass discards. The AST has what the author actually wrote,
/// `membership/Membership with member = this`, and resolving that across the
/// spec set is the point of reading the files as a set.
///
/// Only empty types are filled. Where `model` did resolve a type it had more
/// context than a single assignment carries, and re-reading the syntax over the
/// top of that would be a downgrade.
fn retype(block: &Value, module: &str, kind: NodeKind, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(block) else { return };
    let declared = value_fields(block);
    if declared.is_empty() {
        return;
    }

    let id = NodeId::new(module, kind, &name);
    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id) else { return };
    let NodeDetail::Entity(detail) = &mut node.detail else { return };

    for field in &mut detail.fields {
        if !field.type_expr.is_empty() {
            continue;
        }
        if let Some(source) = declared.iter().find(|other| other.name == field.name) {
            field.type_expr.clone_from(&source.type_expr);
        }
    }
}

/// Give the already-ingested node for this block its span.
fn locate(block: &Value, module: &str, kind: NodeKind, span: Option<Span>, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(block) else { return };
    locate_named(module, kind, &name, span, graph);
}

fn locate_named(
    module: &str,
    kind: NodeKind,
    name: &str,
    span: Option<Span>,
    graph: &mut SpecGraph,
) {
    let id = NodeId::new(module, kind, name);
    if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id) {
        node.span = span;
    }
}

/// A value type: structured data with no identity.
///
/// `model` does not report these at all, so unlike an entity there is nothing
/// here to locate — the node has to be created. Its fields are left to the
/// linker, which reads them off the block's assignments the same way it reads
/// any other type reference.
fn ingest_value(block: &Value, module: &str, span: Option<Span>, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(block) else { return };
    let id = NodeId::new(module, NodeKind::Value, &name);
    if graph.nodes.iter().any(|node| node.id == id) {
        locate_named(module, NodeKind::Value, &name, span, graph);
        return;
    }
    graph.nodes.push(Node::new(module, NodeKind::Value, &name).at(span).with(NodeDetail::Entity(
        crate::graph::EntityDetail {
            kind: crate::graph::EntityKind::Value,
            fields: value_fields(block),
            transitions: Vec::new(),
            parent: None,
        },
    )));
}

/// The `name: Type` assignments inside a value or variant block.
fn value_fields(block: &Value) -> Vec<crate::graph::EntityField> {
    json::array(block, "items")
        .iter()
        .filter_map(|item| {
            let assignment = item.get("kind")?.get("Assignment")?;
            let name = json::declared_name(assignment)?;
            let type_expr =
                assignment.get("value").and_then(json::type_reference).unwrap_or_default();
            Some(crate::graph::EntityField::new(name, type_expr))
        })
        .collect()
}

/// A variant of a sum type: an entity that names its parent.
fn ingest_variant(declaration: &Value, module: &str, graph: &mut SpecGraph) {
    let Some(name) = json::declared_name(declaration) else { return };
    let parent =
        declaration.get("parent").or_else(|| declaration.get("base")).and_then(
            |parent| match parent {
                Value::String(text) => Some(text.clone()),
                other => json::string(other, "name"),
            },
        );

    graph.nodes.push(
        Node::new(module, NodeKind::Variant, &name).at(json::span(declaration, "span")).with(
            NodeDetail::Entity(crate::graph::EntityDetail {
                kind: crate::graph::EntityKind::Internal,
                fields: value_fields(declaration),
                transitions: Vec::new(),
                parent,
            }),
        ),
    );
}

/// One `use "./other.allium" as alias` declaration.
///
/// The path is a template of parts, because a spec may interpolate into it. Only
/// the literal text is read: an interpolated import cannot be resolved to a file
/// on disk anyway, and joining the parts would invent a path nothing points at.
fn read_import(declaration: &Value) -> Option<Import> {
    let alias = declaration.get("alias").and_then(json::declared_name)?;
    let path = declaration
        .get("path")
        .map(|path| {
            json::array(path, "parts")
                .iter()
                .filter_map(|part| match json::tagged(part) {
                    Some(("Text", Value::String(text))) => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    Some(Import { alias, path, target: None, span: json::span(declaration, "span") })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::graph::{EntityField, EntityKind};

    fn named(name: &str) -> Value {
        json!({"span": {"start": 0, "end": 0}, "name": name})
    }

    fn document(declarations: Value, version: u64) -> Value {
        json!({"module": {"version": version, "span": {"start": 0, "end": 0}, "declarations": declarations}})
    }

    fn ingested(declarations: Value) -> SpecGraph {
        ingested_with(declarations).0
    }

    /// Both halves, for the tests that care what the simulator was given.
    fn ingested_with(declarations: Value) -> (SpecGraph, crate::program::Program) {
        let mut into = Ingestion::empty("test");
        ingest(&document(declarations, 3), "lending", "lending.allium", "", &mut into);
        (into.graph, into.program)
    }

    #[test]
    fn a_module_records_its_name_path_and_language_version() {
        let graph = ingested(json!([]));
        assert_eq!(graph.modules.len(), 1);
        let module = &graph.modules[0];
        assert_eq!(module.name, "lending");
        assert_eq!(module.path, "lending.allium");
        assert_eq!(module.language_version, Some(3));
    }

    #[test]
    fn a_module_with_no_declared_version_records_none() {
        let mut into = Ingestion::empty("test");
        let document = json!({"module": {"declarations": []}});
        ingest(&document, "m", "m.allium", "", &mut into);
        assert_eq!(into.graph.modules[0].language_version, None);
    }

    #[test]
    fn a_use_declaration_becomes_an_unresolved_import() {
        // Unresolved at this stage by construction: this walk sees one file,
        // and which module a path points at is only knowable once every file
        // in the set has been read.
        let graph = ingested(json!([{"Use": {
            "span": {"start": 0, "end": 37},
            "path": {"span": {"start": 4, "end": 24}, "parts": [{"Text": "./catalogue.allium"}]},
            "alias": named("catalogue"),
        }}]));
        let import = &graph.modules[0].imports[0];
        assert_eq!(import.alias, "catalogue");
        assert_eq!(import.path, "./catalogue.allium");
        assert!(!import.is_resolved());
        assert_eq!(import.span, Some(Span::new(0, 37)));
    }

    #[test]
    fn an_import_with_no_alias_is_dropped() {
        let graph = ingested(json!([{"Use": {
            "path": {"parts": [{"Text": "./a.allium"}]},
        }}]));
        assert!(graph.modules[0].imports.is_empty());
    }

    #[test]
    fn an_interpolated_import_path_keeps_only_its_literal_text() {
        // Joining an interpolation into the path would invent a filename that
        // points at nothing, and the resolver would then report a miss against
        // a path the spec never wrote.
        let graph = ingested(json!([{"Use": {
            "path": {"parts": [{"Text": "./specs/"}, {"Interpolation": {"name": "env"}}]},
            "alias": named("other"),
        }}]));
        assert_eq!(graph.modules[0].imports[0].path, "./specs/");
    }

    #[test]
    fn an_entity_block_gives_an_existing_node_its_span() {
        // The model pass creates the node; this pass is the only source of
        // where it is.
        let mut into = Ingestion::empty("test");
        into.graph.nodes.push(Node::new("lending", NodeKind::Entity, "Loan"));
        assert_eq!(into.graph.nodes[0].span, None);

        let declarations = json!([{"Block": {
            "span": {"start": 100, "end": 400},
            "kind": "Entity",
            "name": named("Loan"),
            "items": [],
        }}]);
        ingest(&document(declarations, 3), "lending", "lending.allium", "", &mut into);

        assert_eq!(into.graph.nodes.len(), 1, "the node is located, not duplicated");
        assert_eq!(into.graph.nodes[0].span, Some(Span::new(100, 400)));
    }

    #[test]
    fn a_relationship_model_could_not_resolve_is_typed_from_the_source() {
        // The case this pass exists for. `allium model` reads one file, so a
        // relationship crossing a module boundary comes back with no target it
        // can name; the model pass drops that and this supplies the type the
        // author wrote, which the linker can then resolve across the set.
        let mut into = Ingestion::empty("test");
        let mut untyped = EntityField::new("conversations", "");
        untyped.relationship = true;
        into.graph.nodes.push(Node::new("identity", NodeKind::Entity, "Identity").with(
            NodeDetail::Entity(crate::graph::EntityDetail {
                kind: EntityKind::Internal,
                fields: vec![untyped, EntityField::new("name", "String")],
                transitions: Vec::new(),
                parent: None,
            }),
        ));

        let declarations = json!([{"Block": {
            "span": {"start": 0, "end": 80},
            "kind": "Entity",
            "name": named("Identity"),
            "items": [
                {"kind": {"Assignment": {
                    "name": named("conversations"),
                    "value": {"With": {
                        "source": {"QualifiedName": {"qualifier": "membership", "name": "Membership"}},
                        "condition": {"Ident": {"name": "active"}},
                    }},
                }}},
                {"kind": {"Assignment": {
                    "name": named("name"),
                    "value": {"Ident": {"name": "SomethingElse"}},
                }}},
            ],
        }}]);
        ingest(&document(declarations, 3), "identity", "identity.allium", "", &mut into);

        let detail = into
            .graph
            .node(&NodeId::new("identity", NodeKind::Entity, "Identity"))
            .and_then(|node| node.detail.as_entity())
            .expect("the entity");
        assert_eq!(
            detail.field("conversations").map(|f| f.type_expr.as_str()),
            Some("membership/Membership")
        );
        assert_eq!(
            detail.field("name").map(|f| f.type_expr.as_str()),
            Some("String"),
            "a type the model pass did resolve is not overwritten by the syntax"
        );
    }

    #[test]
    fn an_entity_block_with_no_matching_node_is_passed_over() {
        // A spec whose `model` output and `parse` output disagree should lose
        // nothing it already has and gain no half-built node.
        let graph = ingested(json!([{"Block": {
            "span": {"start": 1, "end": 2},
            "kind": "Entity",
            "name": named("Ghost"),
            "items": [],
        }}]));
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn a_config_block_locates_the_config_node() {
        let mut into = Ingestion::empty("test");
        into.graph.nodes.push(Node::new("lending", NodeKind::Config, "config"));
        let declarations = json!([{"Block": {
            "span": {"start": 10, "end": 60},
            "kind": "Config",
            "items": [],
        }}]);
        ingest(&document(declarations, 3), "lending", "lending.allium", "", &mut into);
        assert_eq!(into.graph.nodes[0].span, Some(Span::new(10, 60)));
    }

    #[test]
    fn a_value_block_creates_a_node_because_model_never_reports_one() {
        let graph = ingested(json!([{"Block": {
            "span": {"start": 5, "end": 90},
            "kind": "Value",
            "name": named("LoanWindow"),
            "items": [
                {"kind": {"Assignment": {"name": named("opened_at"), "value": {"Ident": {"name": "Timestamp"}}}}},
                {"kind": {"Assignment": {"name": named("due_at"), "value": {"Ident": {"name": "Timestamp"}}}}},
            ],
        }}]));
        let node =
            graph.node(&NodeId::new("lending", NodeKind::Value, "LoanWindow")).expect("the value");
        assert_eq!(node.span, Some(Span::new(5, 90)));
        let detail = node.detail.as_entity().expect("an entity-shaped detail");
        assert_eq!(detail.kind, EntityKind::Value);
        assert_eq!(detail.fields.len(), 2);
        assert_eq!(detail.field("due_at").map(|f| f.type_expr.as_str()), Some("Timestamp"));
    }

    #[test]
    fn a_value_field_typed_across_modules_keeps_its_qualifier() {
        let graph = ingested(json!([{"Block": {
            "kind": "Value",
            "name": named("Holder"),
            "items": [{"kind": {"Assignment": {
                "name": named("copy"),
                "value": {"QualifiedName": {"qualifier": "catalogue", "name": "Copy"}},
            }}}],
        }}]));
        let detail = graph
            .node(&NodeId::new("lending", NodeKind::Value, "Holder"))
            .and_then(|node| node.detail.as_entity())
            .expect("the value");
        assert_eq!(detail.field("copy").map(|f| f.type_expr.as_str()), Some("catalogue/Copy"));
    }

    #[test]
    fn a_value_block_for_a_node_model_already_made_locates_it_instead() {
        let mut into = Ingestion::empty("test");
        into.graph.nodes.push(Node::new("lending", NodeKind::Value, "LoanWindow"));
        let declarations = json!([{"Block": {
            "span": {"start": 5, "end": 90},
            "kind": "Value",
            "name": named("LoanWindow"),
            "items": [],
        }}]);
        ingest(&document(declarations, 3), "lending", "lending.allium", "", &mut into);
        assert_eq!(into.graph.nodes.len(), 1);
        assert_eq!(into.graph.nodes[0].span, Some(Span::new(5, 90)));
    }

    #[test]
    fn a_variant_records_the_sum_type_it_belongs_to() {
        let graph = ingested(json!([{"Variant": {
            "span": {"start": 3, "end": 40},
            "name": named("SeenAnnouncement"),
            "parent": named("PendingAnnouncement"),
            "items": [],
        }}]));
        let detail = graph
            .node(&NodeId::new("lending", NodeKind::Variant, "SeenAnnouncement"))
            .and_then(|node| node.detail.as_entity())
            .expect("the variant");
        assert_eq!(detail.parent.as_deref(), Some("PendingAnnouncement"));
    }

    #[test]
    fn a_variant_whose_parent_is_a_plain_string_is_read_too() {
        let graph = ingested(json!([{"Variant": {
            "name": named("A"),
            "base": "Parent",
        }}]));
        let detail = graph
            .node(&NodeId::new("lending", NodeKind::Variant, "A"))
            .and_then(|node| node.detail.as_entity())
            .expect("the variant");
        assert_eq!(detail.parent.as_deref(), Some("Parent"));
    }

    #[test]
    fn an_open_question_is_skipped_without_disturbing_anything_else() {
        // Prose about what is deliberately undecided. Not a construct, and not
        // an error either.
        let graph = ingested(json!([
            {"OpenQuestion": {"span": {"start": 0, "end": 5}, "text": "unresolved"}},
            {"Block": {"kind": "Value", "name": named("V"), "items": []}},
        ]));
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].name, "V");
    }

    #[test]
    fn a_declaration_kind_this_crate_has_never_seen_leaves_the_rest_intact() {
        let graph = ingested(json!([
            {"SomethingFromAFutureRelease": {"span": {"start": 0, "end": 1}}},
            {"Block": {"kind": "Value", "name": named("V"), "items": []}},
        ]));
        assert_eq!(graph.nodes.len(), 1, "the recognised declaration still lands");
        assert_eq!(graph.modules.len(), 1, "and the module is still recorded");
    }

    #[test]
    fn a_block_kind_this_crate_has_never_seen_is_skipped() {
        let graph = ingested(json!([{"Block": {
            "kind": "Contract",
            "name": named("SomeContract"),
            "items": [],
        }}]));
        assert!(graph.nodes.is_empty());
        assert_eq!(graph.modules.len(), 1);
    }

    #[test]
    fn a_document_with_no_module_still_records_the_module_row() {
        // The file was read and the graph should say so, even when the CLI
        // returned nothing usable about its contents.
        let mut into = Ingestion::empty("test");
        ingest(&json!({}), "broken", "broken.allium", "", &mut into);
        assert_eq!(into.graph.modules.len(), 1);
        assert_eq!(into.graph.modules[0].name, "broken");
        assert!(into.graph.nodes.is_empty());
    }
}
