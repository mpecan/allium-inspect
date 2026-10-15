//! A creation as a trigger: `when: o: Order.created`.
//!
//! The language lists entity creation as a trigger kind of its own, and it is
//! not a state condition, although it is written like one. A state condition is
//! about the world — `Copy.status = lost` holds of every lost copy for as long
//! as it stays lost — while a creation is something that *happened*, once, to
//! one instance. An order that exists has not thereby just been created.
//!
//! Read as a condition, `Order.created` asked each order for a field called
//! `created`, which nothing sets, so the rule never fired; and since a state
//! rule's trigger is its entity, every firing about an order woke it to report
//! itself undecided. So it is neither: a creation rule is woken by its own
//! event, `Order.created`, carrying the instance that was made, and by nothing
//! else.
//!
//! The step that creates is still one step. What it made is offered among
//! `newly_enabled`, the same way a rule the world has made true is, and
//! [`reactions`] turns that into the events to fire — which is what a journey
//! does, because it has already said the act happened.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;
use inspect_model::{NodeKind, Program, SpecGraph};

use crate::{
    apply::Effect,
    eval::{Evaluation, Unresolved},
    step::{Enabled, RuleOutcome, StepOutcome},
    truth::Truth,
    value::{EntityId, Value},
    world::{Event, World},
};

/// What `.created` is spelled as, on the end of an entity's name.
const CREATED: &str = ".created";

/// The trigger a creation of `entity` fires: `Order.created`.
///
/// Spelled the way the spec spells the clause, so a reader who sees it in the
/// trace recognises it, and so it can never collide with a state rule's
/// trigger — that is an entity's bare name, which holds no dot.
#[must_use]
pub fn created_trigger(entity: &str) -> String {
    format!("{entity}{CREATED}")
}

/// Whether `trigger` is a creation's, as [`created_trigger`] spells one.
#[must_use]
pub fn is_created_trigger(trigger: &str) -> bool {
    trigger.ends_with(CREATED)
}

/// The binding and entity of a creation rule's `when`, or `None` for any other.
///
/// `o: Order.created`, and `note: memo/Note.created` across a module
/// boundary. The qualifier is dropped here, because instances are held by
/// entity name; which module's entity it means is `reacts_in`'s to say.
#[must_use]
pub fn creation_of(when: Option<&Expr>) -> Option<(&str, &str)> {
    let Some(Expr::Binding { name, value, .. }) = when else { return None };
    let Expr::MemberAccess { object, field, .. } = value.as_ref() else { return None };
    if field.name != "created" {
        return None;
    }
    let entity = match object.as_ref() {
        Expr::Ident(ident) => ident.name.as_str(),
        Expr::QualifiedName(qualified) => qualified.name.as_str(),
        _ => return None,
    };
    Some((name.name.as_str(), entity))
}

/// The module whose `entity` a name written in `from` means, when it means one.
///
/// `memo/Note` is whatever `from` imported `as memo`, and the alias need not be
/// that module's name. A bare `Order` is `from`'s own. Either way only if that
/// module does declare the entity: `None` is the honest answer when the name
/// leads nowhere, and what to do about that is the caller's to say.
pub(crate) fn declared_in(
    spec: &SpecGraph,
    from: &str,
    alias: Option<&str>,
    entity: &str,
) -> Option<String> {
    let module = match alias {
        None => from.to_owned(),
        Some(alias) => spec
            .modules
            .iter()
            .find(|module| module.name == from)?
            .imports
            .iter()
            .find(|import| import.alias == alias)?
            .target
            .clone()?,
    };
    let declares = spec
        .nodes
        .iter()
        .any(|node| node.kind == NodeKind::Entity && node.name == entity && node.module == module);
    declares.then_some(module)
}

/// The module whose instances a creation rule in `module` reacts to.
///
/// Two modules may each declare an `Order`, and a rule written against one of
/// them is not about the other's being made: the instance carries its module,
/// and this is the module it has to carry. `None` when the rule's name for the
/// entity leads to no module that declares it.
pub(crate) fn reacts_in(spec: &SpecGraph, module: &str, when: Option<&Expr>) -> Option<String> {
    let Some(Expr::Binding { value, .. }) = when else { return None };
    let Expr::MemberAccess { object, .. } = value.as_ref() else { return None };
    match object.as_ref() {
        Expr::Ident(ident) => declared_in(spec, module, None, &ident.name),
        Expr::QualifiedName(name) => {
            declared_in(spec, module, name.qualifier.as_deref(), &name.name)
        }
        _ => None,
    }
}

/// Whether `event` wakes a rule waiting on `trigger` with this `when`.
///
/// A creation rule only by its creation; every other rule by its trigger's
/// name, as before. The second half is what keeps a state rule's firing about
/// an order from reaching a rule that reacts to orders being made.
pub(crate) fn wakes(trigger: &str, when: Option<&Expr>, event: &Event) -> bool {
    match creation_of(when) {
        Some((_, entity)) => event.trigger == created_trigger(entity),
        None => trigger == event.trigger,
    }
}

