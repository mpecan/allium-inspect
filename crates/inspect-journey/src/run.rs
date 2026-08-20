//! Walking a journey through the step engine.
//!
//! Every verdict is one the simulator already gives, because the honesty is the
//! point: a step holds, is refused, or could not be decided, and the third is
//! not a polite way of saying the first. What this adds is the two verdicts a
//! journey needs and a simulation does not — [`Verdict::Unspecified`], which is
//! a requirement nobody has met, and [`Verdict::Unexposed`], which is a system
//! that does the right thing and tells nobody.
//!
//! One world, many instances. Two people of the same kind with different
//! preconditions is the ordinary case, and it is a precondition on an instance
//! rather than a second world — which is also what the spec itself models, with
//! sets like `OutboxEntry.awaiting` naming the devices that do not have it yet.

use std::collections::BTreeMap;

use inspect_model::{NodeKind, Program, SpecGraph};
use inspect_sim::{
    Truth, Value,
    step::{Sources, StepOutcome, step},
    value::EntityId,
    world::{Event, World},
};

use crate::{
    check::{self, Verdict},
    journey::{Assertion, Clause, Comparison, Journey, Path, Step, Term},
    outcome::{refusal, verdict_of},
};

/// What became of one line.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub line: usize,
    pub verdict: Verdict,
    /// What the line said, as the author wrote it.
    pub about: String,
    /// Why, when there is a why worth reading.
    pub detail: Option<String>,
}

/// What became of one step.
#[derive(Debug, Clone, PartialEq)]
pub struct Walked {
    pub number: u32,
    pub title: String,
    pub outcomes: Vec<Outcome>,
}

impl Walked {
    /// The worst thing that happened in this step.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        worst(self.outcomes.iter().map(|outcome| outcome.verdict))
    }
}

/// What became of a journey.
#[derive(Debug, Clone, PartialEq)]
pub struct Walk {
    pub name: String,
    pub steps: Vec<Walked>,
    /// What the journey was told rather than shown, always reported.
    pub stipulated: Vec<String>,
}

impl Walk {
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        worst(self.steps.iter().map(Walked::verdict))
    }
}

/// The worst of several, in the order a reader cares about them.
fn worst(verdicts: impl Iterator<Item = Verdict>) -> Verdict {
    fn rank(verdict: Verdict) -> u8 {
        match verdict {
            Verdict::Specified => 0,
            Verdict::Remark => 1,
            Verdict::Unexposed => 2,
            Verdict::Undecided => 3,
            Verdict::Unspecified => 4,
            Verdict::Refused => 5,
        }
    }
    verdicts.max_by_key(|verdict| rank(*verdict)).unwrap_or(Verdict::Specified)
}

/// Walk `journey` against `spec`.
#[must_use]
pub fn walk(journey: &Journey, spec: &SpecGraph, program: &Program, sources: &Sources) -> Walk {
    let notes = check::check(journey, spec);
    let mut walker = Walker {
        spec,
        program,
        sources,
        world: inspect_sim::seed::seed(spec),
        bound: BTreeMap::new(),
        stipulated: Vec::new(),
        fired: Vec::new(),
        undecided: Vec::new(),
    };
    walker.lay_out(journey);

    let steps = journey.steps.iter().map(|step| walker.walk_step(step, &notes)).collect();

    Walk { name: journey.name.clone(), steps, stipulated: walker.stipulated }
}

pub(crate) struct Walker<'a> {
    pub(crate) spec: &'a SpecGraph,
    pub(crate) program: &'a Program,
    pub(crate) sources: &'a Sources,
    pub(crate) world: World,
    /// Every name the journey has bound to something in the world.
    pub(crate) bound: BTreeMap<String, EntityId>,
    pub(crate) stipulated: Vec<String>,
    /// Rules that ran since the last clause, for `then … fires`.
    pub(crate) fired: Vec<String>,
    /// Rules that could not be decided since it, for the same.
    pub(crate) undecided: Vec<String>,
}

