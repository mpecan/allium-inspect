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
    Value,
    step::{Sources, StepOutcome},
    value::EntityId,
    world::{Answer, Event, World},
};

pub use crate::shapes::{
    CastMember, Ground, Inherited, Origin, Stipulation, Walk, Walked, notes_outside_steps,
};
pub(crate) use crate::shapes::{Settled, worst};
use crate::{
    assert::Sight,
    check::{self, Verdict},
    journey::{Assertion, Clause, Journey, Step, Subject, Term},
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

/// Walk `journey` against `spec`.
#[must_use]
/// Walk one journey against the specification.
///
/// `everything` is every journey that was read, because a journey may say it
/// continues from another and that one has to be found — across files, since
/// a name is unique across the set and a life does not stop at a file boundary.
pub fn walk(
    journey: &Journey,
    everything: &[Journey],
    spec: &SpecGraph,
    program: &Program,
    sources: &Sources,
) -> Walk {
    let checked = check::check(journey, everything, spec);
    let mut walker = Walker {
        spec,
        program,
        sources,
        world: inspect_sim::seed::seed(spec),
        bound: BTreeMap::new(),
        stipulated: Vec::new(),
        inherited: Vec::new(),
        after: None,
        standing_on: None,
        cast: Vec::new(),
        fired: Vec::new(),
        undecided: Vec::new(),
        notes: Vec::new(),
    };
    // The ground first, when this journey stands on some. Everything below —
    // the world it lays out, the names it binds, the steps it takes — happens
    // in the world the journey it follows left behind.
    if let Some(named) = &journey.after {
        walker.stand_on(named, everything, &mut vec![journey.name.clone()]);
    }
    walker.lay_out(journey);

    let steps: Vec<Walked> =
        journey.steps.iter().map(|step| walker.walk_step(step, &checked)).collect();

    let mut notes = walker.notes;
    notes.extend(notes_outside_steps(journey, &checked));
    notes.sort_by_key(|note| note.line);

    Walk {
        name: journey.name.clone(),
        title: crate::title::readable(&journey.name),
        cast: walker.cast,
        goal: journey.goal.clone(),
        ends: journey.ends.clone(),
        line: journey.line,
        steps,
        stipulated: walker.stipulated,
        inherited: walker.inherited,
        after: walker.after,
        notes,
    }
}

pub(crate) struct Walker<'a> {
    pub(crate) spec: &'a SpecGraph,
    pub(crate) program: &'a Program,
    pub(crate) sources: &'a Sources,
    pub(crate) world: World,
    /// Every name the journey has bound to something in the world.
    pub(crate) bound: BTreeMap<String, EntityId>,
    pub(crate) stipulated: Vec<Stipulation>,
    pub(crate) inherited: Vec<Inherited>,
    pub(crate) after: Option<Ground>,
    /// Which journey's steps are running, when it is not this one's.
    pub(crate) standing_on: Option<String>,
    /// Everybody the journey has named, in the order they were bound.
    pub(crate) cast: Vec<CastMember>,
    /// Rules that ran since the last clause, for `then … fires`.
    pub(crate) fired: Vec<String>,
    /// Rules that could not be decided since it, for the same.
    pub(crate) undecided: Vec<String>,
    /// Faults that belong to no step clause.
    ///
    /// A cast member whose type the spec does not have, or a `given` that
    /// wrote nothing. Both were computed and then dropped, because every
    /// consumer filtered notes by *clause* line and neither of these sits on
    /// one — so a journey whose cast is nobody reported as satisfied.
    pub(crate) notes: Vec<Outcome>,
}