/// A creation rule's `when`, as a precondition: was this firing the creation of
/// an instance of its entity, from the module the rule means?
///
/// True when the binding holds one. Otherwise undecided, and it says why
/// naming the clause: when the firing carried nothing (it was fired by hand),
/// when what it carried is some other entity, or when it is an `Order` from a
/// module the rule's `Order` is not. Which instance was created is not
/// something to guess, and there may be any number of orders in the world.
pub(crate) fn when_verdict(
    (binding, entity, home): (&str, &str, Option<&str>),
    when: &Expr,
    bindings: &BTreeMap<String, Value>,
    world: &World,
    source: &str,
) -> (Truth, Vec<Unresolved>) {
    let instance = match bindings.get(binding) {
        Some(Value::Ref(id)) => world.instance(id).filter(|it| it.entity == entity),
        _ => None,
    };
    let reason = match (instance, home) {
        (Some(it), Some(home)) if it.module == home => return (Truth::True, Vec::new()),
        (Some(it), Some(home)) => format!(
            "`{binding}` is `{}` from `{}`, and this rule reacts to `{entity}.created` in `{home}`",
            it.id.0, it.module
        ),
        (Some(_), None) => format!(
            "this rule's `{entity}` names no module that declares one, so which `{entity}` \
             being created it reacts to is not known"
        ),
        (None, _) => format!(
            "nothing is bound to `{binding}`: this rule reacts to `{entity}.created`, and this \
             firing carried no `{entity}` that was created"
        ),
    };
    let unresolved = Evaluation::unknown(reason, when, source).unresolved;
    (Truth::Unknown, unresolved)
}

/// The creation rules woken by what `rules` created, one entry per rule, over
/// the instances it is about.
///
/// Only instances still in `world` afterwards: one created and removed in the
/// same step was never there for anything to react to.
pub(crate) fn reacting(
    spec: &SpecGraph,
    program: &Program,
    rules: &[RuleOutcome],
    world: &World,
) -> Vec<Enabled> {
    let created: Vec<&EntityId> = rules
        .iter()
        .flat_map(|rule| &rule.effects)
        .filter_map(|effect| match effect {
            Effect::Created { id, .. } if world.instance(id).is_some() => Some(id),
            _ => None,
        })
        .collect();
    if created.is_empty() {
        return Vec::new();
    }

    let mut found = Vec::new();
    for node in spec.nodes_of(NodeKind::Rule) {
        let Some(detail) = node.detail.as_rule() else { continue };
        let Some(ast) = program.rule(node.id.as_str()) else { continue };
        let Some((binding, entity)) = creation_of(ast.when.as_ref()) else { continue };
        // The rule's own `Order`, not every module's: a creation in a module
        // the rule's name does not lead to is not one it reacts to. A name
        // that leads nowhere is still offered, and its firing says so.
        let home = reacts_in(spec, &node.module, ast.when.as_ref());
        let over: Vec<Value> = created
            .iter()
            .filter(|id| id.entity() == entity)
            .filter(|id| {
                home.as_ref().is_none_or(|home| {
                    world.instance(id).is_some_and(|instance| instance.module == *home)
                })
            })
            .map(|id| Value::Ref((*id).clone()))
            .collect();
        if over.is_empty() {
            continue;
        }
        found.push(Enabled {
            rule: node.id.as_str().to_owned(),
            name: node.name.clone(),
            module: node.module.clone(),
            trigger: created_trigger(entity),
            source: detail.source,
            over,
            binding: binding.to_owned(),
            undecided: Vec::new(),
        });
    }
    found
}

/// The creation events a step's outcome calls for, one per instance created.
///
/// One per *instance*, not per rule: the event reaches every rule reacting to
/// that entity, so firing it once per rule would run each of them once for
/// every other. Each rule's binding name is carried, because two rules may call
/// the same order `o` and `order`. In the order the instances were offered, so
/// the same outcome always asks for the same firings.
#[must_use]
pub fn reactions(outcome: &StepOutcome) -> Vec<Event> {
    let mut events: Vec<(Value, Event)> = Vec::new();
    for rule in &outcome.newly_enabled {
        if !is_created_trigger(&rule.trigger) {
            continue;
        }
        for instance in &rule.over {
            // `reacting` offers only references to instances still in the
            // world it hands back, so both of these always hold; the event is
            // filed in the module the instance is from.
            let Value::Ref(id) = instance else { continue };
            let Some(made) = outcome.world.instance(id) else { continue };
            let at = match events.iter().position(|(seen, _)| seen == instance) {
                Some(at) => at,
                None => {
                    events.push((instance.clone(), Event::new(&rule.trigger, &made.module)));
                    events.len() - 1
                }
            };
            events[at].1.arguments.insert(rule.binding.clone(), instance.clone());
        }
    }
    events.into_iter().map(|(_, event)| event).collect()
}
