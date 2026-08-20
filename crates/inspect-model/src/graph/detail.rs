//! The kind-specific payload each node carries.
//!
//! A node's identity is uniform — id, kind, name, module, span — but what there
//! is to *say* about it is not. An entity has fields and a lifecycle; a rule has
//! a trigger and clauses; a surface has an actor and a list of operations. Those
//! live here, behind [`NodeDetail`], so the graph's own structure stays small
//! and a view can ask a node for exactly what it needs to draw.
//!
//! Everything is stored as the text the spec used. This tool shows a
//! specification; rewriting `attachment_size <= config.max_attachment_bytes`
//! into some normalised form would make the UI show something the author never
//! wrote, and the author is the person reading it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::span::Span;

/// Whether an entity is governed here, elsewhere, or has no lifecycle at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum EntityKind {
    /// Declared and governed by this spec set.
    Internal,
    /// Declared here, governed somewhere else.
    External,
    /// Structured data compared by value, with no identity or lifecycle.
    Value,
}

/// One field, projection or derived value on an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct EntityField {
    pub name: String,
    /// The type as written: `Set<Book>`, `catalogue/Copy`, `copies.count`.
    pub type_expr: String,
    /// The states, when the field is an inline enumeration.
    pub enum_values: Vec<String>,
    /// Whether the field is computed rather than stored.
    pub derived: bool,
    /// Whether the field navigates to related entities.
    pub relationship: bool,
    /// The states this field is only meaningful in, from a `when` qualifier.
    pub when: Option<String>,
}

impl EntityField {
    /// A plain stored field of `name` typed `type_expr`.
    #[must_use]
    pub fn new(name: impl Into<String>, type_expr: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_expr: type_expr.into(),
            enum_values: Vec::new(),
            derived: false,
            relationship: false,
            when: None,
        }
    }

    /// Whether the field declares an inline set of states.
    #[must_use]
    pub fn is_status(&self) -> bool {
        !self.enum_values.is_empty()
    }
}

/// One `from -> to` step in a lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct TransitionEdge {
    pub from: String,
    pub to: String,
}

/// An entity's declared lifecycle for one status field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct TransitionGraph {
    /// The field the lifecycle governs, usually `status`.
    pub field: String,
    pub states: Vec<String>,
    pub edges: Vec<TransitionEdge>,
    /// States from which nothing further may happen.
    pub terminal: Vec<String>,
}

impl TransitionGraph {
    /// Whether `state` is declared terminal.
    #[must_use]
    pub fn is_terminal(&self, state: &str) -> bool {
        self.terminal.iter().any(|terminal| terminal == state)
    }

    /// Whether the lifecycle permits moving from `from` to `to`.
    ///
    /// The question the simulator asks before writing a status field. A move the
    /// lifecycle does not declare is an error to report, not a value to store —
    /// silently writing it would make the simulator disagree with the spec it
    /// exists to demonstrate.
    #[must_use]
    pub fn allows(&self, from: &str, to: &str) -> bool {
        self.edges.iter().any(|edge| edge.from == from && edge.to == to)
    }

    /// The states reachable in one step from `state`.
    pub fn successors<'a>(&'a self, state: &'a str) -> impl Iterator<Item = &'a str> {
        self.edges.iter().filter(move |edge| edge.from == state).map(|edge| edge.to.as_str())
    }
}

/// An entity, value type or variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct EntityDetail {
    pub kind: EntityKind,
    pub fields: Vec<EntityField>,
    pub transitions: Vec<TransitionGraph>,
    /// The sum type this is a variant of, when it is one.
    pub parent: Option<String>,
}

impl EntityDetail {
    /// The field named `name`.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&EntityField> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The lifecycle governing `field`.
    #[must_use]
    pub fn transitions_for(&self, field: &str) -> Option<&TransitionGraph> {
        self.transitions.iter().find(|graph| graph.field == field)
    }
}

/// An enumeration and its values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct EnumDetail {
    pub values: Vec<String>,
}

/// One configuration parameter and its default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct ConfigParameter {
    pub name: String,
    pub type_expr: String,
    /// The default as written: `21.days`, `20`, `false`.
    pub default_expr: Option<String>,
}

/// A module's configuration block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct ConfigDetail {
    pub parameters: Vec<ConfigParameter>,
}

/// What makes a trigger happen.
///
/// The distinction the simulator is built on. An external stimulus is something
/// a person or another system does, so the user fires it. A state condition is
/// something that *becomes* true, so the simulator offers it as a next step once
/// a change makes it hold — and walking that chain is as close to a user journey
/// as a spec without journeys can get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum TriggerSource {
    /// An action from outside the system.
    External,
    /// A condition over entity state that has become true.
    State,
    /// A condition over state and the clock.
    Temporal,
}

/// A trigger: what a rule waits for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct TriggerDetail {
    pub source: TriggerSource,
    /// Parameter names, for an external stimulus.
    pub parameters: Vec<String>,
    /// The condition as written, for a state or temporal trigger.
    pub condition: Option<String>,
    /// The entity a state condition is bound over.
    pub entity: Option<String>,
}

