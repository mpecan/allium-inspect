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

mod calls;
mod iteration;

pub(crate) use iteration::{bound_name, destructured, header, keeps};

use std::collections::BTreeMap;

use allium_parser::ast::{ComparisonOp, Expr};
use inspect_model::{Node, NodeKind, SpecGraph, graph::NodeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    eval::{Env, Evaluation, Unresolved, bare_name, eval, span_of},
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
    /// An instance stopped existing: `ensures: not exists presence`.
    Removed { id: EntityId, entity: String },
    /// A field was set.
    Assigned { id: EntityId, field: String, from: Value, to: Value },
    /// A trigger was emitted, for another rule to consume.
    ///
    /// With the arguments it carries, because the consumer needs them: a
    /// journey saying "he writes into the room" means everything that set off,
    /// and `QueueOnSend` cannot fill the outbox without knowing which message.
    /// The simulator still stops here and offers the trigger to the reader —
    /// one step is one step — but a walk has already said the act happened.
    ///
    /// Named arguments under the name written, positional ones in order: which
    /// name a positional argument goes by is the *listener's* to say, and two
    /// listeners may say differently (see [`crate::world::Event::positional`]).
    Emitted {
        trigger: String,
        module: String,
        arguments: BTreeMap<String, Value>,
        #[serde(default)]
        positional: Vec<Value>,
    },
    /// A state change the entity's lifecycle does not permit.
    ///
    /// Reported rather than performed. The lifecycle is part of the spec, and a
    /// simulator that wrote the value anyway would be demonstrating behaviour
    /// the specification forbids.
    Refused { id: EntityId, field: String, from: String, to: String, reason: String },
    /// Something the clause asserts that the simulator did not act on.
    ///
    /// A conditional whose condition is undecided is a branch the simulator
    /// cannot take or skip, and `ensures: exists x` asserts rather than does.
    /// Both are shown rather than guessed.
    Noted { description: String },
}

/// What applying one `ensures` clause produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub effects: Vec<Effect>,
    /// Every note the clause left, whether or not it mattered.
    pub unresolved: Vec<Unresolved>,
    /// What the clause promised and the world does not show, and why.
    ///
    /// Apart from `unresolved`, because a note is not that. An `if` whose
    /// `or` was settled by its left side still carries a note about the right,
    /// and `c.done = Hooks.any(…)` settled by one hook carries one about every
    /// other; both were applied in full. Read off the notes, those rules were
    /// reported as leaving something undone. Said here only by the paths that
    /// actually left something undone: a member of a `for` nobody could
    /// decide about, a `for` over something that is not a collection, a
    /// branch nobody could choose, an assignment to nothing, and a form the
    /// simulator does not model.
    pub unapplied: Vec<Unresolved>,
}

impl Applied {
    fn note(unresolved: Vec<Unresolved>) -> Self {
        Self { effects: Vec::new(), unresolved, unapplied: Vec::new() }
    }

    fn effect(effect: Effect) -> Self {
        Self { effects: vec![effect], ..Self::default() }
    }

    /// Something promised and not applied, said first among the notes so it
    /// is what a reader is shown.
    fn left_undone(&mut self, why: Unresolved) {
        self.unresolved.insert(0, why.clone());
        self.unapplied.push(why);
    }

    fn merge(&mut self, other: Applied) {
        self.effects.extend(other.effects);
        self.unresolved.extend(other.unresolved);
        self.unapplied.extend(other.unapplied);
    }
}

/// One rule's postconditions being applied to one world.
///
/// A struct rather than a pile of arguments because every one of these travels
/// together through six recursive calls, and `bindings` in particular has to be
/// carried *forward*: a creation names its instance, and the next clause reads
/// that name.
/// The specification a postcondition is applied against.
///
/// Four things that always travel together and never change during an
/// application, grouped so the call site reads as *this spec, this world*
/// rather than as six positional arguments of four shapes.
#[derive(Clone, Copy)]
pub struct Against<'a> {
    pub spec: &'a SpecGraph,
    pub module: &'a str,
    pub source: &'a str,
    /// What the spec computes, so a postcondition reading a derived value gets
    /// the same answer a precondition would.
    pub derived: &'a inspect_model::Derivations,
}

pub struct Application<'a> {
    against: Against<'a>,
    world: &'a mut World,
    bindings: BTreeMap<String, Value>,
}

impl<'a> Application<'a> {
    /// Apply `module`'s postconditions to `world`, starting from `bindings`.
    pub fn new(
        against: Against<'a>,
        world: &'a mut World,
        bindings: BTreeMap<String, Value>,
    ) -> Self {
        Self { against, world, bindings }
    }

