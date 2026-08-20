//! The linked graph the whole UI is drawn from.
//!
//! One [`SpecGraph`] holds every module in the spec set, every construct in
//! them as a [`Node`], and every relationship between those constructs as an
//! [`Edge`]. The four view graphs the UI offers are projections of this one
//! structure rather than four separate ingestions, which is why adding a view
//! costs a filter rather than a parser.
//!
//! Two properties are load-bearing:
//!
//! *Everything is addressable.* A [`NodeId`] is `module::Kind::Name`, so a node
//! is nameable across module boundaries, stable across runs, and usable as a
//! key in a URL or a saved simulation.
//!
//! *Everything is ordered.* Nodes and edges are sorted before the graph is
//! handed out. The CLI's output order is not a contract, and a graph whose JSON
//! changes between identical runs cannot be snapshot-tested or diffed.

mod detail;

pub use detail::{
    ActorDetail, ConfigDetail, ConfigParameter, EntityDetail, EntityField, EntityKind, EnumDetail,
    InvariantDetail, NodeDetail, RuleClause, RuleDetail, SurfaceDetail, SurfaceOperation,
    TransitionEdge, TransitionGraph, TriggerDetail, TriggerSource,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    diagnostic::{Diagnostic, Finding},
    span::Span,
};

/// What kind of construct a node stands for.
///
/// The vocabulary follows the one the official VS Code extension settled on, so
/// anyone moving between the two tools reads the same words for the same
/// things. [`NodeKind::External`] is the addition: a reference that could not be
/// resolved becomes a visible node rather than a dropped edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum NodeKind {
    /// An entity with identity and a lifecycle.
    Entity,
    /// A value type: structured data compared by value, with no lifecycle.
    Value,
    /// A variant of a sum type.
    Variant,
    /// An enumeration.
    Enum,
    /// A rule: a trigger, its preconditions and its postconditions.
    Rule,
    /// A trigger: an external stimulus or a state condition.
    Trigger,
    /// A surface: what an actor can see and do at a boundary.
    Surface,
    /// An actor: who is on the other side of a surface.
    Actor,
    /// A configuration parameter block.
    Config,
    /// An invariant: something that must hold after every rule.
    Invariant,
    /// A reference to something no module in this set defines.
    External,
}

impl NodeKind {
    /// The word used in a [`NodeId`].
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Entity => "entity",
            NodeKind::Value => "value",
            NodeKind::Variant => "variant",
            NodeKind::Enum => "enum",
            NodeKind::Rule => "rule",
            NodeKind::Trigger => "trigger",
            NodeKind::Surface => "surface",
            NodeKind::Actor => "actor",
            NodeKind::Config => "config",
            NodeKind::Invariant => "invariant",
            NodeKind::External => "external",
        }
    }

    /// Whether this kind names a type that a field or parameter can refer to.
    ///
    /// Used when resolving a type expression: `Set<Book>` should find the
    /// entity `Book`, never the rule that happens to share its name.
    #[must_use]
    pub fn is_type(self) -> bool {
        matches!(
            self,
            NodeKind::Entity
                | NodeKind::Value
                | NodeKind::Variant
                | NodeKind::Enum
                | NodeKind::External
        )
    }
}

/// A node's address: `module::kind::Name`.
///
/// Stable across runs and unique across modules, so it can key a selection in a
/// URL, a saved simulation, or a diff between two versions of a spec.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct NodeId(pub String);

impl NodeId {
    /// The id for `name` of `kind` in `module`.
    #[must_use]
    pub fn new(module: &str, kind: NodeKind, name: &str) -> Self {
        Self(format!("{module}::{}::{name}", kind.as_str()))
    }

    /// The address as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The module part, or `""` when the id is malformed.
    #[must_use]
    pub fn module(&self) -> &str {
        self.0.split("::").next().unwrap_or_default()
    }

    /// The name part, or `""` when the id is malformed.
    ///
    /// Split from the right, because a name can itself contain the separator
    /// when a module is nested — the module is the first field, and everything
    /// after the kind is the name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.splitn(3, "::").nth(2).unwrap_or_default()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One construct in the spec set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// The bare name, as written: `Book`.
    pub name: String,
    /// The module that declares it.
    pub module: String,
    /// How the spec text refers to it across modules: `catalogue/Book`.
    pub qualified: String,
    /// Where it is declared, when the CLI reported a span.
    pub span: Option<Span>,
    /// Kind-specific payload.
    pub detail: NodeDetail,
}

