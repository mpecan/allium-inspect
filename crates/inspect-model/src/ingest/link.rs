//! Resolving references, within a module and across module boundaries.
//!
//! Up to this point every module has been read on its own, so a field typed
//! `catalogue/Copy` is a string and a `use` declaration points at a path rather
//! than a module. This pass is where a set of files becomes one graph.
//!
//! Three rules govern it.
//!
//! *An import binds an alias to a module.* `use "./catalogue.allium" as
//! catalogue` means `catalogue/Copy` in this file is `Copy` in that module. The
//! alias is not always the module's own name, so the binding has to be read from
//! the declaration rather than assumed.
//!
//! *A reference that resolves becomes an edge.* That is the whole point: the
//! cross-module arrows are the thing no other Allium tool draws.
//!
//! *A reference that does not resolve becomes a visible node.* Built-in types
//! aside, a name pointing at nothing is a fact about the spec worth seeing.
//! Dropping the edge would make the graph look complete when it is not.

use std::collections::{BTreeMap, BTreeSet};

use crate::graph::{
    Edge, EdgeKind, Node, NodeDetail, NodeId, NodeKind, SpecGraph, TriggerDetail, TriggerSource,
};

/// Types the language provides, which name no construct in any module.
///
/// Listed so an ordinary `title: String` does not litter the canvas with an
/// `External` node for `String` in every module that has a name.
const BUILT_IN: &[&str] = &[
    "Boolean",
    "Bool",
    "Date",
    "DateTime",
    "Decimal",
    "Duration",
    "Float",
    "Integer",
    "Int",
    "Money",
    "Number",
    "Percentage",
    "String",
    "Text",
    "Time",
    "Timestamp",
    "Url",
    "Uuid",
];

/// Resolve every reference in `graph`, adding edges and external nodes.
pub fn link(graph: &mut SpecGraph) {
    resolve_imports(graph);
    drop_imports_we_resolved(graph);
    declare_offered_triggers(graph);
    let aliases = alias_table(graph);
    let known: BTreeSet<NodeId> = graph.nodes.iter().map(|node| node.id.clone()).collect();

    let mut edges = Vec::new();
    let mut externals: BTreeMap<NodeId, Node> = BTreeMap::new();

    for node in &graph.nodes {
        let Some(detail) = node.detail.as_entity() else { continue };
        for field in &detail.fields {
            // A derived value is typed by the expression that computes it —
            // `copies.count` — which names no construct. Only a stored field or
            // a relationship refers to a type.
            if field.derived && !field.relationship {
                continue;
            }
            let Some(reference) = type_of(&field.type_expr) else { continue };
            let Some(target) = resolve(&reference, &node.module, &aliases, &known, &mut externals)
            else {
                continue;
            };
            if target == node.id {
                // A self-reference is real and common (`parents: Set<Message>`)
                // but an arrow from a node to itself adds nothing to read.
                continue;
            }
            let kind = if field.relationship { EdgeKind::Relationship } else { EdgeKind::Field };
            edges.push(Edge::new(node.id.clone(), target, kind, field.name.clone()));
        }
    }

    // Every edge ingestion produced assumed its target was in the same module.
    // Anything still pointing at a node that does not exist is re-aimed here,
    // which is how a surface's `facing` clause finds an entity used directly as
    // a party, and how an emitted trigger finds the module that declares it.
    let existing: Vec<Edge> = graph.edges.drain(..).collect();
    for mut edge in existing {
        if !known.contains(&edge.to)
            && let Some(target) = relocate(&edge, &aliases, &known, &mut externals)
        {
            edge.to = target;
        }
        edges.push(edge);
    }

    graph.edges = edges;
    graph.nodes.extend(externals.into_values());
    graph.normalise();
}

/// Point each import at the module it names, where the set holds one.
fn resolve_imports(graph: &mut SpecGraph) {
    let names: BTreeSet<String> = graph.modules.iter().map(|module| module.name.clone()).collect();
    for module in &mut graph.modules {
        for import in &mut module.imports {
            import.target = module_of_path(&import.path).filter(|name| names.contains(name));
        }
    }
}

/// Give a trigger a surface offers a node, when nothing else made one.
///
/// A trigger node is otherwise created only by the `when` clause that waits for
/// one — so an operation a surface offers and no rule consumes had no node, and
/// resolved to an unresolved external reference. It is not unresolved: the spec
/// declares it right there, by offering it. `plan` already makes the same
/// argument for a trigger a rule emits and nothing listens for.
///
/// What it *is* is an operation with nothing behind it, which is worth seeing.
/// Drawn as a trigger with nothing following it, that is exactly what it looks
/// like; drawn as an unresolved reference, it looks like a typo.
fn declare_offered_triggers(graph: &mut SpecGraph) {
    let known: BTreeSet<NodeId> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    let missing: BTreeSet<NodeId> = graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Provides)
        .map(|edge| edge.to.clone())
        .filter(|id| !known.contains(id))
        .collect();

    for id in missing {
        graph.nodes.push(Node::new(id.module(), NodeKind::Trigger, id.name()).with(
            NodeDetail::Trigger(TriggerDetail {
                source: TriggerSource::External,
                parameters: Vec::new(),
                condition: None,
                entity: None,
            }),
        ));
    }
}