impl Walker<'_> {
    /// One step: every clause in the order it was written.
    pub(crate) fn walk_step(&mut self, step: &Step, notes: &[check::Note]) -> Walked {
        let mut outcomes = Vec::new();
        for clause in &step.clauses {
            // A line the spec cannot support is not run. Firing an act at a
            // surface nobody specified would produce a second, confusing
            // failure about a world rather than the one that matters.
            let blocking: Vec<&check::Note> = notes
                .iter()
                .filter(|note| note.line == clause.line() && note.verdict != Verdict::Specified)
                .collect();
            if !blocking.is_empty() {
                // All of them, not the first. One line can name a surface the
                // spec does not have *and* an actor it does not have, and
                // showing one at a time means fixing one, re-running, and
                // discovering the next — which is the same walk three times.
                outcomes.extend(blocking.iter().map(|note| Outcome {
                    line: note.line,
                    verdict: note.verdict,
                    about: about(clause),
                    detail: Some(note.message.clone()),
                }));
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
        Walked {
            number: step.number,
            title: step.title.clone(),
            line: step.line,
            outcomes,
            world: self.world.clone(),
        }
    }

    fn walk_clause(&mut self, clause: &Clause) -> Outcome {
        let about = about(clause);
        match clause {
            Clause::Does { trigger, arguments, creating, negated, line, .. } => self.act(
                &Act { trigger, arguments, creating: creating.as_ref(), negated: *negated },
                *line,
                about,
            ),
            Clause::After { duration, line, .. } => self.advance(duration, *line, about),
            Clause::Then { assertion, line } => self.assert(assertion, *line, about),
            Clause::Sees { actor, subject, surface, context, negated, line } => self.observe(
                &Sight {
                    actor,
                    subject,
                    surface,
                    context: context.as_deref(),
                    negated: *negated,
                    line: *line,
                },
                about,
            ),
            Clause::Stipulate { subject, value, line } => {
                let value = self.value_of(value);
                let written = format!("{} = {}", subject.as_written(), value.render());
                match subject {
                    // Listed only once it landed. The ledger exists so a reader
                    // can see everything the journey was told rather than
                    // shown, and a line in it that never reached the world is a
                    // false receipt.
                    Subject::Path(path) => match self.assign(path, value) {
                        Ok(()) => {
                            self.stipulated.push(Stipulation {
                                said: written,
                                through: self.standing_on.clone(),
                            });
                            Outcome {
                                line: *line,
                                verdict: Verdict::Specified,
                                about,
                                detail: None,
                            }
                        }
                        Err(reason) => Outcome {
                            line: *line,
                            verdict: Verdict::Undecided,
                            about,
                            detail: Some(reason),
                        },
                    },
                    // A call writes nowhere: there is no field to set, which is
                    // the whole reason it needs saying. It is remembered and
                    // answered from when the specification asks.
                    Subject::Call { name, arguments } => {
                        let arguments: Vec<Value> =
                            arguments.iter().map(|term| self.value_of(term)).collect();
                        self.world.answers.push(Answer { call: name.clone(), arguments, value });
                        self.stipulated
                            .push(Stipulation { said: written, through: self.standing_on.clone() });
                        Outcome { line: *line, verdict: Verdict::Specified, about, detail: None }
                    }
                }
            }
        }
    }

    /// Fire an act and keep what it caught.
    fn act(&mut self, act: &Act<'_>, line: usize, about: String) -> Outcome {
        let Act { trigger, arguments, creating, negated } = *act;
        let module = trigger_module(self.spec, trigger);

        // Positional, matched against the trigger's declared parameters — which
        // is how the spec writes the act and how a person reads it back.
        let (parameters, _) = self.parameters_of(trigger);
        let mut event = Event::new(trigger, &module);
        for (at, argument) in arguments.iter().enumerate() {
            let name = parameters.get(at).cloned().unwrap_or_else(|| format!("arg{at}"));
            event = event.with(name, self.value_of(argument));
        }

        let before: Vec<EntityId> = self.world.entities.keys().cloned().collect();
        self.fired.clear();
        self.undecided.clear();
        let outcome = self.fire(&event);

        // `bruno cannot do MemberAsksToChat(…)`: the journey asserting a
        // refusal it *wants*, which without this reads as `refused` — the
        // right verdict about the spec and the wrong one about the journey.
        //
        // Fired anyway rather than asked hypothetically, and whatever happened
        // is kept. If the act the journey says is impossible turns out to be
        // possible, the world where it happened is the world the later steps
        // are now in, and hiding that would be the tool deciding which of the
        // two the reader would rather believe.
        if negated {
            return self.blocked(&outcome, line, about);
        }

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
                    self.bind(caught, Some(id), Origin::Caught);
                }
                None => {
                    // Listed anyway, with nothing behind it. A name the journey
                    // uses from here on that resolves to no instance is exactly
                    // what the reader is trying to understand, and leaving it
                    // out of the cast hides the cause of every later line.
                    self.bind(caught, None, Origin::Caught);
                    // The symptom, and then the cause. Nothing being created is
                    // almost never the fault: a rule that could not be decided
                    // creates nothing, and reporting only the empty hands sent
                    // a reader looking at their own journey for a mistake that
                    // was three lines up in the specification.
                    let missing = format!(
                        "nothing of kind `{}` was created for `{}`",
                        caught.type_expr, caught.name
                    );
                    return Outcome {
                        line,
                        verdict: verdict_of(&outcome),
                        about,
                        detail: Some(match refusal(&outcome) {
                            Some(why) => format!("{missing} — {why}"),
                            None => missing,
                        }),
                    };
                }
            }
        }

        Outcome { line, verdict: verdict_of(&outcome), about, detail: refusal(&outcome) }
    }

    /// What a `cannot do` came to: the verdicts, mirrored.
    ///
    /// Refused is what the journey asked for, so it is satisfied and says what
    /// blocked it — a reader wants to know *which* precondition held the line,
    /// because that is the sentence the block is made of.
    ///
    /// Undecided stays undecided, never a polite yes. A refusal that came back
    /// green because nothing could work out whether the act goes through is
    /// the same failure as a `cannot see` that passed unchecked, and this
    /// clause exists for exactly the cases somebody is relying on.
    fn blocked(&self, outcome: &StepOutcome, line: usize, about: String) -> Outcome {
        let (verdict, detail) = match verdict_of(outcome) {
            Verdict::Refused => (Verdict::Specified, refusal(outcome)),
            Verdict::Specified => (
                Verdict::Refused,
                Some(format!(
                    "`{}` went through",
                    self.fired.first().map_or("the act", String::as_str)
                )),
            ),
            other => (other, refusal(outcome)),
        };
        Outcome { line, verdict, about, detail }
    }

    /// The trigger's parameters, in the order the spec declares them.
    fn parameters_of(&self, trigger: &str) -> (Vec<String>, Vec<String>) {
        self.spec
            .nodes_of(NodeKind::Trigger)
            .find(|node| node.name == trigger)
            .and_then(|node| node.detail.as_trigger())
            .map(|detail| (detail.parameters.clone(), detail.optional.clone()))
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

        // Time passing is a claim about the world, so it answers like one.
        // It used to report `Specified` whatever happened underneath it: two
        // identical `a day passes` steps read differently depending only on
        // whether a `then … fires` line followed, because that line was the
        // only reader of what settling had found.
        if let Settled::No { rounds } = self.settle() {
            return Outcome {
                line,
                verdict: Verdict::Undecided,
                about,
                detail: Some(format!(
                    "the world had not stopped changing after {rounds} rounds, so what is \
                     true once time has passed is not settled"
                )),
            };
        }
        if self.undecided.is_empty() {
            Outcome { line, verdict: Verdict::Specified, about, detail: None }
        } else {
            Outcome {
                line,
                verdict: Verdict::Undecided,
                about,
                detail: Some(format!(
                    "{} could not be decided while the clock moved",
                    self.undecided.join(", ")
                )),
            }
        }
    }
}