impl Walker<'_> {
    /// One step: every clause in the order it was written.
    fn walk_step(&mut self, step: &Step, notes: &[check::Note]) -> Walked {
        let mut outcomes = Vec::new();
        for clause in &step.clauses {
            // A line the spec cannot support is not run. Firing an act at a
            // surface nobody specified would produce a second, confusing
            // failure about a world rather than the one that matters.
            let blocking: Vec<&check::Note> = notes
                .iter()
                .filter(|note| note.line == clause.line() && note.verdict != Verdict::Specified)
                .collect();
            if let Some(note) = blocking.first() {
                outcomes.push(Outcome {
                    line: note.line,
                    verdict: note.verdict,
                    about: about(clause),
                    detail: Some(note.message.clone()),
                });
                continue;
            }
            let mut outcome = self.walk_clause(clause);
            // A definite `false` that follows something undecided is not the
            // spec forbidding anything — it is a consequence of a world this
            // tool could not finish computing. Reporting it as a refusal would
            // say "the specification says no" where the truth is "I could not
            // tell", which is the one thing this design refuses everywhere
            // else.
            if outcome.verdict == Verdict::Refused
                && outcomes.iter().any(|earlier| earlier.verdict == Verdict::Undecided)
            {
                outcome.verdict = Verdict::Undecided;
                outcome.detail = Some(match outcome.detail {
                    Some(detail) => {
                        format!("{detail} — and something earlier in this step was undecided")
                    }
                    None => "something earlier in this step was undecided".to_owned(),
                });
            }
            outcomes.push(outcome);
        }
        Walked { number: step.number, title: step.title.clone(), outcomes }
    }

    fn walk_clause(&mut self, clause: &Clause) -> Outcome {
        let about = about(clause);
        match clause {
            Clause::Does { trigger, arguments, creating, line, .. } => {
                self.act(&Act { trigger, arguments, creating: creating.as_ref() }, *line, about)
            }
            Clause::After { duration, line, .. } => self.advance(duration, *line, about),
            Clause::Then { assertion, line } => self.assert(assertion, *line, about),
            Clause::Sees { path, negated, line, .. } => self.observe(path, *negated, *line, about),
            Clause::Stipulate { path, value, line } => {
                let value = self.value_of(value);
                self.stipulated.push(format!("{} = {}", path.as_written(), value.render()));
                self.assign(path, value);
                Outcome { line: *line, verdict: Verdict::Specified, about, detail: None }
            }
        }
    }

    /// Fire an act and keep what it caught.
    fn act(&mut self, act: &Act<'_>, line: usize, about: String) -> Outcome {
        let Act { trigger, arguments, creating } = *act;
        let module = self
            .spec
            .nodes_of(NodeKind::Trigger)
            .find(|node| node.name == trigger)
            .map_or_else(String::new, |node| node.module.clone());

        // Positional, matched against the trigger's declared parameters — which
        // is how the spec writes the act and how a person reads it back.
        let parameters = self.parameters_of(trigger);
        let mut event = Event::new(trigger, &module);
        for (at, argument) in arguments.iter().enumerate() {
            let name = parameters.get(at).cloned().unwrap_or_else(|| format!("arg{at}"));
            event = event.with(name, self.value_of(argument));
        }

        let before: Vec<EntityId> = self.world.entities.keys().cloned().collect();
        self.fired.clear();
        self.undecided.clear();
        let outcome = self.fire(&event);

        if let Some(caught) = creating {
            let bare = caught.type_expr.rsplit('/').next().unwrap_or(&caught.type_expr);
            let fresh = self
                .world
                .entities
                .keys()
                .filter(|id| !before.contains(id))
                .find(|id| id.entity() == bare)
                .cloned();
            match fresh {
                Some(id) => {
                    self.bound.insert(caught.name.clone(), id);
                }
                None => {
                    return Outcome {
                        line,
                        verdict: verdict_of(&outcome),
                        about,
                        detail: Some(format!(
                            "nothing of kind `{}` was created for `{}`",
                            caught.type_expr, caught.name
                        )),
                    };
                }
            }
        }

        Outcome { line, verdict: verdict_of(&outcome), about, detail: refusal(&outcome) }
    }

    /// The trigger's parameters, in the order the spec declares them.
    fn parameters_of(&self, trigger: &str) -> Vec<String> {
        self.spec
            .nodes_of(NodeKind::Trigger)
            .find(|node| node.name == trigger)
            .and_then(|node| node.detail.as_trigger())
            .map(|detail| detail.parameters.clone())
            .unwrap_or_default()
    }

    /// Move the clock, and let whatever becomes true fire.
    ///
    /// A temporal rule is not fired by anybody: the clock passing a due date is
    /// what makes it hold. So advancing time is a step in its own right, and the
    /// rules it wakes are its outcome.
    fn advance(&mut self, duration: &Value, line: usize, about: String) -> Outcome {
        let Value::Duration(by) = duration else {
            return Outcome {
                line,
                verdict: Verdict::Undecided,
                about,
                detail: Some("that is not a length of time".to_owned()),
            };
        };
        self.world.now = self.world.now.saturating_add(*by);
        self.fired.clear();
        self.undecided.clear();
        self.settle();
        Outcome { line, verdict: Verdict::Specified, about, detail: None }
    }

    /// Fire everything the world now makes true, until nothing else does.
    ///
    /// A state-condition rule is not fired by anybody — the clock passing a due
    /// date is what makes it hold — and the simulator reports those as *newly
    /// enabled* rather than running them, because in the browser a person picks
    /// which to follow. A journey has already said: time passed, so whatever
    /// became true happened.
    ///
    /// To a fixpoint, because one rule firing can make the next one true, and
    /// bounded because a spec with two rules that re-enable each other would
    /// otherwise run forever. Reaching the bound is reported rather than
    /// silently truncated.
    fn settle(&mut self) {
        const ROUNDS: usize = 32;
        for _ in 0..ROUNDS {
            let probe =
                step(self.spec, self.program, self.sources, &self.world, &Event::new("", ""));
            let waiting: Vec<(String, String)> = probe
                .newly_enabled
                .iter()
                .map(|enabled| (enabled.trigger.clone(), enabled.module.clone()))
                .collect();
            if waiting.is_empty() {
                return;
            }
            for (trigger, module) in waiting {
                self.fire(&Event::new(&trigger, &module));
            }
        }
        self.undecided.push("the world never settled".to_owned());
    }

    /// One turn of the engine, remembering what ran.
    fn fire(&mut self, event: &Event) -> StepOutcome {
        use inspect_sim::Disposition;
        let outcome = step(self.spec, self.program, self.sources, &self.world, event);
        for rule in &outcome.rules {
            match rule.disposition {
                Disposition::Fired => self.fired.push(rule.name.clone()),
                Disposition::Undecided => self.undecided.push(rule.name.clone()),
                Disposition::Refused | Disposition::Unsimulatable => {}
            }
        }
        self.world = outcome.world.clone();
        outcome
    }
}