/// The CLI's code for a `use` that named a file it was not given.
const UNRESOLVED_USE: &str = "allium.use.unresolvedPath";

/// Drop the unresolved-import warnings that only this tool's own ingestion
/// caused.
///
/// The CLI answers about one file at a time, so it is run once per file, and a
/// file's `use "./identity.allium"` therefore resolves against a check set of
/// one. The CLI is right to warn: from where it was standing, that path names
/// nothing. Run over the directory it reports none of these.
///
/// Ingesting a *set* is the point of this tool, and it has just resolved that
/// same import to a module it holds. Passing the warning on anyway would report
/// a defect the specification does not have, to a reader who came here to find
/// out whether it has any — which is the most expensive thing this tool could
/// get wrong.
///
/// An import that genuinely did not resolve keeps its warning. A `use` naming a
/// file nobody passed in is exactly what a reviewer needs to be told.
fn drop_imports_we_resolved(graph: &mut SpecGraph) {
    let resolved: BTreeSet<(String, String)> = graph
        .modules
        .iter()
        .flat_map(|module| {
            module
                .imports
                .iter()
                .filter(|import| import.is_resolved())
                .map(move |import| (module.name.clone(), import.path.clone()))
        })
        .collect();

    graph.diagnostics.retain(|diagnostic| {
        if diagnostic.code.as_deref() != Some(UNRESOLVED_USE) {
            return true;
        }
        // Matched on the path the message quotes rather than on a line number:
        // the import's span is a byte offset and this pass has no source text
        // to turn one into a line.
        !resolved.iter().any(|(module, path)| {
            *module == diagnostic.module && diagnostic.message.contains(&format!("\"{path}\""))
        })
    });
}

/// The module name a `use` path refers to: its file stem.
fn module_of_path(path: &str) -> Option<String> {
    let file = path.rsplit(['/', '\\']).next()?;
    let stem = file.strip_suffix(".allium").unwrap_or(file);
    if stem.is_empty() { None } else { Some(stem.to_owned()) }
}

/// `(module, alias) -> target module`, for every resolved import.
fn alias_table(graph: &SpecGraph) -> BTreeMap<(String, String), String> {
    graph
        .modules
        .iter()
        .flat_map(|module| {
            module.imports.iter().filter_map(move |import| {
                Some(((module.name.clone(), import.alias.clone()), import.target.clone()?))
            })
        })
        .collect()
}

/// The construct a type expression refers to, if it refers to one at all.
///
/// A field's `type_expr` may be a type (`Copy`, `catalogue/Copy`), a container
/// (`Set<Book>`), or an expression that computes a value (`copies.count`,
/// `receipts where kind = read -> reporter`). Only the first two name something.
fn type_of(type_expr: &str) -> Option<String> {
    let trimmed = type_expr.trim();
    if trimmed.is_empty() {
        return None;
    }
    // An inline enumeration declares its states rather than referring to a type.
    if trimmed.contains('|') {
        return None;
    }
    // Unwrap one container layer: `Set<Book>` refers to `Book`.
    let inner = match (trimmed.find('<'), trimmed.strip_suffix('>')) {
        (Some(open), Some(_)) => trimmed.get(open + 1..trimmed.len() - 1)?.trim(),
        _ => trimmed,
    };
    let name = inner.strip_suffix('?').unwrap_or(inner).trim();

    // Anything left holding an operator, a space or a dot is an expression that
    // computes a value rather than a name that refers to a type.
    if name.is_empty()
        || BUILT_IN.contains(&name)
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '/')
        || name.matches('/').count() > 1
    {
        return None;
    }

    // Capitalisation is checked on the *last* segment. A qualified reference
    // starts with its module alias, which is lowercase by convention, so
    // testing the first character of the whole string rejects every
    // cross-module type — the one kind of reference this pass exists to
    // resolve.
    let bare = name.rsplit('/').next().unwrap_or(name);
    let names_a_type =
        bare.chars().next().is_some_and(|first| first.is_ascii_uppercase() || first == '_');

    names_a_type.then(|| name.to_owned())
}

/// The node a reference names, creating an external node when nothing matches.
fn resolve(
    reference: &str,
    module: &str,
    aliases: &BTreeMap<(String, String), String>,
    known: &BTreeSet<NodeId>,
    externals: &mut BTreeMap<NodeId, Node>,
) -> Option<NodeId> {
    let (target_module, name) = match reference.split_once('/') {
        Some((alias, name)) => (
            aliases.get(&(module.to_owned(), alias.to_owned())).cloned().unwrap_or_else(|| {
                // An unresolved alias is still a namespace the author wrote, so
                // it is kept as one rather than flattened into this module.
                alias.to_owned()
            }),
            name.to_owned(),
        ),
        None => (module.to_owned(), reference.to_owned()),
    };

    for kind in [NodeKind::Entity, NodeKind::Value, NodeKind::Variant, NodeKind::Enum] {
        let id = NodeId::new(&target_module, kind, &name);
        if known.contains(&id) {
            return Some(id);
        }
    }

    let id = NodeId::new(&target_module, NodeKind::External, &name);
    externals
        .entry(id.clone())
        .or_insert_with(|| Node::new(&target_module, NodeKind::External, &name));
    Some(id)
}

