//! Applying an `ensures` clause: what actually changes.
//!
//! Four shapes cover every postcondition a real spec writes:
//!
//! ```text
//! ensures: Message.created(group: group, status: visible)   -- creation
//! ensures: copy.status = on_loan                            -- assignment
//! ensures: MessageSent(message: message)                    -- emission
//! ensures: if attachment_name != null: …                    -- conditional
//! ```
//!
//! Two rules govern how they are applied, and both exist to stop the simulator
//! agreeing with itself rather than with the spec.
//!
//! **A state change is checked against the lifecycle.** `transitions status {
//! visible -> tombstoned }` is a claim about what may happen, so an assignment
//! the graph does not permit is refused and reported. A simulator that wrote it
//! anyway would demonstrate behaviour the specification forbids, which is worse
//! than useless in a tool people use to understand a specification.
//!
//! **A creation binds its own name.** `Message.created(...)` makes `message`
//! available to the clauses after it, because that is how `MessageSent(message:
//! message)` on the next line finds what to send. The binding is the lower-cased
//! entity name, which is the convention the language uses throughout.

use std::collections::BTreeMap;

use inspect_model::{NodeKind, SpecGraph, graph::NodeId};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use ts_rs::TS;

use crate::{
    eval::{Env, Evaluation, Unresolved, bare_name, eval, span_of, tagged},
    truth::Truth,
    value::{EntityId, Value},
    world::World,
};

/// Something a rule did, or refused to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Effect {
    /// An instance came into existence.
    Created { id: EntityId, entity: String },
    /// A field was set.
    Assigned { id: EntityId, field: String, from: Value, to: Value },
    /// A trigger was emitted, for another rule to consume.
    Emitted { trigger: String, module: String },
    /// A state change the entity's lifecycle does not permit.
    ///
    /// Reported rather than performed. The lifecycle is part of the spec, and a
    /// simulator that wrote the value anyway would be demonstrating behaviour
    /// the specification forbids.
    Refused { id: EntityId, field: String, from: String, to: String, reason: String },
    /// Something the clause asserts that the simulator did not act on.
    ///
    /// Removal in Allium is an assertion about the end state rather than an
    /// instruction, and a conditional whose condition is undecided is a branch
    /// the simulator cannot take or skip. Both are shown rather than guessed.
    Noted { description: String },
}

/// What applying one `ensures` clause produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub effects: Vec<Effect>,
    pub unresolved: Vec<Unresolved>,
}

impl Applied {
    fn note(unresolved: Vec<Unresolved>) -> Self {
        Self { effects: Vec::new(), unresolved }
    }

    fn effect(effect: Effect) -> Self {
        Self { effects: vec![effect], unresolved: Vec::new() }
    }

    fn merge(&mut self, other: Applied) {
        self.effects.extend(other.effects);
        self.unresolved.extend(other.unresolved);
    }
}

/// One rule's postconditions being applied to one world.
///
/// A struct rather than a pile of arguments because every one of these travels
/// together through six recursive calls, and `bindings` in particular has to be
/// carried *forward*: a creation names its instance, and the next clause reads
/// that name.
pub struct Application<'a> {
    spec: &'a SpecGraph,
    module: &'a str,
    source: &'a str,
    world: &'a mut World,
    bindings: BTreeMap<String, Value>,
}

impl<'a> Application<'a> {
    /// Apply `module`'s postconditions to `world`, starting from `bindings`.
    pub fn new(
        spec: &'a SpecGraph,
        module: &'a str,
        source: &'a str,
        world: &'a mut World,
        bindings: BTreeMap<String, Value>,
    ) -> Self {
        Self { spec, module, source, world, bindings }
    }

    /// Apply one `ensures` clause.
    pub fn apply(&mut self, clause: &Json) -> Applied {
        let Some((tag, inner)) = tagged(clause) else { return Applied::default() };

        match tag {
            "Call" => self.call(inner),
            "Comparison" => self.assignment(inner),
            "Block" => {
                let mut applied = Applied::default();
                let empty = Vec::new();
                let items = inner.get("items").and_then(Json::as_array).unwrap_or(&empty);
                for item in items {
                    let step = self.apply(item);
                    applied.merge(step);
                }
                applied
            }
            "Conditional" => self.conditional(inner),
            "For" => self.iteration(inner),
            "NotExists" | "Exists" => Applied::effect(Effect::Noted {
                description: self
                    .describe(clause)
                    .unwrap_or_else(|| "an assertion about what exists".to_owned()),
            }),
            _ => Applied::default(),
        }
    }

    /// The bindings after everything applied, including anything a creation
    /// introduced.
    #[must_use]
    pub fn into_bindings(self) -> BTreeMap<String, Value> {
        self.bindings
    }