impl Node {
    /// A node of `kind` named `name` in `module`, with no detail yet.
    #[must_use]
    pub fn new(module: &str, kind: NodeKind, name: &str) -> Self {
        Self {
            id: NodeId::new(module, kind, name),
            kind,
            name: name.to_owned(),
            module: module.to_owned(),
            qualified: format!("{module}/{name}"),
            span: None,
            detail: NodeDetail::None,
        }
    }

    /// The same node, declared at `span`.
    #[must_use]
    pub fn at(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    /// The same node, carrying `detail`.
    #[must_use]
    pub fn with(mut self, detail: NodeDetail) -> Self {
        self.detail = detail;
        self
    }
}

/// What relationship an edge stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum EdgeKind {
    /// An entity field typed as another construct.
    Field,
    /// A declared relationship between entities.
    Relationship,
    /// A trigger that fires a rule.
    Triggers,
    /// A rule that creates instances of an entity.
    Creates,
    /// A rule that assigns a field of an entity.
    Mutates,
    /// A rule that emits a trigger.
    Emits,
    /// A rule whose preconditions read an entity.
    Reads,
    /// A surface that exposes a field.
    Exposes,
    /// A surface that offers a trigger as an operation.
    Provides,
    /// A surface facing an actor.
    Facing,
    /// An actor identified by an entity.
    IdentifiedBy,
    /// An invariant constraining an entity.
    Constrains,
    /// A module importing another.
    Imports,
}

impl EdgeKind {
    /// The word used in the wire format and in edge ids.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Field => "field",
            EdgeKind::Relationship => "relationship",
            EdgeKind::Triggers => "triggers",
            EdgeKind::Creates => "creates",
            EdgeKind::Mutates => "mutates",
            EdgeKind::Emits => "emits",
            EdgeKind::Reads => "reads",
            EdgeKind::Exposes => "exposes",
            EdgeKind::Provides => "provides",
            EdgeKind::Facing => "facing",
            EdgeKind::IdentifiedBy => "identified_by",
            EdgeKind::Constrains => "constrains",
            EdgeKind::Imports => "imports",
        }
    }
}

/// A directed relationship between two nodes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// What to write on the edge: a field name, a state, an operation.
    pub label: String,
    /// Where the relationship is declared, when known.
    pub span: Option<Span>,
}

impl Edge {
    /// An edge of `kind` from `from` to `to`, labelled `label`.
    #[must_use]
    pub fn new(from: NodeId, to: NodeId, kind: EdgeKind, label: impl Into<String>) -> Self {
        Self { from, to, kind, label: label.into(), span: None }
    }

    /// The same edge, declared at `span`.
    #[must_use]
    pub fn at(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    /// Whether this edge connects the same two nodes the same way as `other`.
    ///
    /// Labels are excluded. Two fields of different names pointing at the same
    /// entity are one relationship to look at, and drawing both as separate
    /// arrows makes a dense graph unreadable for no added information.
    #[must_use]
    pub fn connects_like(&self, other: &Edge) -> bool {
        self.from == other.from && self.to == other.to && self.kind == other.kind
    }
}

/// One spec file's identity and its imports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Module {
    /// The module name: a spec file's stem, `catalogue`.
    pub name: String,
    /// The path it was read from, as given.
    pub path: String,
    /// Its `use` declarations.
    pub imports: Vec<Import>,
    /// The language version the file declares, when it declares one.
    pub language_version: Option<u32>,
}

/// One `use "./other.allium" as alias` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Import {
    /// The namespace the importing file uses: the `as` name.
    pub alias: String,
    /// The path as written in the spec.
    pub path: String,
    /// The module this resolved to, when it resolved to one in this set.
    pub target: Option<String>,
    pub span: Option<Span>,
}

impl Import {
    /// Whether the import found a module in this spec set.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.target.is_some()
    }
}

/// One test a construct owes, as `allium plan` derives it.
///
/// Shown against the node it belongs to, so a rule's card can say what would
/// have to be asserted for it to count as covered. The category is kept as the
/// CLI's own string rather than an enum: there are seventeen of them today, they
/// grow with releases, and none of this crate's behaviour branches on which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Obligation {
    /// The CLI's stable id, such as `rule-success.BorrowCopy`.
    pub id: String,
    /// The CLI's category, such as `rule_success` or `transition_edge`.
    pub category: String,
    pub description: String,
    /// What it is owed by: `BorrowCopy`, `Loan.status`.
    pub construct: String,
    pub module: String,
    pub span: Option<Span>,
}