    /// Apply one `ensures` clause.
    pub fn apply(&mut self, clause: &Expr) -> Applied {
        match clause {
            Expr::Call { function, args, .. } => self.call(function, args),
            Expr::Comparison { left, op, right, .. } => self.assignment(clause, left, *op, right),
            Expr::Block { items, .. } => {
                let mut applied = Applied::default();
                for item in items {
                    let step = self.apply(item);
                    applied.merge(step);
                }
                applied
            }
            Expr::Conditional { branches, else_body, .. } => {
                self.conditional(clause, branches, else_body.as_deref())
            }
            Expr::For { binding, collection, filter, body, .. } => {
                self.iteration(binding, collection, filter.as_deref(), body)
            }
            // `let group = Group.created(…)`, and the lines below it that refer
            // to `group`. A rule that creates two things and joins them writes
            // it this way — it is the only way to write it — so without this a
            // `CreateGroup` fires, reports success, and leaves an empty world.
            Expr::LetExpr { name, value, .. } => self.binding(&name.name, value),
            Expr::NotExists { operand, .. } => self.removal(clause, operand),
            Expr::Exists { .. } => self.asserted(clause),
            _ => self.unmodelled(clause, "this postcondition is not a form the simulator applies"),
        }
    }

    /// `ensures: not exists presence` — afterwards, it is gone.
    ///
    /// What the language says `not exists` means as an outcome: the entity is
    /// removed. Only noting it left the instance standing after a rule that
    /// said it was gone, so `then presence does not exist` was refused over a
    /// removal that went through.
    ///
    /// One instance is removed, and `null` — a lookup that matched nothing —
    /// already holds. A collection is noted, as before: `not exists Copy`
    /// names a type where an instance was meant, and removing every copy in
    /// the world on the strength of it would be a guess with side effects.
    fn removal(&mut self, whole: &Expr, operand: &Expr) -> Applied {
        let found = self.evaluate(operand);
        match found.value {
            Value::Ref(id) => match self.world.remove(&id) {
                Some(instance) => Applied::effect(Effect::Removed { id, entity: instance.entity }),
                // Already gone is the end state the clause asks for.
                None => Applied::default(),
            },
            Value::Null => Applied::default(),
            Value::Unknown => self.skipped(whole, found.unresolved),
            _ => self.asserted(whole),
        }
    }

    /// A clause about what exists that the simulator notes and does not act
    /// on, quoted as the author wrote it.
    fn asserted(&self, clause: &Expr) -> Applied {
        Applied::effect(Effect::Noted {
            description: self
                .describe(clause)
                .unwrap_or_else(|| "an assertion about what exists".to_owned()),
        })
    }

    /// The bindings after everything applied, including anything a creation
    /// introduced.
    #[must_use]
    pub fn into_bindings(self) -> BTreeMap<String, Value> {
        self.bindings
    }

    /// `copy.status = on_loan`.
    fn assignment(
        &mut self,
        whole: &Expr,
        target: &Expr,
        op: ComparisonOp,
        value_node: &Expr,
    ) -> Applied {
        if op != ComparisonOp::Eq {
            // A comparison that is not an assignment is an assertion about the
            // end state. Noted rather than acted on.
            return Applied::effect(Effect::Noted {
                description: self
                    .describe(whole)
                    .unwrap_or_else(|| "an assertion about the end state".to_owned()),
            });
        }

        let Expr::MemberAccess { object, field, .. } = target else {
            return self.unmodelled(target, "this is not a field the simulator can assign to");
        };
        let field = field.name.clone();
        let located = self.evaluate(object);
        let mut unresolved = located.unresolved;
        let Value::Ref(id) = located.value else {
            let mut applied = Applied::note(unresolved);
            applied.left_undone(Unresolved {
                reason: match located.value {
                    Value::Unknown => {
                        format!("`{field}` was not assigned: what it belongs to was never settled")
                    }
                    other => format!(
                        "`{field}` was not assigned: it would belong to {}, which is not an \
                         instance",
                        other.described()
                    ),
                },
                expression: self.describe(whole),
                span: span_of(whole),
            });
            return applied;
        };

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
            && let Some((home, entity)) = self
                .world
                .instance(&id)
                .map(|instance| (instance.module.clone(), instance.entity.clone()))
            && let Some(state) = self.declared_state(&home, &entity, &field, name)
        {
            evaluated = Evaluation { value: state, unresolved: Vec::new() };
        }
        unresolved.extend(evaluated.unresolved);

        let previous =
            self.world.instance(&id).map_or(Value::Unknown, |instance| instance.field(&field));

        if let (Value::Enum(from), Value::Enum(to)) = (&previous, &evaluated.value)
            && let Some(refusal) = self.forbidden_transition(&id, &field, from, to)
        {
            return Applied { effects: vec![refusal], unresolved, unapplied: Vec::new() };
        }

        self.world.set_field(&id, &field, evaluated.value.clone());
        Applied {
            effects: vec![Effect::Assigned { id, field, from: previous, to: evaluated.value }],
            unresolved,
            unapplied: Vec::new(),
        }
    }

