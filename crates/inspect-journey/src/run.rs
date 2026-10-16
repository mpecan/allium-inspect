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
    world::{Event, World},
};

pub use crate::shapes::{
    CastMember, Ground, Inherited, Origin, Stipulation, Walk, Walked, notes_outside_steps,
};
pub(crate) use crate::shapes::{Settled, worst};
use crate::{
    assert::Sight,
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
    let checked = check::check(journey, everything, spec, program);
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
        worlds: Vec::new(),
        cast: Vec::new(),
        fired: Vec::new(),
        undecided: Vec::new(),
        unapplied: Vec::new(),
        refused: Vec::new(),
        created: Vec::new(),
        removed: BTreeMap::new(),
        awaiting: None,
        landed: Vec::new(),
        unheard: Vec::new(),
        answered: Vec::new(),
        refused_in_the_world: Vec::new(),
        cut_off: None,
        listed: None,
        firings: 0,
        moved: false,
        notes: Vec::new(),
        unwritten: Vec::new(),
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
    let outside = notes_outside_steps(journey, &checked);
    // A `given` the walk left unwritten, where the checker said nothing about
    // that line — so nothing is dropped without a word, and nothing is said
    // twice.
    let unsaid: Vec<Outcome> = walker
        .unwritten
        .into_iter()
        .filter(|unwritten| !outside.iter().any(|note| note.line == unwritten.line))
        .collect();
    notes.extend(outside);
    notes.extend(unsaid);
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

/// A rule the engine could not decide, and the first reason it gave.
#[derive(Debug, Clone, Default)]
pub(crate) struct Undecided {
    pub(crate) rule: String,
    pub(crate) why: String,
    /// The calls nothing answered when it ran, as the spec wrote them:
    /// `audience(issue)`. What a remark about an answer matches against.
    pub(crate) asked: Vec<String>,
    /// The engine turn of its act it ran in.
    pub(crate) turn: usize,
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
    /// Every file world this walk has laid out, so a chain lays each out once.
    pub(crate) worlds: Vec<crate::journey::Shared>,
    /// Everybody the journey has named, in the order they were bound.
    pub(crate) cast: Vec<CastMember>,
    /// Rules that ran since the last clause, for `then … fires`.
    pub(crate) fired: Vec<String>,
    /// Rules that could not be decided since it, for the same, and why.
    pub(crate) undecided: Vec<Undecided>,
    /// Rules that fired since it and left part of what they ensure unapplied,
    /// and the first thing they left. Apart from `undecided`: these did run,
    /// and `then Fan fires` saying so beside "`Fan` could not be decided"
    /// was two answers to one question.
    pub(crate) unapplied: Vec<Undecided>,
    /// Rules that were woken and refused since it, and the clauses that did.
    pub(crate) refused: Vec<Undecided>,
    /// Every instance made since it, in the order it was made — the act's own
    /// first, then whatever it set off.
    pub(crate) created: Vec<EntityId>,
    /// Every instance removed since it, and the rule that removed it first.
    pub(crate) removed: BTreeMap<EntityId, String>,
    /// An act's `where` answers, until what they are about is made.
    pub(crate) awaiting: Option<crate::answers::Awaiting>,
    /// The `where` answers that landed since the last clause, and on what.
    pub(crate) landed: Vec<crate::answers::Landed>,
    /// Why a `where` answer was refused once its instance landed, for the
    /// act's line to say.
    pub(crate) unheard: Vec<String>,
    /// Every answer `given` or a file's world put in force, and the line that
    /// said it — so a later one about the same call can say what it replaced.
    pub(crate) answered: Vec<crate::answers::FromTheStart>,
    /// Lines of a file's world that answered a call with something that had
    /// no value, and so put nothing in force. Kept past the walk that laid the
    /// world out, because a journey continuing from it does not lay it out
    /// again and would otherwise list the line `from the file`.
    pub(crate) refused_in_the_world: Vec<usize>,
    /// A cascade since it that was cut off before it settled, and where. Kept
    /// apart from `undecided`: it names triggers still waiting, not a rule.
    pub(crate) cut_off: Option<String>,
    /// Since it, whether the clock settled the world to the end — and if so,
    /// every state rule some round's listing could not decide for an instance.
    /// A rule settled to the end and not in here was decided *false* wherever
    /// it was asked, so `then … does not fire` about it has an answer.
    pub(crate) listed: Option<std::collections::BTreeSet<String>>,
    /// Events fired since it, the act's own included, against the budget that
    /// stops a cascade that grows faster than the round bound can catch.
    pub(crate) firings: usize,
    /// Whether the step being walked has acted or moved the clock yet. What a
    /// step set off belongs to that step: a later step's `then` answered by a
    /// `stipulate` is the journey's own words, not the cascade's.
    pub(crate) moved: bool,
    /// Faults that belong to no step clause.
    ///
    /// A cast member whose type the spec does not have, or a `given` that
    /// wrote nothing. Both were computed and then dropped, because every
    /// consumer filtered notes by *clause* line and neither of these sits on
    /// one — so a journey whose cast is nobody reported as satisfied.
    pub(crate) notes: Vec<Outcome>,
    /// `given` lines the walk did not write, and why: see `Walker::unwritten`.
    pub(crate) unwritten: Vec<Outcome>,
}

impl Walker<'_> {
    /// One step: every clause in the order it was written.
    pub(crate) fn walk_step(&mut self, step: &Step, notes: &[check::Note]) -> Walked {
        let mut outcomes = Vec::new();
        let mut asides = Vec::new();
        self.moved = false;
        for clause in &step.clauses {
            // What `check` remarked on this line, shown beside the step and
            // never as its verdict (see `Walked::asides`).
            asides.extend(
                notes
                    .iter()
                    .filter(|note| note.line == clause.line() && note.verdict == Verdict::Remark)
                    .map(|note| Outcome {
                        line: note.line,
                        verdict: note.verdict,
                        about: about(clause),
                        detail: Some(note.message.clone()),
                    }),
            );
            // A line the spec cannot support is not run. Firing an act at a
            // surface nobody specified would produce a second, confusing
            // failure about a world rather than the one that matters.
            //
            // A remark is not that: it is worth reading and is no reason to
            // stop, and the walk has the better answer to what one says — an
            // act setting off an emission a listener cannot bind is run, and
            // the listener is reported undecided if the emission is reached.
            // It is carried as an aside, above.
            let blocking: Vec<&check::Note> = notes
                .iter()
                .filter(|note| note.line == clause.line())
                .filter(|note| !matches!(note.verdict, Verdict::Specified | Verdict::Remark))
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
            // A stipulation that landed and has something to say about when
            // it was said: worth reading, and no reason the step fails.
            if matches!(clause, Clause::Stipulate { .. })
                && outcome.verdict == Verdict::Specified
                && let Some(detail) = outcome.detail.take()
            {
                asides.push(Outcome {
                    verdict: Verdict::Remark,
                    detail: Some(detail),
                    ..outcome.clone()
                });
            }
            // An act whose `where` answered a call something still asked
            // unanswered: which of the two ways, beside the act.
            //
            // And a `where` answer refused once its instance landed: the
            // journey said something the walk could not put in force. That is
            // a line of the step, not an aside — undecided in its verdict, as
            // a `stipulate` refused for the same reason is — or a step whose
            // act held would read "holds" over an answer it never gave.
            let mut unheard = Vec::new();
            if matches!(clause, Clause::Does { .. }) {
                asides.extend(self.unanswered().into_iter().map(|detail| Outcome {
                    verdict: Verdict::Remark,
                    detail: Some(detail),
                    ..outcome.clone()
                }));
                unheard = std::mem::take(&mut self.unheard);
            }
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
            let refused: Vec<Outcome> = unheard
                .into_iter()
                .map(|detail| Outcome {
                    verdict: Verdict::Undecided,
                    detail: Some(detail),
                    ..outcome.clone()
                })
                .collect();
            outcomes.push(outcome);
            outcomes.extend(refused);
        }
        Walked {
            number: step.number,
            title: step.title.clone(),
            line: step.line,
            outcomes,
            asides,
            world: self.world.clone(),
        }
    }

    fn walk_clause(&mut self, clause: &Clause) -> Outcome {
        let about = about(clause);
        match clause {
            Clause::Does { actor, trigger, arguments, surface, creating, negated, line } => self
                .act(
                    &Act { actor, trigger, arguments, surface, creating, negated: *negated },
                    *line,
                    about,
                ),
            Clause::After { duration, line, .. } => self.advance(duration, *line, about),
            Clause::Then { assertion, line } => self.assert(assertion, *line, about),
            Clause::Sees { actor, subject, surface, contexts, negated, line } => self.observe(
                &Sight { actor, subject, surface, contexts, negated: *negated, line: *line },
                about,
            ),
            Clause::Stipulate { subject, value, line } => {
                self.stipulate(subject, value, *line, about)
            }
        }
    }

    /// An act, once the surface it is on has been asked whether it faces
    /// whoever does it (see `facing`).
    fn act(&mut self, act: &Act<'_>, line: usize, about: String) -> Outcome {
        self.forget();
        self.moved = true;
        self.at_the_surface(act, line, about)
    }

    /// Fire an act and keep what it caught.
    pub(crate) fn fire_act(&mut self, act: &Act<'_>, line: usize, about: String) -> Outcome {
        let Act { trigger, arguments, creating, negated, .. } = *act;
        let module = trigger_module(self.spec, trigger);

        // Positional, as the spec writes the act and a person reads it back:
        // each rule that wakes takes them under its own `when` clause's names,
        // and two of them may name one argument differently.
        let mut event = Event::new(trigger, &module);
        for argument in arguments {
            event = event.passing(self.value_of(argument));
        }

        // A `where` under `creating` is in force from the moment what it is
        // about is made, so the cascade that follows reads it.
        self.awaiting = crate::answers::Awaiting::of(creating);
        let named: Vec<Option<EntityId>> =
            creating.iter().map(|caught| self.bound.get(&caught.cast.name).cloned()).collect();
        let outcome = self.fire(&event);
        // An answer still waiting on another line's instance once the act is
        // over is refused, never weighed against what that name meant before.
        self.refuse_what_still_waits();

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

        // Each line catches its own instance, and one that caught nothing is
        // said about by name: the others caught what they caught.
        let made = self.made_for(creating);
        let mut missing: Vec<String> = Vec::new();
        for ((caught, id), before) in creating.iter().zip(made).zip(named) {
            let caught = &caught.cast;
            if let Some(id) = id {
                self.bind(caught, Some(id), Origin::Caught);
                continue;
            }
            // A `where` bound the name early, to an instance the act then
            // removed: what it named before is what it names.
            match before {
                Some(id) => self.bound.insert(caught.name.clone(), id),
                None => self.bound.remove(&caught.name),
            };
            // Listed anyway, with nothing behind it. A name the journey uses
            // from here on that resolves to no instance is exactly what the
            // reader is trying to understand, and leaving it out of the cast
            // hides the cause of every later line.
            self.bind(caught, None, Origin::Caught);
            // The symptom, and then the cause. Nothing being created is almost
            // never the fault: a rule that could not be decided creates
            // nothing, and reporting only the empty hands sent a reader
            // looking at their own journey for a mistake that was three lines
            // up in the specification.
            missing.push(format!(
                "nothing of kind `{}` was created for `{}`",
                caught.type_expr, caught.name
            ));
        }
        let missing = (!missing.is_empty()).then(|| missing.join("; "));

        let verdict = verdict_of(&outcome);
        // The act's own rule fired, and something it set off could not be
        // decided. That rule is on no line of the journey, so without this the
        // step held — and a `then` about what it would have written read as
        // the spec answering, either way, when nothing had been decided.
        // Undecided here, the same as the clock moving into a rule nobody
        // could decide, and every refusal after it in this step with it.
        //
        // With a `creating` that caught nothing it is usually the cause: `for
        // x in config.chosen` over a parameter nothing settled made nothing,
        // and read as the rule refusing nothing that was a held line with an
        // empty name behind it (issue #25).
        let (verdict, why) = if verdict == Verdict::Specified
            && let Some(why) = self.unsettled()
        {
            (Verdict::Undecided, Some(why))
        } else {
            (verdict, refusal(&outcome, |note| self.said_of_the_cast(note)))
        };
        let detail = match (missing, why) {
            (Some(missing), Some(why)) => Some(format!("{missing} — {why}")),
            (missing, why) => missing.or(why),
        };
        Outcome { line, verdict, about, detail }
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
            Verdict::Refused => {
                (Verdict::Specified, refusal(outcome, |note| self.said_of_the_cast(note)))
            }
            Verdict::Specified => (
                Verdict::Refused,
                Some(format!(
                    "`{}` went through",
                    self.fired.first().map_or("the act", String::as_str)
                )),
            ),
            other => (other, refusal(outcome, |note| self.said_of_the_cast(note))),
        };
        Outcome { line, verdict, about, detail }
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
        self.forget();
        self.moved = true;

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
        // What the clock set off is said the way an act's is — each rule
        // once, with its reason, beside what fired and was not applied in full
        // and a cascade that never settled — under the clock's name.
        match self.unsettled() {
            None => Outcome { line, verdict: Verdict::Specified, about, detail: None },
            Some(why) => Outcome {
                line,
                verdict: Verdict::Undecided,
                about,
                detail: Some(format!("while the clock moved, {why}")),
            },
        }
    }
}