/// One `when`, `requires` or `ensures` clause of a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct RuleClause {
    /// `when`, `requires` or `ensures`.
    pub keyword: String,
    /// The clause as the spec wrote it.
    pub text: String,
    pub span: Option<Span>,
}

/// A rule: a trigger, preconditions and postconditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct RuleDetail {
    /// The trigger's name.
    pub trigger: String,
    pub source: TriggerSource,
    pub clauses: Vec<RuleClause>,
    /// Entities the rule creates instances of.
    pub creates: Vec<String>,
    /// Triggers the rule emits.
    pub emits: Vec<String>,
}

impl RuleDetail {
    /// The clauses with `keyword`.
    pub fn clauses_of<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a RuleClause> {
        self.clauses.iter().filter(move |clause| clause.keyword == keyword)
    }
}

/// An invariant: something that must hold after every rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct InvariantDetail {
    /// The condition as written, absent for a prose-only invariant.
    pub expression: Option<String>,
    /// Entities the invariant constrains.
    pub entities: Vec<String>,
}

impl InvariantDetail {
    /// Whether the invariant states a checkable condition.
    ///
    /// A prose-only invariant is a real part of the spec and is shown as such,
    /// but nothing can evaluate it. The simulator must say so rather than
    /// silently reporting it as holding, which would be a claim nothing checked.
    #[must_use]
    pub fn is_checkable(&self) -> bool {
        self.expression.as_ref().is_some_and(|text| !text.trim().is_empty())
    }
}

/// One operation a surface offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct SurfaceOperation {
    /// The trigger it fires.
    pub trigger: String,
    pub parameters: Vec<String>,
    /// The guard that decides whether it is offered.
    pub when: Option<String>,
}

/// A surface: what an actor can see and do at a boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct SurfaceDetail {
    /// The actor or entity type on the other side.
    pub actor: Option<String>,
    /// The binding name the surface gave that actor.
    pub actor_binding: Option<String>,
    /// The entity the surface is scoped to.
    pub context: Option<String>,
    /// Fields visible across the boundary, as written.
    pub exposes: Vec<String>,
    pub provides: Vec<SurfaceOperation>,
    /// Named `@guarantee` constraints, which are prose.
    pub guarantees: Vec<String>,
}

/// An actor: who is on the other side of a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct ActorDetail {
    /// The entity type the actor is an instance of.
    pub entity: Option<String>,
    /// The condition that identifies them, as written.
    pub condition: Option<String>,
    /// The context type the actor requires from a surface.
    pub within: Option<String>,
}

/// The kind-specific payload of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum NodeDetail {
    Entity(EntityDetail),
    Enum(EnumDetail),
    Config(ConfigDetail),
    Trigger(TriggerDetail),
    Rule(RuleDetail),
    Invariant(InvariantDetail),
    Surface(SurfaceDetail),
    Actor(ActorDetail),
    /// Nothing beyond the node's identity — an unresolved reference, or a
    /// construct whose payload the CLI did not report.
    None,
}

impl NodeDetail {
    /// The entity payload, when this is one.
    #[must_use]
    pub fn as_entity(&self) -> Option<&EntityDetail> {
        match self {
            NodeDetail::Entity(detail) => Some(detail),
            _ => None,
        }
    }

    /// The rule payload, when this is one.
    #[must_use]
    pub fn as_rule(&self) -> Option<&RuleDetail> {
        match self {
            NodeDetail::Rule(detail) => Some(detail),
            _ => None,
        }
    }

    /// The trigger payload, when this is one.
    #[must_use]
    pub fn as_trigger(&self) -> Option<&TriggerDetail> {
        match self {
            NodeDetail::Trigger(detail) => Some(detail),
            _ => None,
        }
    }

