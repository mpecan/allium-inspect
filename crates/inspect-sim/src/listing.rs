//! Which state rules the world makes true: what the browser offers as the
//! next step, and what a journey's clock fires.
//!
//! Apart from `step`, which runs what a trigger wakes, because this is the
//! other question — not what one firing did, but what the world now holds
//! for — and both read a state rule's condition through the same `about`.

use allium_parser::ast::Expr;
use inspect_model::{NodeKind, Program, SpecGraph};

use crate::{
    due,
    eval::Env,
    step::{Enabled, Sources, about, state_condition},
    truth::Truth,
    value::Value,
    world::World,
};

/// Every state-condition rule whose condition holds in `world`.
///
/// Distinct from [`crate::StepOutcome::newly_enabled`], which subtracts the ones that
/// already held before the step. That subtraction is right for the browser,
/// where the list is "what your action just made possible"; it is wrong for
/// anything asking what the world currently makes true, because a rule that was
/// already enabled and never run would never appear.
#[must_use]
pub fn enabled(
    spec: &SpecGraph,
    program: &Program,
    sources: &Sources,
    world: &World,
) -> Vec<Enabled> {
    let mut found = Vec::new();

    for node in spec.nodes_of(NodeKind::Rule) {
        let Some((detail, (binding, condition))) = listing(program, node) else { continue };

        let source = sources.get(&node.module).map(String::as_str).unwrap_or_default();
        let entity = detail.trigger.as_str();
        let mut over = Vec::new();
        let mut undecided = Vec::new();
        let mut unsettled_over = Vec::new();

        for instance in world.instances_of(entity) {
            let scope = about(
                Env::new(world, &node.module, source)
                    .deriving(program.derivations())
                    .quoting(sources),
                entity,
                binding,
                &instance.id,
            );
            let evaluated = due::trigger(condition, &scope);
            match evaluated.truth() {
                Truth::True => over.push(Value::Ref(instance.id.clone())),
                // A condition nobody could settle is not a condition that said
                // no, and the browser offers these as the steps a person may
                // take next — so an instance missing from that list for a
                // reason nothing recorded is a rule silently not offered.
                Truth::Unknown => {
                    undecided.extend(evaluated.unresolved);
                    unsettled_over.push(Value::Ref(instance.id.clone()));
                }
                Truth::False => {}
            }
        }

        if !over.is_empty() || !undecided.is_empty() {
            found.push(Enabled {
                rule: node.id.as_str().to_owned(),
                name: node.name.clone(),
                module: node.module.clone(),
                trigger: detail.trigger.clone(),
                source: detail.source,
                over,
                binding: binding.to_owned(),
                undecided,
                unsettled_over,
            });
        }
    }

    found
}

/// Whether [`enabled`] reads `rule` at all: a state rule whose `when` binds
/// the instance it is about, with clauses to read.
///
/// One that is not — `when: Task.due_at <= now`, binding nothing — is never
/// listed, as due or as undecided, so a listing's silence about it is not a
/// *no*, and anything reading the listing has to be able to tell.
#[must_use]
pub fn lists(program: &Program, rule: &inspect_model::graph::Node) -> bool {
    listing(program, rule).is_some()
}

/// A rule [`enabled`] reads, and the binding and condition it reads it by.
fn listing<'a>(
    program: &'a Program,
    node: &'a inspect_model::graph::Node,
) -> Option<(&'a inspect_model::graph::RuleDetail, (&'a str, &'a Expr))> {
    let detail = node.detail.as_rule()?;
    let ast = program.rule(node.id.as_str())?;
    Some((detail, state_condition(detail, ast)?))
}