    /// `Entity.created(...)` or `TriggerName(...)`.
    fn call(&mut self, inner: &Json) -> Applied {
        let Some(function) = inner.get("function") else { return Applied::default() };

        // `Message.created(...)`: a member access whose field is `created`.
        if let Some(("MemberAccess", access)) = tagged(function)
            && access
                .get("field")
                .and_then(|field| field.get("name"))
                .and_then(Json::as_str)
                .is_some_and(|name| name == "created")
            && let Some(entity) = access.get("object").and_then(bare_name)
        {
            return self.creation(&entity, inner);
        }

        // Otherwise a bare name: a trigger emission.
        match bare_name(function) {
            Some(trigger) => {
                Applied::effect(Effect::Emitted { trigger, module: self.module.to_owned() })
            }
            None => Applied::default(),
        }
    }

    /// `Entity.created(field: value, ...)`.
    fn creation(&mut self, entity: &str, call: &Json) -> Applied {
        let home = self.declaring_module(entity).unwrap_or_else(|| self.module.to_owned());
        let id = self.world.create(entity, &home);
        let mut applied =
            Applied::effect(Effect::Created { id: id.clone(), entity: entity.to_owned() });

        let empty = Vec::new();
        let arguments = call.get("args").and_then(Json::as_array).unwrap_or(&empty).clone();
        for argument in &arguments {
            let Some(("Named", named)) = tagged(argument) else { continue };
            let Some(field) =
                named.get("name").and_then(|name| name.get("name")).and_then(Json::as_str)
            else {
                continue;
            };
            let Some(value_node) = named.get("value") else { continue };

            let mut evaluated = self.evaluate(value_node);
            // `Loan.created(status: open)` — `open` is a state, and at creation
            // there is no previous value to say so. The entity's declared states
            // are what settle it, and they come from the spec rather than from
            // the world. Without this the new instance starts with an undecided
            // status, and every rule that later reads it is undecided too.
            if evaluated.value.is_unknown()
                && let Some(name) = bare_name(value_node)
                && let Some(state) = self.declared_state(entity, field, &name)
            {
                evaluated = Evaluation { value: state, unresolved: Vec::new() };
            }
            applied.unresolved.extend(evaluated.unresolved);
            self.world.set_field(&id, field, evaluated.value);
        }

        // The instance binds its own name, so the next clause can refer to it.
        self.bindings.insert(entity.to_lowercase(), Value::Ref(id));
        applied
    }

    /// `copy.status = on_loan`.
    fn assignment(&mut self, inner: &Json) -> Applied {
        if inner.get("op").and_then(Json::as_str) != Some("Eq") {
            // A comparison that is not an assignment is an assertion about the
            // end state. Noted rather than acted on.
            return Applied::effect(Effect::Noted {
                description: self
                    .describe(inner)
                    .unwrap_or_else(|| "an assertion about the end state".to_owned()),
            });
        }

        let (Some(target), Some(value_node)) = (inner.get("left"), inner.get("right")) else {
            return Applied::default();
        };
        let Some(("MemberAccess", access)) = tagged(target) else { return Applied::default() };
        let Some(field) =
            access.get("field").and_then(|field| field.get("name")).and_then(Json::as_str)
        else {
            return Applied::default();
        };
        let field = field.to_owned();

        let Some(object) = access.get("object") else { return Applied::default() };
        let located = self.evaluate(object);
        let mut unresolved = located.unresolved;
        let Value::Ref(id) = located.value else { return Applied::note(unresolved) };

        let mut evaluated = self.evaluate(value_node);
        // A bare name on the right of a status assignment is a state, the same
        // reading a comparison makes.
        //
        // Keyed on the field's *declared states*, not on it having a lifecycle:
        // a status field may declare `open | returned` and no transitions at
        // all, and gating on the transition graph leaves every such assignment
        // undecided. Checking the states also rejects a misspelling instead of
        // inventing a state nothing declares.
        if evaluated.value.is_unknown()
            && let Some(name) = bare_name(value_node)
            && let Some(entity) = self.world.instance(&id).map(|instance| instance.entity.clone())
            && let Some(state) = self.declared_state(&entity, &field, &name)
        {
            evaluated = Evaluation { value: state, unresolved: Vec::new() };
        }
        unresolved.extend(evaluated.unresolved);

        let previous =
            self.world.instance(&id).map_or(Value::Unknown, |instance| instance.field(&field));

        if let (Value::Enum(from), Value::Enum(to)) = (&previous, &evaluated.value)
            && let Some(refusal) = self.forbidden_transition(&id, &field, from, to)
        {
            return Applied { effects: vec![refusal], unresolved };
        }

        self.world.set_field(&id, &field, evaluated.value.clone());
        Applied {
            effects: vec![Effect::Assigned { id, field, from: previous, to: evaluated.value }],
            unresolved,
        }
    }

