//! One step: fire a trigger, and report everything that follows.
//!
//! The order is the order a reader would check it in, and each stage can stop
//! the next:
//!
//! 1. **Which rules wait for this trigger.** Across every module, because a
//!    trigger emitted in one is routinely consumed in another — that crossing is
//!    the main thing this tool exists to make visible.
//! 2. **Do their preconditions hold?** Reported clause by clause. Only a
//!    definite `false` refuses the rule; an undecided one leaves the outcome
//!    indeterminate and says which clause could not be settled.
//! 3. **What do the postconditions change?** Applied in declaration order,
//!    because a creation binds a name the next clause uses.
//! 4. **Do the invariants still hold?** Checked after, over the world that
//!    resulted. An invariant that was already broken before the step is
//!    reported as such rather than blamed on the rule.
//! 5. **What is newly possible?** The state-condition rules that hold now and
//!    did not before. Walking that list is how a simulation moves without
//!    anybody inventing the next event, and it is the closest an Allium spec
//!    gets to describing a journey.

use std::collections::BTreeMap;

use inspect_model::{NodeKind, Program, SpecGraph, graph::TriggerSource};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    apply::Against,
    apply::{Application, Effect},
    eval::{Env, Unresolved, eval},
    truth::Truth,
    value::Value,
    world::{Event, World},
};

/// One precondition, and what the simulator made of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct ClauseVerdict {
    /// The clause as the spec wrote it.
    pub text: String,
    pub truth: Truth,
    /// What could not be decided, when anything could not.
    pub unresolved: Vec<Unresolved>,
}

/// An invariant, checked against the world a step produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct InvariantVerdict {
    pub id: String,
    pub name: String,
    pub truth: Truth,
    /// Whether it was already failing before this step ran.
    ///
    /// The difference between "this rule broke it" and "it was broken when you
    /// got here", which a reader needs and the truth value alone cannot give.
    pub already_broken: bool,
    pub unresolved: Vec<Unresolved>,
}

/// How a rule responded to the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Disposition {
    /// Every precondition held; the postconditions were applied.
    Fired,
    /// A precondition was definitely false. Nothing was applied.
    Refused,
    /// A precondition could not be decided, so neither could the rule.
    Undecided,
    /// The rule's clauses did not parse, so there was nothing to evaluate.
    Unsimulatable,
}

/// What one rule did with the trigger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct RuleOutcome {
    pub rule: String,
    pub name: String,
    pub module: String,
    pub disposition: Disposition,
    pub requires: Vec<ClauseVerdict>,
    pub effects: Vec<Effect>,
    pub unresolved: Vec<Unresolved>,
}

/// A state-condition rule that holds now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Enabled {
    pub rule: String,
    pub name: String,
    pub module: String,
    /// The trigger to fire to run it.
    pub trigger: String,
    pub source: TriggerSource,
    /// The instances it holds for, so the user can pick one.
    pub over: Vec<Value>,
    /// What the `when` clause calls the instance — the `copy` in
    /// `when: copy: Copy.status = lost`. Anything running the rule has to bind
    /// it under that name or every clause that mentions it reads as unknown.
    pub binding: String,
    /// Instances whose condition could not be settled, and why.
    ///
    /// `over` holds the ones this rule definitely holds for. Everything else
    /// used to be dropped without distinction, so an instance the condition
    /// said *no* about and one nothing could decide left the same trace: none.
    /// This was the only verdict-bearing type in a step outcome with nowhere
    /// to put a reason, which made it the one place the simulator could go
    /// quiet without breaking its own rule.
    pub undecided: Vec<Unresolved>,
}

/// Everything one step produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct StepOutcome {
    pub world: World,
    pub event: Event,
    /// Every rule that waits for this trigger, in id order.
    pub rules: Vec<RuleOutcome>,
    pub invariants: Vec<InvariantVerdict>,
    /// State-condition rules that hold after the step and did not before.
    pub newly_enabled: Vec<Enabled>,
    /// Triggers the applied rules emitted, for the user to fire next.
    pub emitted: Vec<String>,
}

