//! The simulator's routes.
//!
//! Stateless by design: the browser holds the world and posts it back with each
//! event, and the server answers with the world that resulted. There are no
//! sessions to expire, nothing to clean up, two people can drive two
//! simulations of the same spec at once, and a whole run is a value the client
//! can keep, undo through, or put in a URL.
//!
//! The expression trees never cross the wire in either direction. They are an
//! order of magnitude larger than the graph and only this side evaluates them,
//! so what travels is a world in and a step out.

use axum::{Json, extract::State};
use inspect_model::{NodeKind, graph::TriggerSource};
use inspect_sim::{StepOutcome, World, seed::seed, step::step, world::Event};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// What the browser posts to take a step.
#[derive(Debug, Deserialize)]
pub struct StepRequest {
    pub world: World,
    pub event: Event,
}

/// A trigger the user can fire, and where it comes from.
#[derive(Debug, Serialize)]
pub struct Fireable {
    pub trigger: String,
    pub module: String,
    /// The parameters it carries, so the form knows what to ask for.
    pub parameters: Vec<String>,
    /// The surface that offers it, when one does.
    ///
    /// A trigger a surface provides is something a *person* does, and those are
    /// the honest starting points for a simulation. One with no surface is
    /// still fireable — a spec may emit it from elsewhere — but it is listed
    /// second, because starting there means starting in the middle.
    pub surface: Option<String>,
    /// The actor the surface faces, when it names one.
    pub actor: Option<String>,
}

/// Everything a fresh simulation needs.
#[derive(Debug, Serialize)]
pub struct Setup {
    /// A world with the spec's configuration defaults already in force.
    pub world: World,
    /// Entity types the user can create instances of.
    pub entities: Vec<EntityChoice>,
    /// Triggers the user can fire.
    pub triggers: Vec<Fireable>,
}

/// An entity the world editor can make instances of.
#[derive(Debug, Serialize)]
pub struct EntityChoice {
    pub entity: String,
    pub module: String,
    /// Field names and the states each may take, for the editor's inputs.
    pub fields: Vec<FieldChoice>,
}

#[derive(Debug, Serialize)]
pub struct FieldChoice {
    pub name: String,
    pub type_expr: String,
    pub states: Vec<String>,
    /// Derived fields are computed by the spec and not by this simulator, so
    /// the editor marks them: leaving one unset is why a rule reading it comes
    /// back undecided.
    pub derived: bool,
}

/// `POST /api/sim/step` — fire one trigger.
pub async fn take_step(
    State(state): State<AppState>,
    Json(request): Json<StepRequest>,
) -> Json<StepOutcome> {
    let inspection = state.get();
    Json(step(
        &inspection.graph,
        &inspection.program,
        inspection.sources_by_module(),
        &request.world,
        &request.event,
    ))
}

/// `GET /api/sim/setup` — a seeded world and what can be done to it.
pub async fn setup(State(state): State<AppState>) -> Json<Setup> {
    let inspection = state.get();
    let graph = &inspection.graph;

    let entities = graph
        .nodes_of(NodeKind::Entity)
        .filter_map(|node| {
            let detail = node.detail.as_entity()?;
            Some(EntityChoice {
                entity: node.name.clone(),
                module: node.module.clone(),
                fields: detail
                    .fields
                    .iter()
                    .map(|field| FieldChoice {
                        name: field.name.clone(),
                        type_expr: field.type_expr.clone(),
                        states: field.enum_values.clone(),
                        derived: field.derived,
                    })
                    .collect(),
            })
        })
        .collect();

    Json(Setup { world: seed(graph), entities, triggers: fireable(graph) })
}

/// Every trigger the user can fire, surfaces first.
fn fireable(graph: &inspect_model::SpecGraph) -> Vec<Fireable> {
    let mut offered: Vec<Fireable> = Vec::new();

    for surface in graph.nodes_of(NodeKind::Surface) {
        let Some(detail) = surface.detail.as_surface() else { continue };
        for operation in &detail.provides {
            offered.push(Fireable {
                trigger: operation.trigger.clone(),
                module: surface.module.clone(),
                parameters: operation.parameters.clone(),
                surface: Some(surface.name.clone()),
                actor: detail.actor.clone(),
            });
        }
    }

    // Then any external trigger no surface offers. A spec may emit one from a
    // rule, and refusing to let the user fire it would make part of the system
    // unreachable in the simulator for no reason the reader would recognise.
    for trigger in graph.nodes_of(NodeKind::Trigger) {
        let Some(detail) = trigger.detail.as_trigger() else { continue };
        if detail.source != TriggerSource::External {
            continue;
        }
        if offered.iter().any(|already| already.trigger == trigger.name) {
            continue;
        }
        offered.push(Fireable {
            trigger: trigger.name.clone(),
            module: trigger.module.clone(),
            parameters: detail.parameters.clone(),
            surface: None,
            actor: None,
        });
    }

    offered
}