/// An act, gathered so the walk does not take six arguments.
struct Act<'a> {
    trigger: &'a str,
    arguments: &'a [Term],
    creating: Option<&'a crate::journey::Cast>,
}

/// The line, as its author wrote it.
fn about(clause: &Clause) -> String {
    match clause {
        Clause::Does { actor, trigger, arguments, surface, .. } => {
            let args: Vec<String> = arguments.iter().map(Term::as_written).collect();
            format!("{actor} does {trigger}({}) on {surface}", args.join(", "))
        }
        Clause::After { text, .. } => format!("after {text}"),
        Clause::Then { assertion, .. } => format!("then {}", written(assertion)),
        Clause::Sees { actor, path, surface, negated, .. } => {
            let verb = if *negated { "cannot see" } else { "sees" };
            format!("{actor} {verb} {} on {surface}", path.as_written())
        }
        Clause::Stipulate { path, value, .. } => {
            format!("stipulate {} = {}", path.as_written(), value.as_written())
        }
    }
}

fn written(assertion: &Assertion) -> String {
    match assertion {
        Assertion::Compare { left, operator, right } => {
            format!("{} {} {}", left.as_written(), operator.as_str(), right.as_written())
        }
        Assertion::Within { needle, haystack } => {
            format!("{} in {}", needle.as_written(), haystack.as_written())
        }
        Assertion::Fires { rule, negated } => {
            format!("{rule}{}", if *negated { " does not fire" } else { " fires" })
        }
        Assertion::Exists { path, negated } => {
            format!("{}{}", path.as_written(), if *negated { " does not exist" } else { " exists" })
        }
    }
}