impl StepOutcome {
    /// Whether any rule actually fired.
    #[must_use]
    pub fn fired(&self) -> bool {
        self.rules.iter().any(|rule| rule.disposition == Disposition::Fired)
    }

    /// Whether anything at all could not be decided.
    #[must_use]
    pub fn has_unknowns(&self) -> bool {
        self.rules.iter().any(|rule| !rule.unresolved.is_empty())
            || self.invariants.iter().any(|invariant| !invariant.unresolved.is_empty())
    }

    /// Invariants that this step broke.
    pub fn broken(&self) -> impl Iterator<Item = &InvariantVerdict> {
        self.invariants
            .iter()
            .filter(|invariant| invariant.truth == Truth::False && !invariant.already_broken)
    }
}

/// The spec text of each module, for quoting what could not be decided.
pub type Sources = BTreeMap<String, String>;

/// Fire `event` against `world`.
#[must_use]
pub fn step(
    spec: &SpecGraph,
    program: &Program,
    sources: &Sources,
    world: &World,
    event: &Event,
) -> StepOutcome {
    let before = check_invariants(spec, program, sources, world);
    let enabled_before = enabled(spec, program, sources, world);

    let mut next = world.clone();
    let mut rules = Vec::new();
    let mut emitted = Vec::new();

    for node in spec.nodes_of(NodeKind::Rule) {
        let Some(detail) = node.detail.as_rule() else { continue };
        if detail.trigger != event.trigger {
            continue;
        }
        let source = sources.get(&node.module).map(String::as_str).unwrap_or_default();
        let outcome = run_rule(
            node.id.as_str(),
            &node.name,
            &node.module,
            detail,
            program,
            spec,
            source,
            &mut next,
            event,
        );
        for effect in &outcome.effects {
            if let Effect::Emitted { trigger, .. } = effect {
                emitted.push(trigger.clone());
            }
        }
        rules.push(outcome);
    }

    emitted.sort();
    emitted.dedup();

    let after = check_invariants(spec, program, sources, &next);
    let invariants = after
        .into_iter()
        .map(|mut verdict| {
            verdict.already_broken = before
                .iter()
                .any(|earlier| earlier.id == verdict.id && earlier.truth == Truth::False);
            verdict
        })
        .collect();

    let newly_enabled = enabled(spec, program, sources, &next)
        .into_iter()
        .filter(|candidate| !enabled_before.iter().any(|earlier| earlier.rule == candidate.rule))
        .collect();

    StepOutcome { world: next, event: event.clone(), rules, invariants, newly_enabled, emitted }
}

/// Evaluate one rule's preconditions and, if they hold, its postconditions.
#[expect(
    clippy::too_many_arguments,
    reason = "every argument is a distinct input the rule needs; bundling them into a \
              context struct would name the same things twice"
)]
fn run_rule(
    id: &str,
    name: &str,
    module: &str,
    detail: &inspect_model::graph::RuleDetail,
    program: &Program,
    spec: &SpecGraph,
    source: &str,
    world: &mut World,
    event: &Event,
) -> RuleOutcome {
    let bindings: BTreeMap<String, Value> = event.arguments.clone();
    let empty = inspect_model::RuleAst::default();
    let ast = program.rule(id).unwrap_or(&empty);

    let mut requires = Vec::new();
    let mut unresolved = Vec::new();

    for (index, clause) in ast.requires.iter().enumerate() {
        let text = detail
            .clauses_of("requires")
            .nth(index)
            .map(|clause| clause.text.clone())
            .unwrap_or_default();
        let evaluated = {
            let mut scope = Env::new(world, module, source).deriving(program.derivations());
            scope.bindings.clone_from(&bindings);
            eval(clause, &scope)
        };
        unresolved.extend(evaluated.unresolved.clone());
        requires.push(ClauseVerdict {
            text,
            truth: evaluated.truth(),
            unresolved: evaluated.unresolved,
        });
    }

    let verdict = Truth::all(requires.iter().map(|clause| clause.truth));
    let disposition = if ast.is_empty() && !detail.clauses.is_empty() {
        Disposition::Unsimulatable
    } else {
        match verdict {
            Truth::True => Disposition::Fired,
            Truth::False => Disposition::Refused,
            Truth::Unknown => Disposition::Undecided,
        }
    };

    let mut effects = Vec::new();
    if disposition == Disposition::Fired {
        let mut application = Application::new(
            Against { spec, module, source, derived: program.derivations() },
            world,
            bindings,
        );
        for clause in &ast.ensures {
            let applied = application.apply(clause);
            effects.extend(applied.effects);
            unresolved.extend(applied.unresolved);
        }
    }

    RuleOutcome {
        rule: id.to_owned(),
        name: name.to_owned(),
        module: module.to_owned(),
        disposition,
        requires,
        effects,
        unresolved,
    }
}