#[cfg(test)]
mod tests {
    use inspect_model::{
        Node, NodeDetail, NodeKind, SpecGraph,
        graph::{
            ConfigDetail, ConfigParameter, EntityDetail, EntityField, EntityKind, SurfaceDetail,
            SurfaceOperation, TriggerDetail, TriggerSource,
        },
    };

    use super::*;

    /// A spec with one surface offering one operation, one trigger nothing
    /// offers, one entity and one config block.
    fn spec() -> SpecGraph {
        let mut graph = SpecGraph::new("test");

        graph.nodes.push(Node::new("lending", NodeKind::Surface, "MemberShelf").with(
            NodeDetail::Surface(SurfaceDetail {
                actor: Some("Reader".to_owned()),
                actor_binding: Some("reader".to_owned()),
                context: None,
                exposes: Vec::new(),
                provides: vec![SurfaceOperation {
                    trigger: "MemberBorrows".to_owned(),
                    parameters: vec!["reader".to_owned(), "copy".to_owned()],
                    when: None,
                }],
                guarantees: Vec::new(),
            }),
        ));

        for (name, source) in [
            ("MemberBorrows", TriggerSource::External),
            ("SomethingElse", TriggerSource::External),
            ("Copy", TriggerSource::State),
        ] {
            graph.nodes.push(Node::new("lending", NodeKind::Trigger, name).with(
                NodeDetail::Trigger(TriggerDetail {
                    source,
                    parameters: vec!["it".to_owned()],
                    condition: None,
                    entity: None,
                }),
            ));
        }

        let mut status = EntityField::new("status", "open | returned");
        status.enum_values = vec!["open".to_owned(), "returned".to_owned()];
        let mut derived = EntityField::new("is_late", "due_at <= now");
        derived.derived = true;
        graph.nodes.push(Node::new("lending", NodeKind::Entity, "Loan").with(NodeDetail::Entity(
            EntityDetail {
                kind: EntityKind::Internal,
                fields: vec![EntityField::new("member", "Member"), status, derived],
                transitions: Vec::new(),
                parent: None,
            },
        )));

        graph.nodes.push(Node::new("lending", NodeKind::Config, "config").with(
            NodeDetail::Config(ConfigDetail {
                parameters: vec![ConfigParameter {
                    name: "loan_limit".to_owned(),
                    type_expr: "Integer".to_owned(),
                    default_expr: Some("5".to_owned()),
                }],
            }),
        ));

        graph.normalise();
        graph
    }

    #[test]
    fn a_surface_operation_is_offered_with_the_actor_who_performs_it() {
        let offered = fireable(&spec());
        let borrows = offered.iter().find(|f| f.trigger == "MemberBorrows").expect("offered");
        assert_eq!(borrows.surface.as_deref(), Some("MemberShelf"));
        assert_eq!(borrows.actor.as_deref(), Some("Reader"));
        assert_eq!(borrows.parameters, ["reader", "copy"]);
    }

    #[test]
    fn surface_operations_come_before_triggers_nothing_offers() {
        // Starting from a surface is starting where a person would. Starting
        // anywhere else is starting in the middle, so it is listed second.
        let offered = fireable(&spec());
        let first_without_surface =
            offered.iter().position(|f| f.surface.is_none()).unwrap_or(offered.len());
        let last_with_surface =
            offered.iter().rposition(|f| f.surface.is_some()).unwrap_or_default();
        assert!(last_with_surface < first_without_surface);
    }

    #[test]
    fn a_trigger_no_surface_offers_is_still_fireable() {
        // A spec may emit one from a rule. Refusing to let the user fire it
        // would make part of the system unreachable for no reason a reader
        // would recognise.
        let offered = fireable(&spec());
        assert!(offered.iter().any(|f| f.trigger == "SomethingElse" && f.surface.is_none()));
    }

    #[test]
    fn a_state_condition_trigger_is_not_offered_as_something_to_fire() {
        // Nobody fires one: the world becoming true is what runs it, and the
        // step reports it as newly enabled instead.
        let offered = fireable(&spec());
        assert!(!offered.iter().any(|f| f.trigger == "Copy"));
    }

    #[test]
    fn a_trigger_offered_twice_is_listed_once() {
        let mut graph = spec();
        graph.nodes.push(Node::new("lending", NodeKind::Surface, "Second").with(
            NodeDetail::Surface(SurfaceDetail {
                actor: None,
                actor_binding: None,
                context: None,
                exposes: Vec::new(),
                provides: vec![SurfaceOperation {
                    trigger: "SomethingElse".to_owned(),
                    parameters: Vec::new(),
                    when: None,
                }],
                guarantees: Vec::new(),
            }),
        ));
        graph.normalise();
        let offered = fireable(&graph);
        let count = offered.iter().filter(|f| f.trigger == "SomethingElse").count();
        assert_eq!(count, 1, "the surface's listing wins over the bare trigger");
    }
}
