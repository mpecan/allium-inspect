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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

use inspect_model::{NodeKind, Program, SpecGraph};
use inspect_sim::{
    Value, enabled,
    step::{Sources, StepOutcome, step},
    value::EntityId,
    world::{Event, World},
};

use crate::{
    check::{self, Verdict},
    journey::{Assertion, Clause, Journey, Step, Term},
    outcome::{refusal, verdict_of},
};

/// What became of one line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Outcome {
    pub line: usize,
    pub verdict: Verdict,
    /// What the line said, as the author wrote it.
    pub about: String,
    /// Why, when there is a why worth reading.
    pub detail: Option<String>,
}

/// What became of one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Walk {
    pub name: String,
    /// What the journey said it was for, and how it said it ends.
    ///
    /// Carried on the report rather than left in the source, because the whole
    /// question a reader has is whether the spec delivers *this* — and a list
    /// of verdicts with the intent stripped off cannot be read against it.
    pub goal: Vec<String>,
    pub ends: Vec<String>,
    /// Where the journey starts in its file, for going there.
    pub line: usize,
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

    Walk {
        name: journey.name.clone(),
        goal: journey.goal.clone(),
        ends: journey.ends.clone(),
        line: journey.line,
        steps,
        stipulated: walker.stipulated,
    }
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
        let module = trigger_module(self.spec, trigger);

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
        let mut ran: Vec<(String, Value)> = Vec::new();
        for _ in 0..ROUNDS {
            // Everything the world makes true, not everything this step made
            // newly true: a rule enabled before the clock moved and never run
            // is still waiting, and a journey that skipped it would report a
            // world the spec does not describe.
            let waiting: Vec<(String, String, String, Value)> =
                enabled(self.spec, self.program, self.sources, &self.world)
                    .into_iter()
                    .flat_map(|rule| {
                        let (trigger, module, binding) = (rule.trigger, rule.module, rule.binding);
                        rule.over.into_iter().map(move |over| {
                            (trigger.clone(), module.clone(), binding.clone(), over)
                        })
                    })
                    // A rule already run for that same instance is where the
                    // fixpoint comes from. Without it a rule whose effect keeps
                    // its own condition true — `status = lost` stays lost —
                    // runs thirty-two times and then reports never settling.
                    .filter(|(trigger, _, _, over)| !already_ran(&ran, trigger, over))
                    .collect();
            if waiting.is_empty() {
                return;
            }
            for (trigger, module, binding, over) in waiting {
                let mut event = Event::new(&trigger, &module);
                // Under the name the `when` clause gave it. A state rule's
                // clauses are written about `copy`, and firing without that
                // binding evaluates every one of them against nothing.
                event.arguments.insert(binding, over.clone());
                self.fire(&event);
                ran.push((trigger, over));
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

/// Whether this rule has already run for this instance.
///
/// Both halves, because neither alone is the question. A rule that holds for
/// two instances at once must run for each — one withdrawal calls off every
/// reservation waiting on that book, not the first one — so the trigger alone
/// would drop all but one, silently, in a walk that otherwise reads as passing.
/// And two rules watching the same instance are two separate things to do, so
/// the instance alone would drop one of those.
fn already_ran(ran: &[(String, Value)], trigger: &str, over: &Value) -> bool {
    ran.iter().any(|(before, instance)| before == trigger && instance == over)
}

/// Where `trigger` is declared, for the event's label.
///
/// Among triggers only. An entity and a state-condition rule's trigger share a
/// name, and the entity is very often declared in a different module from the
/// rule that watches it — `Copy` is the catalogue's, and lending is what
/// reports it lost.
fn trigger_module(spec: &SpecGraph, trigger: &str) -> String {
    spec.nodes_of(NodeKind::Trigger)
        .find(|node| node.name == trigger)
        .map_or_else(String::new, |node| node.module.clone())
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

#[cfg(test)]
mod tests {
    use inspect_model::Node;

    use super::*;

    fn reference(name: &str) -> Value {
        Value::Ref(EntityId::new(name, 1))
    }

    #[test]
    fn a_rule_that_already_ran_for_this_instance_does_not_run_again() {
        // Where the fixpoint terminates. `CancelReservationOnWithdrawal`
        // watches the book, and cancelling a reservation does not un-withdraw
        // it, so it is enabled again every round forever.
        let ran = vec![("Reservation".to_owned(), reference("Reservation"))];
        assert!(already_ran(&ran, "Reservation", &reference("Reservation")));
    }

    #[test]
    fn the_same_rule_still_runs_for_a_different_instance() {
        // Two readers waiting on one book. Keyed on the rule alone, the second
        // reader's reservation is never called off — and nothing in the walk
        // says so, because the rule did run.
        let ran = vec![("Reservation".to_owned(), reference("Reservation"))];
        assert!(!already_ran(&ran, "Reservation", &Value::Ref(EntityId::new("Reservation", 2))));
    }

    #[test]
    fn a_different_rule_still_runs_for_the_same_instance() {
        // Two rules can watch one entity — one on its status, one on its clock
        // — and they are two separate things to do.
        let ran = vec![("Loan".to_owned(), reference("Loan"))];
        assert!(!already_ran(&ran, "Copy", &reference("Loan")));
    }

    #[test]
    fn nothing_has_run_yet_in_the_first_round() {
        assert!(!already_ran(&[], "Reservation", &reference("Reservation")));
    }

    #[test]
    fn an_act_is_labelled_with_the_module_that_declares_its_trigger() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(Node::new("catalogue", NodeKind::Entity, "MemberBorrows"));
        spec.nodes.push(Node::new("lending", NodeKind::Trigger, "MemberBorrows"));
        spec.nodes.push(Node::new("catalogue", NodeKind::Trigger, "LibrarianAddsBook"));
        assert_eq!(trigger_module(&spec, "MemberBorrows"), "lending");
        assert_eq!(trigger_module(&spec, "LibrarianAddsBook"), "catalogue");
    }

    #[test]
    fn a_trigger_the_spec_does_not_declare_has_no_module() {
        // Reported elsewhere as an unspecified act. Guessing a module here
        // would send the event to rules that were never asked for it.
        assert_eq!(trigger_module(&SpecGraph::new("test"), "MemberYodels"), "");
    }

    #[test]
    fn a_walk_is_only_as_good_as_its_worst_step() {
        // The summary line, and the exit code in strict mode. A journey with
        // one unsupported step among nine is not a journey that passes.
        assert_eq!(worst([].into_iter()), Verdict::Specified);
        assert_eq!(worst([Verdict::Specified, Verdict::Undecided].into_iter()), Verdict::Undecided);
        assert_eq!(worst([Verdict::Refused, Verdict::Specified].into_iter()), Verdict::Refused);
    }

    #[test]
    fn what_the_spec_forbids_outranks_what_it_never_said() {
        // Both fail, and a reader can only act on one at a time. A refusal is
        // a disagreement about behaviour that is specified; the rest are gaps.
        // Ordering them puts the disagreement first.
        let all = [
            Verdict::Specified,
            Verdict::Remark,
            Verdict::Unexposed,
            Verdict::Undecided,
            Verdict::Unspecified,
            Verdict::Refused,
        ];
        for (at, worse) in all.iter().enumerate() {
            for better in &all[..at] {
                assert_eq!(worst([*better, *worse].into_iter()), *worse, "{better:?} {worse:?}");
                assert_eq!(worst([*worse, *better].into_iter()), *worse, "{worse:?} {better:?}");
            }
        }
    }
}