/// An act, gathered so the walk does not take six arguments.
pub(crate) struct Act<'a> {
    pub(crate) actor: &'a str,
    pub(crate) trigger: &'a str,
    pub(crate) arguments: &'a [Term],
    pub(crate) surface: &'a str,
    /// One per `creating` line, each with what its `where` lines say calls
    /// answer for the instance it names.
    pub(crate) creating: &'a [crate::journey::Caught],
    /// Whether the journey said this *cannot* happen.
    pub(crate) negated: bool,
}

/// Whether this rule has already run for this instance.
///
/// Both halves, because neither alone is the question. A rule that holds for
/// two instances at once must run for each — one withdrawal calls off every
/// reservation waiting on that book, not the first one — so the trigger alone
/// would drop all but one, silently, in a walk that otherwise reads as passing.
/// And two rules watching the same instance are two separate things to do, so
/// the instance alone would drop one of those.
pub(crate) fn already_ran(ran: &[(String, Value)], rule: &str, over: &Value) -> bool {
    ran.iter().any(|(before, instance)| before == rule && instance == over)
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
        Clause::Sees { actor, subject, surface, contexts, negated, .. } => {
            let verb = if *negated { "cannot see" } else { "sees" };
            let at = if contexts.is_empty() {
                String::new()
            } else {
                format!(" in {}", contexts.join(", "))
            };
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