/// Re-aim an edge whose target does not exist under the kind it assumed.
///
/// Ingestion writes an edge before it can know where its target lives: a
/// surface's `facing` clause writes to an actor, a rule's emission writes to a
/// trigger in its own module. Both are right most of the time and wrong at
/// exactly the interesting boundaries.
fn relocate(
    edge: &Edge,
    aliases: &BTreeMap<(String, String), String>,
    known: &BTreeSet<NodeId>,
    externals: &mut BTreeMap<NodeId, Node>,
) -> Option<NodeId> {
    let module = edge.to.module().to_owned();
    let name = edge.to.name().to_owned();
    if name.is_empty() {
        return None;
    }

    // A qualified name in the label takes precedence: it says where to look.
    let (search_module, search_name) = match name.split_once('/') {
        Some((alias, bare)) => (
            aliases.get(&(module.clone(), alias.to_owned())).cloned().unwrap_or(alias.to_owned()),
            bare.to_owned(),
        ),
        None => (module, name),
    };

    // Same module, any kind that could plausibly carry this name.
    for kind in candidate_kinds(edge.kind) {
        let id = NodeId::new(&search_module, *kind, &search_name);
        if known.contains(&id) {
            return Some(id);
        }
    }

    // Any other module: a trigger emitted here and consumed there is the whole
    // reason the flow graph crosses files.
    let elsewhere = known.iter().find(|id| {
        id.name() == search_name
            && candidate_kinds(edge.kind)
                .iter()
                .any(|kind| id.as_str().contains(&format!("::{}::", kind.as_str())))
    });
    if let Some(id) = elsewhere {
        return Some(id.clone());
    }

    let id = NodeId::new(&search_module, NodeKind::External, &search_name);
    externals
        .entry(id.clone())
        .or_insert_with(|| Node::new(&search_module, NodeKind::External, &search_name));
    Some(id)
}

/// The kinds an edge of this sort could legitimately point at.
fn candidate_kinds(kind: EdgeKind) -> &'static [NodeKind] {
    match kind {
        // A `facing` clause names an actor, or an entity used directly.
        EdgeKind::Facing => &[NodeKind::Actor, NodeKind::Entity],
        EdgeKind::Emits | EdgeKind::Provides | EdgeKind::Triggers => &[NodeKind::Trigger],
        EdgeKind::Creates | EdgeKind::Reads | EdgeKind::Mutates | EdgeKind::Constrains => {
            &[NodeKind::Entity, NodeKind::Value, NodeKind::Variant]
        }
        EdgeKind::IdentifiedBy => &[NodeKind::Entity, NodeKind::External],
        // `VariantOf` is deliberately not listed. A sum type is an entity and a
        // variant of one is drawn as a variant, and the fallback already allows
        // both — a narrower arm here would change no outcome, which is the sort
        // of code that looks like a decision and is not one.
        _ => &[NodeKind::Entity, NodeKind::Value, NodeKind::Variant, NodeKind::Enum],
    }
}

/// Whether a node stands for a reference nothing in the set declares.
#[must_use]
pub fn is_unresolved(node: &Node) -> bool {
    node.kind == NodeKind::External && node.detail == NodeDetail::None
}

