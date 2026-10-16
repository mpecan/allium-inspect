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
    journey::{Cast, Caught, Subject, Term, Where},
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

/// The answers an act in progress will give once the instances they are
/// about exist.
#[derive(Debug, Clone)]
pub(crate) struct Awaiting {
    /// Every line the act catches, in order — those with nothing left to
    /// answer too, because which instance a line catches depends on the lines
    /// of its kind above it. An answer is taken out of its line once it lands.
    lines: Vec<Caught>,
}

impl Awaiting {
    /// `None` when there is nothing to wait for: no `creating`, or no `where`
    /// under any of them.
    pub(crate) fn of(creating: &[Caught]) -> Option<Self> {
        Self { lines: creating.to_vec() }.waiting()
    }

    /// Itself while some line still has an answer to give, and `None` once
    /// every answer has landed.
    fn waiting(self) -> Option<Self> {
        self.lines.iter().any(|line| !line.answering.is_empty()).then_some(self)
    }
}

/// Which instance each `creating` line catches, of those `made` in the order
/// they were made: the first of its kind no line above it caught.
///
/// The first made, not the first in the world's order: ids sort as text, so
/// `Order#10` comes before `Order#9`, and a reaction that made another order
/// would be caught instead of the act's own whenever the count crossed a
/// digit. And not merely the first of its kind: two lines of one kind would
/// otherwise both name the first instance, and the second line would read as
/// caught while the second instance went unnamed.
///
/// By the entity's name, as a single line always caught: `pulls/Comment` and
/// `Comment` name the same thing.
pub(crate) fn catch(made: &[EntityId], lines: &[&Cast]) -> Vec<Option<EntityId>> {
    let mut claimed: Vec<&EntityId> = Vec::new();
    let mut caught = Vec::new();
    for line in lines {
        let bare = line.type_expr.rsplit('/').next().unwrap_or(&line.type_expr);
        let id = made.iter().find(|id| id.entity() == bare && !claimed.contains(id));
        if let Some(id) = id {
            claimed.push(id);
        }
        caught.push(id.cloned());
    }
    caught
}

/// The first name `answer` reads that a line of the same act names and has
/// not caught an instance for, with what that leaves the answer.
fn uncaught<'a>(
    answer: &Where,
    (names, made): (&'a [String], &[Option<EntityId>]),
) -> Option<(&'a str, &'static str)> {
    names.iter().zip(made).filter(|(_, id)| id.is_none()).find_map(|(name, _)| {
        if answer.arguments.iter().any(|term| term.mentions(name)) {
            Some((name.as_str(), "answers no call the specification can ask"))
        } else {
            answer.value.mentions(name).then_some((name.as_str(), "gives the call no answer"))
        }
    })
}

