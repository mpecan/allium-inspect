//! What a journey says a call answers: from the start, in `given` or a file's
//! `world` (issue #19), and for an instance an act is about to make.
//!
//! ```text
//! repo does Open(repo, "crash") on Tracker
//!     creating issue: Issue
//!     where audience(issue) = {}
//! ```
//!
//! A reaction to the issue being made asks `audience(issue)`, a function the
//! specification names and never defines. `stipulate` is how a journey answers
//! such a call, and here it cannot: before the act there is no issue to name,
//! and after it the reaction has already run and been left undecided. So the
//! answer is written under the act and comes into force *during* it — once
//! the engine turn that made the instance `creating` will catch is over, before
//! anything that creation sets off is fired. The rule that made it has already
//! run by then, so it cannot read the answer, and the walk says so beside the
//! act.
//!
//! It is a stipulation, and is listed in the ledger like one, once it lands. If
//! the act makes nothing of that kind, nothing was answered and nothing is
//! listed: a receipt for an answer no call could have read is a false one.

use inspect_sim::{Value, step::RuleOutcome, value::EntityId, world::Answer};

use crate::{
    Outcome,
    check::Verdict,
    journey::{Cast, Subject, Term, Where},
    run::{Stipulation, Undecided, Walker},
};

/// An answer `given` or a file's world put in force, by what it resolved to.
///
/// Kept by resolved arguments rather than by the words: `allowed(n.who)`
/// written in the world while `n.who` was `b` is an answer about `b`, and the
/// same words in a journey that moved `n.who` to `c` are about `c` — another
/// call, which overrides nothing.
#[derive(Debug, Clone)]
pub(crate) struct FromTheStart {
    call: String,
    arguments: Vec<Value>,
    /// The line that said it, in the file.
    line: usize,
    /// Whether the file's world said it rather than a journey.
    by_the_file: bool,
    /// The journey whose `given` said it, when one did, and the ledger line.
    standing_on: Option<String>,
    said: String,
}

/// A `where` answer that landed, and what it landed on.
#[derive(Debug, Clone)]
pub(crate) struct Landed {
    /// `audience`.
    call: String,
    /// `audience(issue) = {}`, as the ledger lists it.
    said: String,
    /// The name `creating` caught, and the instance it named when it landed.
    name: String,
    id: EntityId,
    /// The engine turn that made it: a rule in that turn asked before it.
    turn: usize,
}

/// The calls a rule asked that nothing answered, as the spec wrote them.
///
/// A note about a call is one whose sub-expression is that call and whose
/// reason names the function: the simulator's "nothing defines `audience`".
/// A loop over it adds a note of its own, quoting the call too — that one says
/// the clauses inside were not applied, not that nobody answered, and an
/// answer the journey gave that read as unknown would leave only that.
pub(crate) fn asked(rule: &RuleOutcome) -> Vec<String> {
    let mut asked: Vec<String> = Vec::new();
    for note in &rule.unresolved {
        let Some(expression) = &note.expression else {
            continue;
        };
        let Some((name, _)) = expression.split_once('(') else {
            continue;
        };
        if note.reason.contains(&format!("`{}`", name.trim())) && !asked.contains(expression) {
            asked.push(expression.clone());
        }
    }
    asked
}

/// The first term of an answer about a call with no value, and what that
/// leaves the line doing — or `None` when every term has one.
///
/// One test for all three ways a journey answers a call — `given` or a file's
/// world, a `stipulate`, and a `where` under `creating` — so none of them can
/// list a line the others refuse. The simulator never matches an unknown
/// argument, and an unknown answer settles nothing: either way the ledger
/// would list a line that answers nothing, two lines above a rule saying it
/// was not answered — a false receipt.
pub(crate) fn unsettled<'a>(
    (terms, arguments): (&'a [Term], &[Value]),
    (term, value): (&'a Term, &Value),
) -> Option<(&'a Term, &'static str)> {
    terms
        .iter()
        .zip(arguments)
        .find(|(_, value)| value.holds_unknown())
        .map(|(term, _)| (term, "answers no call the specification can ask"))
        .or_else(|| value.holds_unknown().then_some((term, "gives the call no answer")))
}

/// A rule left unsettled in engine turn `turn`, with the calls it asked.
pub(crate) fn noted(rule: &RuleOutcome, why: String, turn: usize) -> Undecided {
    Undecided { rule: rule.name.clone(), why, asked: asked(rule), turn }
}

/// Whether `expression` is a call to `call`: `reach(issue)` is one to `reach`,
/// and `outreach(issue)` is not.
fn calls(expression: &str, call: &str) -> bool {
    expression.split_once('(').is_some_and(|(name, _)| name.trim() == call)
}

/// The first call `rule` left unanswered that is a call to `call`.
fn asked_of<'a>(rule: &'a Undecided, call: &str) -> Option<&'a str> {
    rule.asked.iter().find(|expression| calls(expression, call)).map(String::as_str)
}