/// Every module, construct and relationship in one spec set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct SpecGraph {
    /// The CLI version the documents were produced by.
    pub allium_version: String,
    pub modules: Vec<Module>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub diagnostics: Vec<Diagnostic>,
    pub findings: Vec<Finding>,
    pub obligations: Vec<Obligation>,
}

impl SpecGraph {
    /// An empty graph attributed to `allium_version`.
    #[must_use]
    pub fn new(allium_version: impl Into<String>) -> Self {
        Self {
            allium_version: allium_version.into(),
            modules: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
            findings: Vec::new(),
            obligations: Vec::new(),
        }
    }

    /// The node with `id`.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| &node.id == id)
    }

    /// Every node of `kind`.
    pub fn nodes_of(&self, kind: NodeKind) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(move |node| node.kind == kind)
    }

    /// Every edge leaving `id`.
    pub fn edges_from<'a>(&'a self, id: &'a NodeId) -> impl Iterator<Item = &'a Edge> {
        self.edges.iter().filter(move |edge| &edge.from == id)
    }

    /// Every edge arriving at `id`.
    pub fn edges_into<'a>(&'a self, id: &'a NodeId) -> impl Iterator<Item = &'a Edge> {
        self.edges.iter().filter(move |edge| &edge.to == id)
    }

    /// Every obligation owed by the construct named `construct`.
    ///
    /// Matched on the CLI's `source_construct`, which is a bare name for a rule
    /// and a dotted path for a field lifecycle (`Loan.status`), so a node asks
    /// with its own name and a lifecycle asks with `Entity.field`.
    pub fn obligations_for<'a>(
        &'a self,
        module: &'a str,
        construct: &'a str,
    ) -> impl Iterator<Item = &'a Obligation> {
        self.obligations.iter().filter(move |obligation| {
            obligation.module == module && obligation.construct == construct
        })
    }

    /// The highest severity reported against `module`, if anything was.
    #[must_use]
    pub fn worst_severity(&self, module: &str) -> Option<crate::diagnostic::Severity> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.module == module)
            .map(|diagnostic| diagnostic.severity)
            .max()
    }

    /// Sort and de-duplicate, so identical inputs produce an identical graph.
    ///
    /// Called once at the end of ingestion. The CLI's output order is not a
    /// contract and neither is the order modules were walked in, so nothing
    /// downstream — a snapshot test, a diff between two spec versions, a URL
    /// naming the third node — can depend on either until this has run.
    pub fn normalise(&mut self) {
        self.modules.sort_by(|a, b| a.name.cmp(&b.name));
        self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        self.nodes.dedup_by(|a, b| a.id == b.id);
        self.edges.sort();
        self.edges.dedup();
        self.obligations.sort_by(|a, b| a.id.cmp(&b.id));
        self.obligations.dedup_by(|a, b| a.id == b.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> SpecGraph {
        let mut graph = SpecGraph::new("allium 3.5.3");
        graph.nodes.push(Node::new("catalogue", NodeKind::Entity, "Book"));
        graph.nodes.push(Node::new("catalogue", NodeKind::Entity, "Copy"));
        graph.nodes.push(Node::new("catalogue", NodeKind::Rule, "AddBook"));
        graph.edges.push(Edge::new(
            NodeId::new("catalogue", NodeKind::Entity, "Copy"),
            NodeId::new("catalogue", NodeKind::Entity, "Book"),
            EdgeKind::Field,
            "book",
        ));
        graph
    }

    #[test]
    fn a_node_id_is_module_kind_and_name() {
        assert_eq!(
            NodeId::new("catalogue", NodeKind::Entity, "Book").as_str(),
            "catalogue::entity::Book"
        );
    }

    #[test]
    fn a_node_id_reports_its_parts() {
        let id = NodeId::new("catalogue", NodeKind::Rule, "AddBook");
        assert_eq!(id.module(), "catalogue");
        assert_eq!(id.name(), "AddBook");
    }

    #[test]
    fn a_node_id_name_survives_a_separator_inside_it() {
        // Splitting from the left with a fixed field count would truncate this
        // to "Odd", quietly renaming the node.
        let id = NodeId::new("m", NodeKind::Entity, "Odd::Name");
        assert_eq!(id.name(), "Odd::Name");
        assert_eq!(id.module(), "m");
    }

    #[test]
    fn a_malformed_node_id_reports_empty_parts_rather_than_panicking() {
        let id = NodeId("nonsense".to_owned());
        assert_eq!(id.name(), "");
        assert_eq!(id.module(), "nonsense");
    }

    #[test]
    fn a_node_id_displays_as_its_address() {
        assert_eq!(NodeId::new("m", NodeKind::Enum, "Medium").to_string(), "m::enum::Medium");
    }

    #[test]
    fn a_node_carries_its_cross_module_name() {
        let node = Node::new("catalogue", NodeKind::Entity, "Book");
        assert_eq!(node.qualified, "catalogue/Book", "how another module writes it");
        assert_eq!(node.name, "Book");
    }

    #[test]
    fn only_type_like_kinds_resolve_a_type_expression() {
        // `Set<Book>` must find the entity, never the rule of the same name.
        for kind in [NodeKind::Entity, NodeKind::Value, NodeKind::Variant, NodeKind::Enum] {
            assert!(kind.is_type(), "{kind:?} names a type");
        }
        for kind in [NodeKind::Rule, NodeKind::Trigger, NodeKind::Surface, NodeKind::Actor] {
            assert!(!kind.is_type(), "{kind:?} does not name a type");
        }
    }

    #[test]
    fn an_unresolved_reference_is_a_type_so_it_can_still_be_pointed_at() {
        assert!(NodeKind::External.is_type());
    }

    #[test]
    fn every_node_kind_has_a_distinct_word() {
        let kinds = [
            NodeKind::Entity,
            NodeKind::Value,
            NodeKind::Variant,
            NodeKind::Enum,
            NodeKind::Rule,
            NodeKind::Trigger,
            NodeKind::Surface,
            NodeKind::Actor,
            NodeKind::Config,
            NodeKind::Invariant,
            NodeKind::External,
        ];
        let mut words: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        let total = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), total, "two kinds share a word, so their ids would collide");
    }

    #[test]
    fn every_edge_kind_has_a_distinct_word() {
        let kinds = [
            EdgeKind::Field,
            EdgeKind::Relationship,
            EdgeKind::Triggers,
            EdgeKind::Creates,
            EdgeKind::Mutates,
            EdgeKind::Emits,
            EdgeKind::Reads,
            EdgeKind::Exposes,
            EdgeKind::Provides,
            EdgeKind::Facing,
            EdgeKind::IdentifiedBy,
            EdgeKind::Constrains,
            EdgeKind::Imports,
        ];
        let mut words: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        let total = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), total);
    }

    #[test]
    fn a_graph_finds_a_node_by_id() {
        let graph = graph();
        let id = NodeId::new("catalogue", NodeKind::Entity, "Book");
        assert_eq!(graph.node(&id).map(|node| node.name.as_str()), Some("Book"));
    }

    #[test]
    fn a_graph_reports_a_missing_node_as_none() {
        assert_eq!(graph().node(&NodeId::new("x", NodeKind::Entity, "Nope")), None);
    }

    #[test]
    fn nodes_of_filters_by_kind() {
        let graph = graph();
        let names: Vec<&str> =
            graph.nodes_of(NodeKind::Entity).map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Book", "Copy"]);
    }

    #[test]
    fn edges_from_and_into_are_directional() {
        let graph = graph();
        let copy = NodeId::new("catalogue", NodeKind::Entity, "Copy");
        let book = NodeId::new("catalogue", NodeKind::Entity, "Book");
        assert_eq!(graph.edges_from(&copy).count(), 1);
        assert_eq!(graph.edges_into(&copy).count(), 0);
        assert_eq!(graph.edges_from(&book).count(), 0);
        assert_eq!(graph.edges_into(&book).count(), 1);
    }

    #[test]
    fn connects_like_ignores_the_label() {
        let from = NodeId::new("m", NodeKind::Entity, "A");
        let to = NodeId::new("m", NodeKind::Entity, "B");
        let one = Edge::new(from.clone(), to.clone(), EdgeKind::Field, "first");
        let two = Edge::new(from.clone(), to, EdgeKind::Field, "second");
        assert!(one.connects_like(&two), "two fields to the same entity are one relationship");

        let elsewhere = Edge::new(from.clone(), from, EdgeKind::Field, "first");
        assert!(!one.connects_like(&elsewhere));
    }

    #[test]
    fn connects_like_distinguishes_the_kind() {
        let from = NodeId::new("m", NodeKind::Entity, "A");
        let to = NodeId::new("m", NodeKind::Entity, "B");
        let field = Edge::new(from.clone(), to.clone(), EdgeKind::Field, "x");
        let relationship = Edge::new(from, to, EdgeKind::Relationship, "x");
        assert!(!field.connects_like(&relationship));
    }

    #[test]
    fn normalise_orders_nodes_and_edges() {
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(Node::new("m", NodeKind::Entity, "Zebra"));
        graph.nodes.push(Node::new("m", NodeKind::Entity, "Aardvark"));
        graph.normalise();
        let names: Vec<&str> = graph.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Aardvark", "Zebra"]);
    }

    #[test]
    fn normalise_removes_duplicate_nodes_and_edges() {
        // The same entity is named by `model` and by `parse`, and the same
        // relationship can be reached from either end. Ingestion adds both and
        // lets this collapse them, rather than every call site checking first.
        let mut graph = SpecGraph::new("v");
        graph.nodes.push(Node::new("m", NodeKind::Entity, "Book"));
        graph.nodes.push(Node::new("m", NodeKind::Entity, "Book"));
        let edge = Edge::new(
            NodeId::new("m", NodeKind::Entity, "A"),
            NodeId::new("m", NodeKind::Entity, "B"),
            EdgeKind::Field,
            "x",
        );
        graph.edges.push(edge.clone());
        graph.edges.push(edge);
        graph.normalise();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn normalise_keeps_edges_that_differ_only_by_label() {
        // De-duplication here is exact. Collapsing by `connects_like` is a
        // *view* decision — the domain view merges them, the flow view wants
        // both — so the graph keeps them and the projections choose.
        let mut graph = SpecGraph::new("v");
        let from = NodeId::new("m", NodeKind::Entity, "A");
        let to = NodeId::new("m", NodeKind::Entity, "B");
        graph.edges.push(Edge::new(from.clone(), to.clone(), EdgeKind::Field, "first"));
        graph.edges.push(Edge::new(from, to, EdgeKind::Field, "second"));
        graph.normalise();
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn normalise_orders_modules_by_name() {
        let mut graph = SpecGraph::new("v");
        for name in ["lending", "catalogue"] {
            graph.modules.push(Module {
                name: name.to_owned(),
                path: format!("{name}.allium"),
                imports: Vec::new(),
                language_version: Some(3),
            });
        }
        graph.normalise();
        let names: Vec<&str> = graph.modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["catalogue", "lending"]);
    }

    #[test]
    fn normalise_is_idempotent() {
        let mut once = graph();
        once.normalise();
        let mut twice = once.clone();
        twice.normalise();
        assert_eq!(once, twice);
    }

    #[test]
    fn obligations_are_filtered_by_both_module_and_construct() {
        // Both halves matter. Matching on the construct alone attributes
        // another module's obligations to a rule that shares its name, and two
        // modules sharing a name is ordinary in a spec set.
        let mut graph = SpecGraph::new("v");
        for (module, construct, id) in [
            ("lending", "BorrowCopy", "a"),
            ("lending", "ReturnCopy", "b"),
            ("catalogue", "BorrowCopy", "c"),
        ] {
            graph.obligations.push(Obligation {
                id: id.to_owned(),
                category: "rule_success".to_owned(),
                description: String::new(),
                construct: construct.to_owned(),
                module: module.to_owned(),
                span: None,
            });
        }

        let owed: Vec<&str> = graph
            .obligations_for("lending", "BorrowCopy")
            .map(|obligation| obligation.id.as_str())
            .collect();
        assert_eq!(owed, ["a"], "not the other module's, and not the other rule's");
        assert_eq!(graph.obligations_for("lending", "Absent").count(), 0);
        assert_eq!(graph.obligations_for("absent", "BorrowCopy").count(), 0);
    }

    #[test]
    fn worst_severity_is_the_highest_reported_for_that_module() {
        use crate::diagnostic::Severity;
        let mut graph = SpecGraph::new("v");
        for (module, severity) in
            [("a", Severity::Info), ("a", Severity::Error), ("b", Severity::Warning)]
        {
            graph.diagnostics.push(Diagnostic {
                severity,
                message: "m".to_owned(),
                code: None,
                location: None,
                module: module.to_owned(),
                node: None,
            });
        }
        assert_eq!(graph.worst_severity("a"), Some(Severity::Error));
        assert_eq!(graph.worst_severity("b"), Some(Severity::Warning));
        assert_eq!(graph.worst_severity("c"), None, "a clean module reports nothing");
    }

    #[test]
    fn an_import_knows_whether_it_resolved() {
        let unresolved = Import {
            alias: "catalogue".to_owned(),
            path: "./catalogue.allium".to_owned(),
            target: None,
            span: None,
        };
        assert!(!unresolved.is_resolved());
        let resolved = Import { target: Some("catalogue".to_owned()), ..unresolved };
        assert!(resolved.is_resolved());
    }
}