/// An act, gathered so the walk does not take six arguments.
struct Act<'a> {
    trigger: &'a str,
    arguments: &'a [Term],
    creating: Option<&'a crate::journey::Cast>,
    /// Whether the journey said this *cannot* happen.
    negated: bool,
}

/// Whether this rule has already run for this instance.
///
/// Both halves, because neither alone is the question. A rule that holds for
/// two instances at once must run for each — one withdrawal calls off every
/// reservation waiting on that book, not the first one — so the trigger alone
/// would drop all but one, silently, in a walk that otherwise reads as passing.
/// And two rules watching the same instance are two separate things to do, so
/// the instance alone would drop one of those.
pub(crate) fn already_ran(ran: &[(String, Value)], trigger: &str, over: &Value) -> bool {
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
        Clause::Does { actor, trigger, arguments, surface, negated, .. } => {
            let args: Vec<String> = arguments.iter().map(Term::as_written).collect();
            let verb = if *negated { "cannot do" } else { "does" };
            format!("{actor} {verb} {trigger}({}) on {surface}", args.join(", "))
        }
        Clause::After { text, .. } => format!("after {text}"),
        Clause::Then { assertion, .. } => format!("then {}", written(assertion)),
        Clause::Sees { actor, subject, surface, context, negated, .. } => {
            let verb = if *negated { "cannot see" } else { "sees" };
            let at = context.as_ref().map_or_else(String::new, |it| format!(" in {it}"));
            format!("{actor} {verb} {} on {surface}{at}", subject.as_written())
        }
        Clause::Stipulate { subject, value, .. } => {
            format!("stipulate {} = {}", subject.as_written(), value.as_written())
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