/// The answers an act in progress will give once the instance they are about
/// exists.
#[derive(Debug, Clone)]
pub(crate) struct Awaiting {
    caught: Cast,
    answering: Vec<Where>,
}

impl Awaiting {
    /// `None` when there is nothing to wait for: no `creating`, or no `where`.
    pub(crate) fn of(caught: Option<&Cast>, answering: &[Where]) -> Option<Self> {
        let caught = caught?;
        if answering.is_empty() {
            return None;
        }
        Some(Self { caught: caught.clone(), answering: answering.to_vec() })
    }
}

impl Walker<'_> {
    /// The instance `creating` catches, if the act has made it yet.
    ///
    /// The first made, not the first in the world's order: ids sort as text,
    /// so `Order#10` comes before `Order#9`, and a reaction that made another
    /// order would be caught instead of the act's own whenever the count
    /// crossed a digit.
    pub(crate) fn first_made(&self, caught: &Cast) -> Option<EntityId> {
        let bare = caught.type_expr.rsplit('/').next().unwrap_or(&caught.type_expr);
        self.created
            .iter()
            .find(|id| id.entity() == bare && self.world.instance(id).is_some())
            .cloned()
    }

    /// Put the act's `where` answers in force, if what they are about has
    /// just been made. Called after every turn of the engine during an act;
    /// once they land they are not waited for again.
    pub(crate) fn answer_what_was_made(&mut self) {
        let Some(awaiting) = &self.awaiting else {
            return;
        };
        let Some(id) = self.first_made(&awaiting.caught) else {
            return;
        };
        let Some(Awaiting { caught, answering }) = self.awaiting.take() else {
            return;
        };
        // Bound now rather than once the act is over, so the answer's
        // arguments read as the instance. The cast entry is still made by the
        // act, once, when it catches the same instance.
        self.bound.insert(caught.name.clone(), id.clone());
        for answer in answering {
            let arguments: Vec<Value> =
                answer.arguments.iter().map(|term| self.value_of(term)).collect();
            let value = self.value_of(&answer.value);
            // Refused, as `given` refuses it, and said on the act's line: the
            // answer could only be weighed once the instance landed.
            if let Some((term, consequence)) =
                unsettled((&answer.arguments, &arguments), (&answer.value, &value))
            {
                self.unheard.push(format!(
                    "`{}` has no value once `{}` is made, so `where {}` {consequence}",
                    term.as_written(),
                    caught.name,
                    answer.as_written()
                ));
                continue;
            }
            // In the journey's words, as `given` and `stipulate` list a call:
            // `{repo}`, not the world's `{Repo#1}`.
            let said = format!("{} = {}", answer.as_written(), answer.value.as_written());
            self.world.tell(Answer { call: answer.call.clone(), arguments, value });
            self.landed.push(Landed {
                call: answer.call.clone(),
                said: said.clone(),
                name: caught.name.clone(),
                id: id.clone(),
                turn: self.firings,
            });
            self.stipulated.push(Stipulation { said, through: self.standing_on.clone() });
        }
    }

    /// Put a `given` answer in force, or say why it could not be.
    ///
    /// Listed once it lands, like every stipulation, when `listed` — the
    /// journey wrote it. The file's world lists its own lines `from the file`.
    ///
    /// An argument or an answer with no value is refused rather than
    /// recorded (see [`unsettled`]). A refused world line is left out of the
    /// inherited ledger, by [`Self::lay_out`], in every journey that takes the
    /// world.
    ///
    /// An answer replacing one the file's world gave marks that world line
    /// overridden; one replacing an earlier line in the same journey's
    /// `given` takes its place in the ledger. Decided by what was replaced,
    /// not by the words, which may read the same and be about another value.
    pub(crate) fn answer_from_the_start(&mut self, answer: &Where, line: usize, listed: bool) {
        let arguments: Vec<Value> =
            answer.arguments.iter().map(|term| self.value_of(term)).collect();
        let value = self.value_of(&answer.value);
        if let Some((term, consequence)) =
            unsettled((&answer.arguments, &arguments), (&answer.value, &value))
        {
            self.notes.push(Outcome {
                line,
                verdict: crate::check::Verdict::Undecided,
                about: format!("given {}", answer.as_written()),
                detail: Some(format!(
                    "`{}` has no value when the world is laid out, so this {consequence}",
                    term.as_written()
                )),
            });
            if !listed {
                self.refused_in_the_world.push(line);
            }
            return;
        }
        let said = format!("{} = {}", answer.as_written(), answer.value.as_written());
        self.world.tell(Answer { call: answer.call.clone(), arguments: arguments.clone(), value });
        let replaced: Vec<FromTheStart> = self
            .answered
            .iter()
            .filter(|earlier| {
                earlier.call == answer.call
                    && inspect_sim::world::same_arguments(&earlier.arguments, &arguments)
            })
            .cloned()
            .collect();
        for earlier in replaced {
            if earlier.by_the_file {
                // Only this journey's own lines mark the world's, as with a
                // field: the ledger is about what *this* journey changed.
                if listed {
                    for inherited in &mut self.inherited {
                        if inherited.line == earlier.line {
                            inherited.overridden = true;
                        }
                    }
                }
            } else if earlier.standing_on == self.standing_on
                && let Some(gone) = self.stipulated.iter().position(|told| {
                    told.said == earlier.said && told.through == earlier.standing_on
                })
            {
                self.stipulated.remove(gone);
            }
        }
        self.answered.push(FromTheStart {
            call: answer.call.clone(),
            arguments,
            line,
            by_the_file: !listed,
            standing_on: self.standing_on.clone(),
            said: said.clone(),
        });
        if listed {
            self.stipulated.push(Stipulation { said, through: self.standing_on.clone() });
        }
    }

    /// `stipulate <path> = <value>`, or `stipulate <call>(…) = <value>`: what
    /// the journey says outright rather than shows.
    pub(crate) fn stipulate(
        &mut self,
        subject: &Subject,
        stipulated: &Term,
        line: usize,
        about: String,
    ) -> Outcome {
        let value = self.value_of(stipulated);
        let written = format!("{} = {}", subject.as_written(), value.render());
        match subject {
            // Listed only once it landed. The ledger exists so a reader
            // can see everything the journey was told rather than
            // shown, and a line in it that never reached the world is a
            // false receipt.
            Subject::Path(path) => match self.assign(path, value) {
                Ok(()) => {
                    self.stipulated
                        .push(Stipulation { said: written, through: self.standing_on.clone() });
                    Outcome { line, verdict: Verdict::Specified, about, detail: None }
                }
                Err(crate::config::Unwritten { verdict, reason }) => {
                    Outcome { line, verdict, about, detail: Some(reason) }
                }
            },
            // A call writes nowhere: there is no field to set, which is
            // the whole reason it needs saying. It is remembered and
            // answered from when the specification asks.
            Subject::Call { name, arguments: terms } => {
                let arguments: Vec<Value> = terms.iter().map(|term| self.value_of(term)).collect();
                // Refused, as `given` refuses it, and not listed.
                if let Some((term, consequence)) =
                    unsettled((terms, &arguments), (stipulated, &value))
                {
                    return Outcome {
                        line,
                        verdict: Verdict::Undecided,
                        about,
                        detail: Some(format!(
                            "`{}` has no value here, so this {consequence}",
                            term.as_written()
                        )),
                    };
                }
                let detail = self.too_late(name, terms);
                self.world.tell(Answer { call: name.clone(), arguments, value });
                // In the journey's words, as every answer to a call is
                // listed, wherever it is said.
                let said = format!("{} = {}", subject.as_written(), stipulated.as_written());
                self.stipulated.push(Stipulation { said, through: self.standing_on.clone() });
                Outcome { line, verdict: Verdict::Specified, about, detail }
            }
        }
    }

    /// Why a call a `where` answered was still left unanswered by this act.
    ///
    /// Two ways, and the walk tells them apart by turn. The rule that made the
    /// instance asked in the same turn, before the answer was in force; or a
    /// rule asked about something else — another instance of the kind — that
    /// the answer, about the one caught, does not cover. Either way the step
    /// is undecided while the ledger lists the answer two lines above, and
    /// without this the report reads as contradicting itself.
    pub(crate) fn unanswered(&self) -> Vec<String> {
        let mut said = Vec::new();
        for landed in &self.landed {
            for rule in self.undecided.iter().chain(&self.unapplied) {
                let Some(expression) = asked_of(rule, &landed.call) else {
                    continue;
                };
                let remark = if rule.turn <= landed.turn {
                    format!(
                        "`{}` asked `{expression}` as it made `{}`, before `where {}` was in \
                         force — a `where` answers what that creation sets off, not the rule \
                         that makes it",
                        rule.rule, landed.name, landed.said
                    )
                } else {
                    format!(
                        "`{}` asked `{expression}` of something `where {}` does not answer — it \
                         answers for `{}`, {}, and no other",
                        rule.rule,
                        landed.said,
                        landed.name,
                        landed.id.as_str()
                    )
                };
                if !said.contains(&remark) {
                    said.push(remark);
                }
            }
        }
        said
    }

    /// Why a `stipulate` about a call came too late for the act it is about.
    ///
    /// Said beside the stipulation, which still lands — a later act may ask —
    /// when one of its arguments is an instance this step's act made and
    /// something the act set off asked this very call and was left unsettled.
    /// That reaction asked before the answer existed, and the step reads
    /// undecided for a reason the journey's own next line looks like it
    /// answers. Something else left unsettled is not this line's doing, and
    /// pointing at it would send the reader to a fix that changes nothing.
    pub(crate) fn too_late(&self, call: &str, arguments: &[Term]) -> Option<String> {
        if !self.undecided.iter().chain(&self.unapplied).any(|rule| asked_of(rule, call).is_some())
        {
            return None;
        }
        let made = arguments.iter().find_map(|term| match term {
            Term::Path(path) => self
                .bound
                .get(&path.root)
                .filter(|id| self.created.contains(id))
                .map(|_| path.root.as_str()),
            _ => None,
        })?;
        let caught = self
            .cast
            .iter()
            .rev()
            .find(|member| member.name == made)
            .map_or_else(|| made.to_owned(), |member| format!("{made}: {}", member.type_expr));
        let inside: Vec<String> = arguments.iter().map(Term::as_written).collect();
        Some(format!(
            "said after the act that made `{made}`, so nothing the act set off could read it — \
             to answer `{call}({})` for what it set off, write `where` under `creating {caught}`",
            inside.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journey::Path;

    fn caught() -> Cast {
        Cast { name: "issue".to_owned(), type_expr: "Issue".to_owned(), line: 1 }
    }

    fn named(root: &str) -> Term {
        Term::Path(Path { root: root.to_owned(), segments: Vec::new() })
    }

    fn audience() -> Where {
        Where {
            call: "audience".to_owned(),
            arguments: vec![named("issue")],
            value: Term::Set(vec![]),
        }
    }

    #[test]
    fn nothing_is_awaited_without_a_catch_or_an_answer() {
        assert!(Awaiting::of(None, &[audience()]).is_none());
        assert!(Awaiting::of(Some(&caught()), &[]).is_none());
        let awaiting = Awaiting::of(Some(&caught()), &[audience()]).expect("both");
        assert_eq!(awaiting.caught, caught());
        assert_eq!(awaiting.answering, [audience()]);
    }

    #[test]
    fn a_term_mentions_a_name_it_reads_and_no_other() {
        assert!(named("issue").mentions("issue"));
        let through =
            Term::Path(Path { root: "issue".to_owned(), segments: vec!["repo".to_owned()] });
        assert!(through.mentions("issue"));
        assert!(Term::Set(vec![named("repo"), named("issue")]).mentions("issue"));
        assert!(!named("repo").mentions("issue"));
        assert!(!Term::Set(vec![named("repo")]).mentions("issue"));
        assert!(!Term::Literal(inspect_sim::Value::Bool(true)).mentions("issue"));
        assert!(!Term::Clock { offset: 0, written: "now".to_owned() }.mentions("issue"));
    }

    #[test]
    fn a_where_is_written_as_the_call_it_answers() {
        let both = Where {
            call: "muted".to_owned(),
            arguments: vec![named("issue"), named("repo")],
            value: Term::Literal(inspect_sim::Value::Bool(false)),
        };
        assert_eq!(both.as_written(), "muted(issue, repo)");
        assert_eq!(audience().as_written(), "audience(issue)");
    }
}