    /// `if condition: …`.
    fn conditional(
        &mut self,
        whole: &Expr,
        branches: &[allium_parser::ast::CondBranch],
        otherwise: Option<&Expr>,
    ) -> Applied {
        let mut unresolved = Vec::new();
        for branch in branches {
            let verdict = self.evaluate(&branch.condition);
            unresolved.extend(verdict.unresolved);
            match verdict.value.truth() {
                Truth::True => {
                    let body = branch.body.clone();
                    let mut applied = self.apply(&body);
                    applied.unresolved.extend(unresolved);
                    return applied;
                }
                Truth::False => {}
                Truth::Unknown => return self.skipped(whole, unresolved),
            }
        }
        match otherwise {
            Some(body) => {
                let body = body.clone();
                let mut applied = self.apply(&body);
                applied.unresolved.extend(unresolved);
                applied
            }
            None => Applied::note(unresolved),
        }
    }

    /// A postcondition this simulator does not model.
    ///
    /// The rule fired — the spec says so — but the world does not carry this
    /// part of what it promised, and somebody reading the two against each
    /// other has to be told which part. Dropping it returned a rule marked
    /// Fired over a world that had not changed, with nothing anywhere saying
    /// why. `skipped` says this for a branch nobody could choose; this says it
    /// for a form nobody has taught the simulator yet.
    fn unmodelled(&self, whole: &Expr, why: &str) -> Applied {
        let description = self.describe(whole).unwrap_or_else(|| why.to_owned());
        let said = format!("{why}, so the world does not show it: {description}");
        let mut applied = Applied::effect(Effect::Noted { description: said.clone() });
        applied.left_undone(Unresolved {
            reason: said,
            expression: self.describe(whole),
            span: span_of(whole),
        });
        applied
    }

    /// A branch neither taken nor skipped, because nobody could tell which.
    ///
    /// Applying it would be a coin toss with side effects, and saying nothing
    /// would leave a world that is missing a change with no record of why.
    fn skipped(&self, whole: &Expr, unresolved: Vec<Unresolved>) -> Applied {
        let description = self.describe(whole).unwrap_or_else(|| "its condition".to_owned());
        let said = format!("a conditional postcondition was skipped: {description}");
        let mut applied = Applied::note(unresolved);
        applied.effects.push(Effect::Noted { description: said.clone() });
        // After the condition's own reason, which says what could not be read;
        // this says what went unapplied because of it.
        applied.unresolved.push(Unresolved {
            reason: said.clone(),
            expression: self.describe(whole),
            span: span_of(whole),
        });
        applied.unapplied.push(Unresolved {
            reason: said,
            expression: self.describe(whole),
            span: span_of(whole),
        });
        applied
    }

    // --- helpers ---------------------------------------------------------

    fn evaluate(&self, node: &Expr) -> Evaluation {
        eval(node, &self.scope())
    }

    /// The environment a clause is read in: this world, this module, and
    /// every name the rule has bound so far.
    fn scope(&self) -> Env<'_> {
        let mut scope = Env::new(self.world, self.against.module, self.against.source)
            .deriving(self.against.derived);
        scope.bindings.clone_from(&self.bindings);
        scope
    }

    /// The transition graph governing `field`, if the spec declares one.
    fn lifecycle(
        &self,
        id: &EntityId,
        field: &str,
    ) -> Option<&'a inspect_model::graph::TransitionGraph> {
        let instance = self.world.instance(id)?;
        let node = self.against.spec.node(&NodeId::new(
            &instance.module,
            NodeKind::Entity,
            &instance.entity,
        ))?;
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
    fn describe(&self, node: &Expr) -> Option<String> {
        let span = span_of(node)?;
        span.slice(self.against.source)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

/// The entity `name` that `module` declares, or the first of that name when it
/// declares none: two modules may each have an `Order`, and need not give it
/// the same fields.
fn entity_node<'s>(spec: &'s SpecGraph, module: &str, name: &str) -> Option<&'s Node> {
    spec.node(&NodeId::new(module, NodeKind::Entity, name)).or_else(|| {
        spec.nodes.iter().find(|node| node.kind == NodeKind::Entity && node.name == name)
    })
}