    /// `if condition: …`.
    fn conditional(&mut self, inner: &Json) -> Applied {
        let Some(condition) = inner.get("condition") else { return Applied::default() };
        let verdict = self.evaluate(condition);

        match verdict.truth() {
            Truth::True => match inner.get("then").or_else(|| inner.get("body")) {
                Some(node) => {
                    let node = node.clone();
                    let mut applied = self.apply(&node);
                    applied.unresolved.extend(verdict.unresolved);
                    applied
                }
                None => Applied::note(verdict.unresolved),
            },
            Truth::False => Applied::note(verdict.unresolved),
            // The branch is neither taken nor skipped — the simulator does not
            // know which, and applying it would be a coin toss with side
            // effects.
            Truth::Unknown => {
                let description =
                    self.describe(inner).unwrap_or_else(|| "its condition".to_owned());
                let mut applied = Applied::note(verdict.unresolved);
                applied.effects.push(Effect::Noted {
                    description: format!("a conditional postcondition was skipped: {description}"),
                });
                applied
            }
        }
    }

    /// `for x in collection: …`.
    fn iteration(&mut self, inner: &Json) -> Applied {
        let Some(collection) = inner.get("collection").or_else(|| inner.get("source")) else {
            return Applied::default();
        };
        let binding = inner
            .get("binding")
            .and_then(|node| node.get("name"))
            .and_then(Json::as_str)
            .unwrap_or("it")
            .to_owned();
        let Some(body) = inner.get("body").cloned() else { return Applied::default() };

        let evaluated = self.evaluate(collection);
        let mut applied = Applied::note(evaluated.unresolved);
        let Value::Set(items) = evaluated.value else { return applied };

        for item in items {
            let restore = self.bindings.insert(binding.clone(), item);
            let step = self.apply(&body);
            applied.merge(step);
            match restore {
                Some(previous) => self.bindings.insert(binding.clone(), previous),
                None => self.bindings.remove(&binding),
            };
        }
        applied
    }

    // --- helpers ---------------------------------------------------------

    fn evaluate(&self, node: &Json) -> Evaluation {
        let mut scope = Env::new(self.world, self.module, self.source);
        scope.bindings.clone_from(&self.bindings);
        eval(node, &scope)
    }

    /// The module that declares `entity`, so a created instance knows where it
    /// is from even when the rule creating it lives elsewhere.
    fn declaring_module(&self, entity: &str) -> Option<String> {
        self.spec
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Entity && node.name == entity)
            .map(|node| node.module.clone())
    }

    /// `name` as a state of `entity.field`, when the spec declares it as one.
    ///
    /// Checked against the declared states rather than accepted on sight: a
    /// misspelled state should stay undecided and be reported, not become a
    /// state nothing in the lifecycle mentions.
    fn declared_state(&self, entity: &str, field: &str, name: &str) -> Option<Value> {
        let node = self
            .spec
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Entity && node.name == entity)?;
        let declared = node.detail.as_entity()?.field(field)?;
        declared.enum_values.iter().any(|state| state == name).then(|| Value::Enum(name.to_owned()))
    }

    /// The transition graph governing `field`, if the spec declares one.
    fn lifecycle(
        &self,
        id: &EntityId,
        field: &str,
    ) -> Option<&'a inspect_model::graph::TransitionGraph> {
        let instance = self.world.instance(id)?;
        let node =
            self.spec.node(&NodeId::new(&instance.module, NodeKind::Entity, &instance.entity))?;
        node.detail.as_entity()?.transitions_for(field)
    }

    /// A refusal, when the lifecycle does not permit `from -> to`.
    fn forbidden_transition(
        &self,
        id: &EntityId,
        field: &str,
        from: &str,
        to: &str,
    ) -> Option<Effect> {
        let graph = self.lifecycle(id, field)?;
        if from == to || graph.allows(from, to) {
            return None;
        }
        let permitted: Vec<&str> = graph.successors(from).collect();
        Some(Effect::Refused {
            id: id.clone(),
            field: field.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            reason: if permitted.is_empty() {
                format!("`{from}` is terminal: the lifecycle allows nothing after it")
            } else {
                format!("the lifecycle allows only {} after `{from}`", permitted.join(", "))
            },
        })
    }

    /// The clause as the spec wrote it, on one line.
    fn describe(&self, node: &Json) -> Option<String> {
        let span = span_of(node)?;
        span.slice(self.source).map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}