/// Check every invariant the spec states a condition for.
fn check_invariants(
    spec: &SpecGraph,
    program: &Program,
    sources: &Sources,
    world: &World,
) -> Vec<InvariantVerdict> {
    spec.nodes_of(NodeKind::Invariant)
        .filter_map(|node| {
            let condition = program.invariant(node.id.as_str())?;
            let source = sources.get(&node.module).map(String::as_str).unwrap_or_default();
            let scope = Env::new(world, &node.module, source).deriving(program.derivations());
            let evaluated = eval(condition, &scope);
            Some(InvariantVerdict {
                id: node.id.as_str().to_owned(),
                name: node.name.clone(),
                truth: evaluated.truth(),
                already_broken: false,
                unresolved: evaluated.unresolved,
            })
        })
        .collect()
}

/// Every state-condition rule whose condition holds over `world`.
/// Every state-condition rule whose condition holds in `world`.
///
/// Distinct from [`StepOutcome::newly_enabled`], which subtracts the ones that
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
        let Some(detail) = node.detail.as_rule() else { continue };
        if detail.source == TriggerSource::External {
            continue;
        }
        let Some(ast) = program.rule(node.id.as_str()) else { continue };
        // `when: copy: Copy.status = lost` — a binding, not a call. A state
        // rule that is written any other way has no instance to range over.
        let Some(allium_parser::ast::Expr::Binding { name, value: condition, .. }) = &ast.when
        else {
            continue;
        };

        let source = sources.get(&node.module).map(String::as_str).unwrap_or_default();
        let entity = detail.trigger.as_str();
        let binding = name.name.as_str();
        let mut over = Vec::new();
        let mut undecided = Vec::new();

        for instance in world.instances_of(entity) {
            let mut scope = Env::new(world, &node.module, source).deriving(program.derivations());
            scope.bindings.insert(binding.to_owned(), Value::Ref(instance.id.clone()));
            scope.bindings.insert("this".to_owned(), Value::Ref(instance.id.clone()));
            // The entity's own name too. `when: copy: Copy.status = lost` reads
            // as "for each Copy, where *this* copy's status is lost" — inside
            // the condition the type name means the instance, not the
            // collection. Without this the condition asks whether *every* copy
            // is lost, which is a different question and answers `unknown`.
            scope.bindings.insert(entity.to_owned(), Value::Ref(instance.id.clone()));
            for (field, value) in &instance.fields {
                scope.bindings.insert(field.clone(), value.clone());
            }
            let evaluated = eval(condition, &scope);
            match evaluated.truth() {
                Truth::True => over.push(Value::Ref(instance.id.clone())),
                // A condition nobody could settle is not a condition that said
                // no, and the browser offers these as the steps a person may
                // take next — so an instance missing from that list for a
                // reason nothing recorded is a rule silently not offered.
                Truth::Unknown => undecided.extend(evaluated.unresolved),
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
            });
        }
    }

    found
}