#[cfg(test)]
mod tests {
    fn use_warning(module: &str, path: &str) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            message: format!(
                "Use path \"{path}\" does not resolve to a file in the current check set."
            ),
            code: Some("allium.use.unresolvedPath".to_owned()),
            location: None,
            module: module.to_owned(),
            node: None,
        }
    }

    fn named(module: &str) -> Module {
        Module {
            name: module.to_owned(),
            path: format!("specs/{module}.allium"),
            imports: Vec::new(),
            language_version: None,
        }
    }

    fn importing(module: &str, path: &str, alias: &str) -> Module {
        let mut built = named(module);
        built.imports.push(Import {
            alias: alias.to_owned(),
            path: path.to_owned(),
            target: None,
            span: None,
        });
        built
    }

    #[test]
    fn a_trigger_a_surface_offers_is_declared_by_the_offering() {
        // A trigger node is otherwise made only by the `when` clause that waits
        // for one, so an operation nothing consumes resolved to an unresolved
        // external reference — which reads as a typo. It is not: the spec
        // declares it right there, by offering it. What it *is* is an operation
        // with nothing behind it, and a trigger with nothing following it is
        // what that looks like.
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(Node::new("delivery", NodeKind::Surface, "HubStorage"));
        graph.edges.push(Edge::new(
            NodeId::new("delivery", NodeKind::Surface, "HubStorage"),
            NodeId::new("delivery", NodeKind::Trigger, "OperatorShowsCode"),
            EdgeKind::Provides,
            "OperatorShowsCode",
        ));

        link(&mut graph);

        let made = graph
            .node(&NodeId::new("delivery", NodeKind::Trigger, "OperatorShowsCode"))
            .expect("the trigger the surface declared");
        assert_eq!(made.kind, NodeKind::Trigger);
        assert!(graph.nodes.iter().all(|node| node.kind != NodeKind::External));
    }

    #[test]
    fn a_trigger_a_rule_already_waits_for_is_not_declared_twice() {
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(Node::new("delivery", NodeKind::Surface, "Desk"));
        graph.nodes.push(Node::new("delivery", NodeKind::Trigger, "Borrow"));
        graph.edges.push(Edge::new(
            NodeId::new("delivery", NodeKind::Surface, "Desk"),
            NodeId::new("delivery", NodeKind::Trigger, "Borrow"),
            EdgeKind::Provides,
            "Borrow",
        ));

        link(&mut graph);

        let triggers = graph.nodes.iter().filter(|node| node.kind == NodeKind::Trigger).count();
        assert_eq!(triggers, 1);
    }

    #[test]
    fn an_import_this_tool_resolved_does_not_keep_its_warning() {
        // The CLI answers about one file, so every cross-file `use` in a spec
        // set comes back unresolved. Passing that on reports a defect the spec
        // does not have — over the directory the CLI reports none of them.
        let mut graph = SpecGraph::new("v");
        graph.modules.push(importing("archive", "./identity.allium", "identity"));
        graph.modules.push(named("identity"));
        graph.diagnostics.push(use_warning("archive", "./identity.allium"));

        link(&mut graph);

        assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
        assert_eq!(graph.modules[0].imports[0].target.as_deref(), Some("identity"));
    }

    #[test]
    fn an_import_that_really_does_not_resolve_keeps_its_warning() {
        // A `use` naming a file nobody passed in is exactly what a reviewer
        // needs to be told, and it is the same warning either way — so the
        // distinction has to be drawn on whether *we* found the module.
        let mut graph = SpecGraph::new("v");
        graph.modules.push(importing("archive", "./missing.allium", "missing"));
        graph.diagnostics.push(use_warning("archive", "./missing.allium"));

        link(&mut graph);

        assert_eq!(graph.diagnostics.len(), 1);
        assert!(graph.modules[0].imports[0].target.is_none());
    }

    #[test]
    fn only_the_import_that_resolved_loses_its_warning() {
        let mut graph = SpecGraph::new("v");
        let mut archive = importing("archive", "./identity.allium", "identity");
        archive.imports.push(Import {
            alias: "missing".to_owned(),
            path: "./missing.allium".to_owned(),
            target: None,
            span: None,
        });
        graph.modules.push(archive);
        graph.modules.push(named("identity"));
        graph.diagnostics.push(use_warning("archive", "./identity.allium"));
        graph.diagnostics.push(use_warning("archive", "./missing.allium"));

        link(&mut graph);

        let kept: Vec<&str> = graph.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].contains("missing.allium"), "{kept:?}");
    }

    #[test]
    fn a_warning_about_the_same_path_in_another_module_is_left_alone() {
        // Two files can both `use "./identity.allium"`, and only one of them
        // needs to be in the set for the other's warning to be real. The match
        // is per module for that reason.
        let mut graph = SpecGraph::new("v");
        graph.modules.push(importing("archive", "./identity.allium", "identity"));
        graph.modules.push(named("identity"));
        graph.diagnostics.push(use_warning("delivery", "./identity.allium"));

        link(&mut graph);

        assert_eq!(graph.diagnostics.len(), 1, "delivery was never ingested");
    }

    #[test]
    fn a_diagnostic_that_is_not_about_an_import_is_never_dropped() {
        // The filter is keyed on the CLI's code, not on the wording, so that a
        // lifecycle warning that happens to mention a path survives.
        let mut graph = SpecGraph::new("v");
        graph.modules.push(importing("archive", "./identity.allium", "identity"));
        graph.modules.push(named("identity"));
        graph.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message: "Status 'pending' has no observed transition, see \"./identity.allium\""
                .to_owned(),
            code: Some("allium.transition.stuck".to_owned()),
            location: None,
            module: "archive".to_owned(),
            node: None,
        });

        link(&mut graph);

        assert_eq!(graph.diagnostics.len(), 1);
    }

    use super::*;
    use crate::{
        diagnostic::{Diagnostic, Severity},
        graph::{EntityDetail, EntityField, EntityKind, Import, Module},
    };

    fn module(name: &str, imports: Vec<Import>) -> Module {
        Module {
            name: name.to_owned(),
            path: format!("specs/{name}.allium"),
            imports,
            language_version: Some(3),
        }
    }

    fn import(alias: &str, path: &str) -> Import {
        Import { alias: alias.to_owned(), path: path.to_owned(), target: None, span: None }
    }

    fn entity(module: &str, name: &str, fields: Vec<EntityField>) -> Node {
        Node::new(module, NodeKind::Entity, name).with(NodeDetail::Entity(EntityDetail {
            kind: EntityKind::Internal,
            fields,
            transitions: Vec::new(),
            parent: None,
        }))
    }

    /// `catalogue` declares `Book` and `Copy`; `lending` imports it and declares
    /// `Loan`, whose `copy` field points across the boundary.
    fn two_modules() -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("catalogue", Vec::new()));
        graph.modules.push(module("lending", vec![import("catalogue", "./catalogue.allium")]));
        graph.nodes.push(entity("catalogue", "Book", Vec::new()));
        graph.nodes.push(entity("catalogue", "Copy", Vec::new()));
        graph.nodes.push(entity(
            "lending",
            "Loan",
            vec![
                EntityField::new("copy", "catalogue/Copy"),
                EntityField::new("reference", "String"),
            ],
        ));
        graph
    }

    // --- type expressions ------------------------------------------------

    #[test]
    fn a_bare_type_name_is_a_reference() {
        assert_eq!(type_of("Copy").as_deref(), Some("Copy"));
        assert_eq!(type_of("  Copy  ").as_deref(), Some("Copy"));
    }

    #[test]
    fn a_qualified_type_name_is_a_reference() {
        assert_eq!(type_of("catalogue/Copy").as_deref(), Some("catalogue/Copy"));
    }

    #[test]
    fn a_container_refers_to_what_it_contains() {
        assert_eq!(type_of("Set<Book>").as_deref(), Some("Book"));
        assert_eq!(type_of("Set<catalogue/Book>").as_deref(), Some("catalogue/Book"));
    }

    #[test]
    fn an_optional_type_refers_to_what_it_wraps() {
        assert_eq!(type_of("Attachment?").as_deref(), Some("Attachment"));
    }

    #[test]
    fn a_built_in_type_names_no_construct() {
        // Otherwise every entity with a name would put a `String` node on the
        // canvas of every module.
        for built_in in ["String", "Integer", "Timestamp", "Duration", "Boolean"] {
            assert_eq!(type_of(built_in), None, "{built_in} is a built-in");
        }
    }

    #[test]
    fn an_inline_enumeration_names_no_construct() {
        assert_eq!(type_of("listed | withdrawn"), None);
        assert_eq!(type_of("available | on_loan | lost"), None);
    }

    #[test]
    fn a_computed_expression_names_no_construct() {
        assert_eq!(type_of("copies.count"), None);
        assert_eq!(type_of("receipts where kind = read -> reporter"), None);
        assert_eq!(type_of("delivered_to.count"), None);
        assert_eq!(type_of("attachment != null"), None);
    }

    #[test]
    fn a_lowercase_name_is_an_expression_not_a_type() {
        // Types are PascalCase in Allium; a lowercase bare word is a field
        // being projected, and pointing an edge at it would invent a node.
        assert_eq!(type_of("loans"), None);
        assert_eq!(type_of("open_loans"), None);
    }

    #[test]
    fn an_empty_type_expression_names_no_construct() {
        assert_eq!(type_of(""), None);
        assert_eq!(type_of("   "), None);
    }

    #[test]
    fn a_doubly_qualified_reference_is_not_resolved() {
        // Allium namespaces are one level deep. `a/b/C` is not a name this
        // crate can place, and splitting it anyway would put an `a` module on
        // the canvas that the spec never declared.
        assert_eq!(type_of("a/b/C"), None);
    }

    #[test]
    fn a_name_holding_an_operator_or_a_space_is_not_a_reference() {
        assert_eq!(type_of("A B"), None);
        assert_eq!(type_of("A-B"), None);
        assert_eq!(type_of("A.B"), None);
    }

    #[test]
    fn a_qualified_reference_whose_type_is_lowercase_is_not_a_reference() {
        // The capitalisation check reads the segment after the slash; the one
        // before it is the module alias and is lowercase by convention.
        assert_eq!(type_of("catalogue/copies"), None);
    }

    #[test]
    fn an_underscore_prefixed_name_is_accepted_as_a_reference() {
        assert_eq!(type_of("_Internal").as_deref(), Some("_Internal"));
    }

    #[test]
    fn candidate_kinds_are_specific_to_what_an_edge_can_point_at() {
        // Each arm exists because the wrong one produces a wrong answer: a
        // `facing` clause resolving to a trigger, or an emission resolving to
        // an entity, would silently join two unrelated constructs.
        assert_eq!(candidate_kinds(EdgeKind::Facing), [NodeKind::Actor, NodeKind::Entity]);
        assert_eq!(candidate_kinds(EdgeKind::Emits), [NodeKind::Trigger]);
        assert_eq!(candidate_kinds(EdgeKind::Provides), [NodeKind::Trigger]);
        assert_eq!(candidate_kinds(EdgeKind::Triggers), [NodeKind::Trigger]);
        assert_eq!(
            candidate_kinds(EdgeKind::Creates),
            [NodeKind::Entity, NodeKind::Value, NodeKind::Variant]
        );
        assert_eq!(candidate_kinds(EdgeKind::Constrains), candidate_kinds(EdgeKind::Creates));
        assert_eq!(candidate_kinds(EdgeKind::IdentifiedBy), [NodeKind::Entity, NodeKind::External]);
        assert_eq!(
            candidate_kinds(EdgeKind::Field),
            [NodeKind::Entity, NodeKind::Value, NodeKind::Variant, NodeKind::Enum]
        );
    }

    #[test]
    fn an_actor_identified_by_something_undeclared_resolves_to_an_external_node() {
        // The `IdentifiedBy` arm accepts `External` so a second link pass does
        // not create a second stand-in for the same missing entity.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Actor, "Reader"));
        graph.edges.push(Edge::new(
            NodeId::new("m", NodeKind::Actor, "Reader"),
            NodeId::new("m", NodeKind::Entity, "Absent"),
            EdgeKind::IdentifiedBy,
            "Absent",
        ));
        link(&mut graph);
        link(&mut graph);
        assert_eq!(graph.nodes_of(NodeKind::External).count(), 1);
    }

    #[test]
    fn a_facing_edge_prefers_an_actor_over_an_entity_of_the_same_name() {
        // Both kinds are legal in a `facing` clause, so order decides. An actor
        // says who the party is; an entity of the same name is the fallback.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Actor, "Reader"));
        graph.nodes.push(entity("m", "Reader", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Surface, "Shelf"));
        graph.edges.push(Edge::new(
            NodeId::new("m", NodeKind::Surface, "Shelf"),
            NodeId::new("m", NodeKind::Trigger, "Reader"),
            EdgeKind::Facing,
            "Reader",
        ));
        link(&mut graph);
        let edge = graph.edges.iter().find(|e| e.kind == EdgeKind::Facing).expect("an edge");
        assert_eq!(edge.to, NodeId::new("m", NodeKind::Actor, "Reader"));
    }

    #[test]
    fn a_cross_module_search_matches_the_name_and_not_merely_the_kind() {
        // The search that joins an emission to the module declaring the trigger
        // looks in every module. Matching on kind alone would attach the
        // emission to whichever trigger happened to be found first.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("catalogue", Vec::new()));
        graph.modules.push(module("lending", Vec::new()));
        graph.nodes.push(Node::new("catalogue", NodeKind::Trigger, "SomethingElse"));
        graph.nodes.push(Node::new("lending", NodeKind::Rule, "R"));
        graph.edges.push(Edge::new(
            NodeId::new("lending", NodeKind::Rule, "R"),
            NodeId::new("lending", NodeKind::Trigger, "CopyLost"),
            EdgeKind::Emits,
            "CopyLost",
        ));
        link(&mut graph);

        let edge = graph.edges.iter().find(|e| e.kind == EdgeKind::Emits).expect("an edge");
        assert_eq!(
            edge.to,
            NodeId::new("lending", NodeKind::External, "CopyLost"),
            "an unrelated trigger must not be adopted just because it is a trigger"
        );
    }

    #[test]
    fn an_edge_whose_target_names_nothing_at_all_is_left_alone() {
        // `relocate` needs both a module and a name to search with. A malformed
        // id has neither, and inventing an external node for it would put an
        // unnamed box on the canvas.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Rule, "R"));
        let broken = Edge::new(
            NodeId::new("m", NodeKind::Rule, "R"),
            NodeId("malformed".to_owned()),
            EdgeKind::Emits,
            "",
        );
        graph.edges.push(broken.clone());
        link(&mut graph);
        assert_eq!(graph.edges, [broken]);
        assert!(graph.nodes_of(NodeKind::External).next().is_none());
    }

    #[test]
    fn an_edge_label_that_qualifies_its_target_is_followed_across_modules() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("catalogue", Vec::new()));
        graph.modules.push(module("lending", vec![import("cat", "./catalogue.allium")]));
        graph.nodes.push(Node::new("catalogue", NodeKind::Trigger, "CopyLost"));
        graph.nodes.push(Node::new("lending", NodeKind::Rule, "R"));
        graph.edges.push(Edge::new(
            NodeId::new("lending", NodeKind::Rule, "R"),
            NodeId::new("lending", NodeKind::Trigger, "cat/CopyLost"),
            EdgeKind::Emits,
            "cat/CopyLost",
        ));
        link(&mut graph);
        let edge = graph.edges.iter().find(|e| e.kind == EdgeKind::Emits).expect("an edge");
        assert_eq!(edge.to, NodeId::new("catalogue", NodeKind::Trigger, "CopyLost"));
    }

    #[test]
    fn only_an_external_node_with_no_detail_counts_as_unresolved() {
        // Both halves matter. A node of another kind is declared; an external
        // node that has since been given detail is a placeholder that was
        // filled in, and reporting either as unresolved would be a false alarm.
        let bare = Node::new("m", NodeKind::External, "Phantom");
        assert!(is_unresolved(&bare));

        let declared = Node::new("m", NodeKind::Entity, "Book");
        assert!(!is_unresolved(&declared), "a declared entity is not unresolved");

        let filled =
            Node::new("m", NodeKind::External, "Phantom").with(NodeDetail::Entity(EntityDetail {
                kind: EntityKind::External,
                fields: Vec::new(),
                transitions: Vec::new(),
                parent: None,
            }));
        assert!(!is_unresolved(&filled), "a placeholder that was filled in is resolved");
    }

    // --- imports ---------------------------------------------------------

    #[test]
    fn a_use_path_names_the_module_by_its_file_stem() {
        assert_eq!(module_of_path("./catalogue.allium").as_deref(), Some("catalogue"));
        assert_eq!(module_of_path("../specs/lending.allium").as_deref(), Some("lending"));
        assert_eq!(module_of_path("catalogue").as_deref(), Some("catalogue"));
    }

    #[test]
    fn an_empty_use_path_names_no_module() {
        assert_eq!(module_of_path(""), None);
        assert_eq!(module_of_path("./"), None);
    }

    #[test]
    fn an_import_resolves_to_a_module_in_the_set() {
        let mut graph = two_modules();
        link(&mut graph);
        let lending = graph.modules.iter().find(|m| m.name == "lending").expect("lending");
        assert_eq!(lending.imports[0].target.as_deref(), Some("catalogue"));
        assert!(lending.imports[0].is_resolved());
    }

    #[test]
    fn an_import_of_a_file_outside_the_set_stays_unresolved() {
        let mut graph = two_modules();
        if let Some(module) = graph.modules.iter_mut().find(|m| m.name == "lending") {
            module.imports.push(import("elsewhere", "./elsewhere.allium"));
        }
        link(&mut graph);
        let lending = graph.modules.iter().find(|m| m.name == "lending").expect("lending");
        let unresolved = lending.imports.iter().find(|i| i.alias == "elsewhere").expect("present");
        assert!(!unresolved.is_resolved());
    }

    // --- resolution ------------------------------------------------------

    #[test]
    fn a_cross_module_field_becomes_an_edge_into_the_other_module() {
        // The thing no other Allium tool draws.
        let mut graph = two_modules();
        link(&mut graph);
        let loan = NodeId::new("lending", NodeKind::Entity, "Loan");
        let edge = graph
            .edges_from(&loan)
            .find(|edge| edge.label == "copy")
            .expect("the copy field is an edge");
        assert_eq!(edge.to, NodeId::new("catalogue", NodeKind::Entity, "Copy"));
        assert_eq!(edge.kind, EdgeKind::Field);
    }

    #[test]
    fn a_built_in_typed_field_produces_no_edge_and_no_node() {
        let mut graph = two_modules();
        link(&mut graph);
        assert!(
            !graph.edges.iter().any(|edge| edge.label == "reference"),
            "a String field is not a relationship"
        );
        assert!(!graph.nodes.iter().any(|node| node.name == "String"));
    }

    #[test]
    fn an_alias_that_differs_from_the_module_name_still_resolves() {
        // `use "./catalogue.allium" as cat` — the alias is the author's choice
        // and assuming it matches the module name would break this.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("catalogue", Vec::new()));
        graph.modules.push(module("lending", vec![import("cat", "./catalogue.allium")]));
        graph.nodes.push(entity("catalogue", "Copy", Vec::new()));
        graph.nodes.push(entity("lending", "Loan", vec![EntityField::new("copy", "cat/Copy")]));
        link(&mut graph);

        let loan = NodeId::new("lending", NodeKind::Entity, "Loan");
        let edge = graph.edges_from(&loan).find(|edge| edge.label == "copy").expect("an edge");
        assert_eq!(edge.to, NodeId::new("catalogue", NodeKind::Entity, "Copy"));
    }

    #[test]
    fn a_reference_to_something_nothing_declares_becomes_a_visible_node() {
        // Dropping the edge would make the graph look complete when it is not.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("lending", Vec::new()));
        graph.nodes.push(entity("lending", "Loan", vec![EntityField::new("ghost", "Phantom")]));
        link(&mut graph);

        let external = NodeId::new("lending", NodeKind::External, "Phantom");
        let node = graph.node(&external).expect("an external node stands in for it");
        assert!(is_unresolved(node));
        assert!(graph.edges.iter().any(|edge| edge.to == external));
    }

    #[test]
    fn an_unresolved_alias_keeps_the_namespace_the_author_wrote() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("lending", Vec::new()));
        graph.nodes.push(entity("lending", "Loan", vec![EntityField::new("x", "other/Thing")]));
        link(&mut graph);
        assert!(
            graph.node(&NodeId::new("other", NodeKind::External, "Thing")).is_some(),
            "flattening it into `lending` would claim the spec said something it did not"
        );
    }

    #[test]
    fn one_external_node_serves_every_reference_to_it() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("lending", Vec::new()));
        graph.nodes.push(entity(
            "lending",
            "Loan",
            vec![EntityField::new("a", "Phantom"), EntityField::new("b", "Phantom")],
        ));
        link(&mut graph);
        let externals: Vec<_> = graph.nodes_of(NodeKind::External).collect();
        assert_eq!(externals.len(), 1);
    }

    #[test]
    fn a_self_referencing_field_draws_no_loop() {
        // `parents: Set<Message>` on `Message` is real and common; an arrow
        // from a node to itself adds nothing to read.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(entity("m", "Message", vec![EntityField::new("parents", "Set<Message>")]));
        link(&mut graph);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn a_derived_field_is_not_resolved_as_a_type() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        let mut derived = EntityField::new("copy_count", "copies.count");
        derived.derived = true;
        graph.nodes.push(entity("m", "Book", vec![derived]));
        link(&mut graph);
        assert!(graph.edges.is_empty());
        assert!(graph.nodes_of(NodeKind::External).next().is_none());
    }

    #[test]
    fn a_projection_still_resolves_because_it_navigates() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(entity("m", "Loan", Vec::new()));
        let mut projection = EntityField::new("open_loans", "Loan");
        projection.derived = true;
        projection.relationship = true;
        graph.nodes.push(entity("m", "Member", vec![projection]));
        link(&mut graph);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::Relationship);
    }

    // --- relocation ------------------------------------------------------

    #[test]
    fn a_trigger_emitted_here_and_declared_there_is_joined_up() {
        // The flow graph's whole reason to cross files.
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("catalogue", Vec::new()));
        graph.modules.push(module("lending", Vec::new()));
        graph.nodes.push(Node::new("catalogue", NodeKind::Trigger, "CopyLost"));
        graph.nodes.push(Node::new("lending", NodeKind::Rule, "ReportCopyLost"));
        graph.edges.push(Edge::new(
            NodeId::new("lending", NodeKind::Rule, "ReportCopyLost"),
            NodeId::new("lending", NodeKind::Trigger, "CopyLost"),
            EdgeKind::Emits,
            "CopyLost",
        ));
        link(&mut graph);

        let edge = graph.edges.iter().find(|edge| edge.kind == EdgeKind::Emits).expect("an edge");
        assert_eq!(edge.to, NodeId::new("catalogue", NodeKind::Trigger, "CopyLost"));
    }

    #[test]
    fn a_surface_facing_an_entity_rather_than_an_actor_finds_the_entity() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(entity("m", "Member", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Surface, "Shelf"));
        graph.edges.push(Edge::new(
            NodeId::new("m", NodeKind::Surface, "Shelf"),
            NodeId::new("m", NodeKind::Actor, "Member"),
            EdgeKind::Facing,
            "Member",
        ));
        link(&mut graph);

        let edge = graph.edges.iter().find(|edge| edge.kind == EdgeKind::Facing).expect("an edge");
        assert_eq!(edge.to, NodeId::new("m", NodeKind::Entity, "Member"));
    }

    #[test]
    fn an_edge_whose_target_exists_is_left_alone() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Actor, "Reader"));
        graph.nodes.push(Node::new("m", NodeKind::Surface, "Shelf"));
        let original = Edge::new(
            NodeId::new("m", NodeKind::Surface, "Shelf"),
            NodeId::new("m", NodeKind::Actor, "Reader"),
            EdgeKind::Facing,
            "Reader",
        );
        graph.edges.push(original.clone());
        link(&mut graph);
        assert_eq!(graph.edges, [original]);
    }

    #[test]
    fn an_edge_to_something_nothing_declares_gets_an_external_target() {
        let mut graph = SpecGraph::new("test");
        graph.modules.push(module("m", Vec::new()));
        graph.nodes.push(Node::new("m", NodeKind::Rule, "R"));
        graph.edges.push(Edge::new(
            NodeId::new("m", NodeKind::Rule, "R"),
            NodeId::new("m", NodeKind::Trigger, "NeverDeclared"),
            EdgeKind::Emits,
            "NeverDeclared",
        ));
        link(&mut graph);
        assert!(graph.node(&NodeId::new("m", NodeKind::External, "NeverDeclared")).is_some());
    }

    #[test]
    fn linking_leaves_the_graph_normalised() {
        let mut graph = two_modules();
        link(&mut graph);
        let mut expected = graph.clone();
        expected.normalise();
        assert_eq!(graph, expected);
    }

    #[test]
    fn linking_twice_changes_nothing_the_second_time() {
        // The server re-links on every file change, so a pass that accumulated
        // duplicate externals would grow the graph on every save.
        let mut once = two_modules();
        link(&mut once);
        let mut twice = once.clone();
        link(&mut twice);
        assert_eq!(once, twice);
    }
}