impl Walker<'_> {
    /// Evaluate one assertion against the world the last step left.
    fn assert(&self, assertion: &Assertion, line: usize, about: String) -> Outcome {
        let (truth, detail) = match assertion {
            Assertion::Compare { left, operator, right } => {
                let found = self.read(left);
                let wanted = self.value_of(right);
                (
                    compare(&found, *operator, &wanted),
                    Some(format!("{} is {}", left.as_written(), found.render())),
                )
            }
            Assertion::Within { needle, haystack } => {
                let wanted = self.value_of(needle);
                let inside = self.read(haystack);
                let truth = match &inside {
                    Value::Set(items) => Truth::any(items.iter().map(|item| item.equals(&wanted))),
                    _ => Truth::Unknown,
                };
                (truth, Some(format!("{} is {}", haystack.as_written(), inside.render())))
            }
            Assertion::Fires { rule, negated } => {
                let ran = self.fired.iter().any(|name| name == rule);
                // "did not run" and "could not be told whether it should" are
                // different answers, and only one of them is about the spec.
                if !ran && self.undecided.iter().any(|name| name == rule) {
                    (Truth::Unknown, Some(format!("`{rule}` could not be decided")))
                } else if !ran && self.waits_on_the_world(rule) {
                    // Nobody fires a state-condition rule; it becomes true or it
                    // does not. The simulator lists the ones that became true and
                    // says nothing about the rest, so a rule whose condition is
                    // *false* and one whose condition could not be *decided* look
                    // identical from here — and reporting the second as a flat no
                    // is the failure this whole design refuses.
                    (
                        Truth::Unknown,
                        Some(format!(
                            "`{rule}` never became true, and whether its condition is false or \
                             could not be decided is not visible from here"
                        )),
                    )
                } else {
                    (
                        Truth::from_bool(ran != *negated),
                        (!ran).then(|| format!("`{rule}` did not run")),
                    )
                }
            }
            Assertion::Exists { path, negated } => {
                let found =
                    matches!(self.read(path), Value::Ref(id) if self.world.instance(&id).is_some());
                (Truth::from_bool(found != *negated), None)
            }
        };

        let verdict = match truth {
            Truth::True => Verdict::Specified,
            // A false assertion is the spec doing something other than what
            // somebody said it should, which is the same thing as a refusal
            // from the reader's side: this journey is not what this spec does.
            Truth::False => Verdict::Refused,
            Truth::Unknown => Verdict::Undecided,
        };
        Outcome { line, verdict, about, detail: if truth == Truth::True { None } else { detail } }
    }

    /// Whether a rule waits on the world rather than on somebody acting.
    fn waits_on_the_world(&self, rule: &str) -> bool {
        use inspect_model::graph::TriggerSource;
        self.spec
            .nodes_of(NodeKind::Rule)
            .find(|node| node.name == rule)
            .and_then(|node| node.detail.as_rule())
            .is_some_and(|detail| {
                matches!(detail.source, TriggerSource::State | TriggerSource::Temporal)
            })
    }

    /// Can this actor observe this value here?
    ///
    /// The checker has already settled whether the surface carries it at all.
    /// What is left is whether there is a value to see, which needs a world —
    /// and whether the surface's own filter admits *this* actor, which needs
    /// the `exposes` clause as an expression rather than as text. That last
    /// part is not read yet, so an observation of a value that exists comes
    /// back undecided rather than true, and a `cannot see` of one comes back
    /// undecided rather than safe. A privacy claim that passes because nothing
    /// checked it is the worst answer this tool could give.
    fn observe(&self, path: &Path, negated: bool, line: usize, about: String) -> Outcome {
        let found = self.read(path);
        if found.is_unknown() {
            return Outcome {
                line,
                verdict: if negated { Verdict::Specified } else { Verdict::Undecided },
                about,
                detail: Some(format!("{} has no value here", path.as_written())),
            };
        }
        Outcome {
            line,
            verdict: Verdict::Undecided,
            about,
            detail: Some(format!(
                "{} is {} — whether this surface shows it to this actor is not read yet",
                path.as_written(),
                found.render()
            )),
        }
    }
}

/// Compare two values the way the assertion asked.
fn compare(found: &Value, operator: Comparison, wanted: &Value) -> Truth {
    use std::ops::Not;
    match operator {
        Comparison::Equal => found.equals(wanted),
        Comparison::NotEqual => found.equals(wanted).not(),
        _ => match found.compare(wanted) {
            Some(ordering) => Truth::from_bool(match operator {
                Comparison::Less => ordering.is_lt(),
                Comparison::LessOrEqual => ordering.is_le(),
                Comparison::Greater => ordering.is_gt(),
                Comparison::GreaterOrEqual => ordering.is_ge(),
                Comparison::Equal | Comparison::NotEqual => unreachable!("handled above"),
            }),
            // Two kinds that do not order is a question with no answer rather
            // than a question answered no.
            None => Truth::Unknown,
        },
    }
}