impl Walker<'_> {
    /// The instance each of `lines` catches, of those the act has made so far
    /// — `None` for a line whose instance was removed since. See [`catch`].
    ///
    /// Assigned over every instance the act made, removed ones included, so
    /// a line keeps the instance it caught: dropping a removed one first moved
    /// every later line of its kind up by one, and the line above took the
    /// instance its sibling's answer had already landed on.
    pub(crate) fn made_for(&self, lines: &[Caught]) -> Vec<Option<EntityId>> {
        let casts: Vec<&Cast> = lines.iter().map(|line| &line.cast).collect();
        catch(&self.created, &casts)
            .into_iter()
            .map(|id| id.filter(|id| self.world.instance(id).is_some()))
            .collect()
    }

    /// Put the act's `where` answers in force, each once the instance its line
    /// caught has been made and so has every other line's it reads. Called
    /// after every turn of the engine during an act; once an answer lands it
    /// is not waited for again.
    ///
    /// An answer reading another line's name waits for that line, rather than
    /// being weighed against whatever the name meant before the act — an
    /// instance an earlier step caught, or no instance at all, which reads as
    /// a state of the same spelling. Either is a definite answer about
    /// something the journey never meant.
    pub(crate) fn answer_what_was_made(&mut self) {
        let Some(mut awaiting) = self.awaiting.take() else {
            return;
        };
        let made = self.made_for(&awaiting.lines);
        let names: Vec<String> = awaiting.lines.iter().map(|line| line.cast.name.clone()).collect();
        // Every name the act has made something for is bound before any
        // answer is weighed, so an answer under one line may read what another
        // line caught — whichever of them is written first. Bound now rather
        // than once the act is over, so the answer's arguments read as the
        // instance; the cast entry is still made by the act, once, when it
        // catches the same instance.
        for (name, id) in names.iter().zip(&made) {
            if let Some(id) = id {
                self.bound.insert(name.clone(), id.clone());
            }
        }
        for (line, id) in awaiting.lines.iter_mut().zip(&made) {
            let Some(id) = id else {
                continue;
            };
            let (ready, waiting): (Vec<Where>, Vec<Where>) = std::mem::take(&mut line.answering)
                .into_iter()
                .partition(|answer| uncaught(answer, (&names, &made)).is_none());
            line.answering = waiting;
            self.answer_for(&line.cast, id, ready);
        }
        self.awaiting = awaiting.waiting();
    }

    /// Refuse what is still waiting once the act is over: an answer whose own
    /// line caught its instance, reading a line that never caught one. Said
    /// on the act's line, as an answer with no value is.
    pub(crate) fn refuse_what_still_waits(&mut self) {
        let Some(awaiting) = self.awaiting.take() else {
            return;
        };
        let made = self.made_for(&awaiting.lines);
        let names: Vec<String> = awaiting.lines.iter().map(|line| line.cast.name.clone()).collect();
        for (line, id) in awaiting.lines.iter().zip(&made) {
            // A line that caught nothing is said about by the act, by name.
            if id.is_none() {
                continue;
            }
            for answer in &line.answering {
                if let Some((sibling, consequence)) = uncaught(answer, (&names, &made)) {
                    self.unheard.push(format!(
                        "`{sibling}` has no value once `{}` is made, so `where {}` {consequence}",
                        line.cast.name,
                        answer.as_written()
                    ));
                }
            }
        }
    }

    /// One line's `where` answers, now that `id` is the instance it caught.
    fn answer_for(&mut self, caught: &Cast, id: &EntityId, answering: Vec<Where>) {
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

    fn line(name: &str, type_expr: &str, answering: Vec<Where>) -> Caught {
        Caught {
            cast: Cast { name: name.to_owned(), type_expr: type_expr.to_owned(), line: 1 },
            answering,
        }
    }

    #[test]
    fn nothing_is_awaited_without_a_catch_or_an_answer() {
        assert!(Awaiting::of(&[]).is_none());
        assert!(Awaiting::of(&[line("issue", "Issue", vec![])]).is_none());
        let both = [line("issue", "Issue", vec![audience()])];
        let awaiting = Awaiting::of(&both).expect("both");
        assert_eq!(awaiting.lines, both);
    }

    /// One line answering is enough to wait, and every line is kept: which
    /// instance the answered one catches depends on the lines above it.
    #[test]
    fn every_line_is_kept_while_any_of_them_answers() {
        let lines = [line("review", "Review", vec![]), line("issue", "Issue", vec![audience()])];
        let awaiting = Awaiting::of(&lines).expect("the second line answers");
        assert_eq!(awaiting.lines, lines);
        // Once its answer has landed there is nothing left to wait for.
        let mut landed = awaiting;
        landed.lines[1].answering.clear();
        assert!(landed.waiting().is_none());
    }

    #[test]
    fn each_line_catches_the_first_of_its_kind_no_line_above_caught() {
        let made =
            [EntityId::new("Review", 1), EntityId::new("Comment", 1), EntityId::new("Comment", 2)];
        let first = caught_as("first", "pulls/Comment");
        let review = caught_as("review", "Review");
        let second = caught_as("second", "Comment");
        let third = caught_as("third", "Comment");
        assert_eq!(
            catch(&made, &[&first, &review, &second, &third]),
            [
                Some(EntityId::new("Comment", 1)),
                Some(EntityId::new("Review", 1)),
                Some(EntityId::new("Comment", 2)),
                None,
            ]
        );
    }

    /// In the order made, not the order ids sort in as text.
    #[test]
    fn the_first_made_is_caught_before_one_that_sorts_first() {
        let made = [EntityId::new("Order", 9), EntityId::new("Order", 10)];
        let one = caught_as("one", "Order");
        let two = caught_as("two", "Order");
        assert_eq!(
            catch(&made, &[&one, &two]),
            [Some(EntityId::new("Order", 9)), Some(EntityId::new("Order", 10))]
        );
    }

    #[test]
    fn nothing_of_a_kind_made_is_nothing_caught() {
        assert_eq!(catch(&[], &[&caught()]), [None]);
        assert_eq!(catch(&[EntityId::new("Repo", 1)], &[&caught()]), [None]);
    }

    fn caught_as(name: &str, type_expr: &str) -> Cast {
        Cast { name: name.to_owned(), type_expr: type_expr.to_owned(), line: 1 }
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