    /// The surface payload, when this is one.
    #[must_use]
    pub fn as_surface(&self) -> Option<&SurfaceDetail> {
        match self {
            NodeDetail::Surface(detail) => Some(detail),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle() -> TransitionGraph {
        TransitionGraph {
            field: "status".to_owned(),
            states: ["available", "on_loan", "lost"].map(ToOwned::to_owned).to_vec(),
            edges: vec![
                TransitionEdge { from: "available".to_owned(), to: "on_loan".to_owned() },
                TransitionEdge { from: "on_loan".to_owned(), to: "available".to_owned() },
                TransitionEdge { from: "on_loan".to_owned(), to: "lost".to_owned() },
            ],
            terminal: vec!["lost".to_owned()],
        }
    }

    #[test]
    fn a_plain_field_is_neither_derived_nor_a_relationship() {
        let field = EntityField::new("title", "String");
        assert!(!field.derived);
        assert!(!field.relationship);
        assert!(!field.is_status());
        assert_eq!(field.when, None);
    }

    #[test]
    fn a_field_with_inline_states_is_a_status() {
        let mut field = EntityField::new("status", "listed | withdrawn");
        field.enum_values = ["listed", "withdrawn"].map(ToOwned::to_owned).to_vec();
        assert!(field.is_status());
    }

    #[test]
    fn a_lifecycle_permits_only_declared_moves() {
        let graph = lifecycle();
        assert!(graph.allows("available", "on_loan"));
        assert!(graph.allows("on_loan", "lost"));
        // Declared as a state, but not reachable from `available` in one step.
        assert!(!graph.allows("available", "lost"));
        assert!(!graph.allows("lost", "available"), "nothing leaves a terminal state");
    }

    #[test]
    fn a_lifecycle_rejects_a_move_between_states_it_does_not_know() {
        let graph = lifecycle();
        assert!(!graph.allows("available", "incinerated"));
        assert!(!graph.allows("nowhere", "on_loan"));
    }

    #[test]
    fn a_lifecycle_reports_its_terminal_states() {
        let graph = lifecycle();
        assert!(graph.is_terminal("lost"));
        assert!(!graph.is_terminal("available"));
        assert!(!graph.is_terminal("unheard_of"));
    }

    #[test]
    fn successors_are_the_one_step_moves() {
        let graph = lifecycle();
        let mut next: Vec<&str> = graph.successors("on_loan").collect();
        next.sort_unstable();
        assert_eq!(next, ["available", "lost"]);
        assert_eq!(graph.successors("lost").count(), 0);
    }

    #[test]
    fn an_entity_finds_its_field_and_its_lifecycle() {
        let detail = EntityDetail {
            kind: EntityKind::Internal,
            fields: vec![EntityField::new("title", "String")],
            transitions: vec![lifecycle()],
            parent: None,
        };
        assert_eq!(detail.field("title").map(|f| f.type_expr.as_str()), Some("String"));
        assert_eq!(detail.field("absent"), None);
        assert!(detail.transitions_for("status").is_some());
        assert!(detail.transitions_for("title").is_none());
    }

    #[test]
    fn rule_clauses_filter_by_keyword() {
        let detail = RuleDetail {
            trigger: "MemberBorrows".to_owned(),
            source: TriggerSource::External,
            clauses: vec![
                RuleClause {
                    keyword: "when".to_owned(),
                    text: "MemberBorrows(m, c)".to_owned(),
                    span: None,
                },
                RuleClause {
                    keyword: "requires".to_owned(),
                    text: "c.status = available".to_owned(),
                    span: None,
                },
                RuleClause {
                    keyword: "requires".to_owned(),
                    text: "not m.is_at_limit".to_owned(),
                    span: None,
                },
                RuleClause {
                    keyword: "ensures".to_owned(),
                    text: "c.status = on_loan".to_owned(),
                    span: None,
                },
            ],
            creates: vec!["Loan".to_owned()],
            emits: vec!["CopyBorrowed".to_owned()],
        };
        assert_eq!(detail.clauses_of("requires").count(), 2);
        assert_eq!(detail.clauses_of("ensures").count(), 1);
        assert_eq!(detail.clauses_of("nonsense").count(), 0);
    }

    #[test]
    fn an_invariant_with_an_expression_is_checkable() {
        let detail = InvariantDetail {
            expression: Some("m.open_loan_count <= config.loan_limit".to_owned()),
            entities: vec!["Member".to_owned()],
        };
        assert!(detail.is_checkable());
    }

    #[test]
    fn a_prose_only_invariant_is_not_checkable() {
        // It is still part of the spec and still shown. What must not happen is
        // the simulator reporting it as *holding* — that would be a claim
        // nothing checked.
        let detail = InvariantDetail { expression: None, entities: vec!["Member".to_owned()] };
        assert!(!detail.is_checkable());
    }

    #[test]
    fn an_invariant_whose_expression_is_blank_is_not_checkable() {
        let detail =
            InvariantDetail { expression: Some("   \n ".to_owned()), entities: Vec::new() };
        assert!(!detail.is_checkable(), "whitespace is not a condition");
    }

    #[test]
    fn detail_accessors_return_only_their_own_variant() {
        let entity = NodeDetail::Entity(EntityDetail {
            kind: EntityKind::Value,
            fields: Vec::new(),
            transitions: Vec::new(),
            parent: None,
        });
        assert!(entity.as_entity().is_some());
        assert!(entity.as_rule().is_none());
        assert!(entity.as_trigger().is_none());
        assert!(entity.as_surface().is_none());

        assert!(NodeDetail::None.as_entity().is_none());
    }

    #[test]
    fn a_trigger_records_how_it_happens() {
        let external = TriggerDetail {
            source: TriggerSource::External,
            parameters: ["member", "copy"].map(ToOwned::to_owned).to_vec(),
            condition: None,
            entity: None,
        };
        assert_eq!(external.source, TriggerSource::External);
        assert_eq!(external.parameters.len(), 2);

        let temporal = TriggerDetail {
            source: TriggerSource::Temporal,
            parameters: Vec::new(),
            condition: Some("Loan.window.due_at <= now".to_owned()),
            entity: Some("Loan".to_owned()),
        };
        assert_eq!(temporal.entity.as_deref(), Some("Loan"));
    }
}
