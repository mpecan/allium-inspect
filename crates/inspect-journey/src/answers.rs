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

mod awaiting;
mod whose;

pub(crate) use awaiting::Awaiting;

use inspect_sim::{Value, step::RuleOutcome, value::EntityId, world::Answer};
use whose::{asked_of, no_module, reaches, shadowed};

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
    /// `audience(issue)`: the call alone, as the journey wrote it.
    written: String,
    /// The name `creating` caught, and the instance it named when it landed.
    name: String,
    id: EntityId,
    /// The engine turn it landed in: a rule in that turn asked before it.
    turn: usize,
    /// The engine turn its own line's instance was made in — earlier than
    /// `turn` when the answer read another line's name and waited for it.
    made: usize,
    /// The line the answer reads that was caught in `turn`: the one it
    /// waited for when `made` is earlier, and meaningless when it is not.
    waited: Option<String>,
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
    Undecided {
        rule: rule.name.clone(),
        module: rule.module.clone(),
        why,
        asked: asked(rule),
        reasons: rule
            .unresolved
            .iter()
            .map(|note| (note.expression.clone(), note.reason.clone()))
            .collect(),
        turn,
    }
}

impl Walker<'_> {
    /// One line's `where` answer, now that `id` is the instance it caught —
    /// made in engine turn `made`, and `waited` for another line when that
    /// line's instance came later.
    fn answer_for(
        &mut self,
        caught: &Cast,
        id: &EntityId,
        answer: Where,
        (made, waited): (usize, Option<String>),
    ) {
        if let Some(why) = self.no_module(&answer.call) {
            self.unheard.push(format!(
                "{why}, so `where {}` answers no call the specification can ask",
                answer.as_written()
            ));
            return;
        }
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
            return;
        }
        // In the journey's words, as `given` and `stipulate` list a call:
        // `{repo}`, not the world's `{Repo#1}`.
        let said = format!("{} = {}", answer.as_written(), answer.value.as_written());
        if let Some(why) = shadowed(&self.world, &answer.call, &arguments) {
            self.remarks.push(format!("`where {said}`: {why}"));
        }
        self.world.tell(Answer { call: answer.call.clone(), arguments, value });
        self.landed.push(Landed {
            call: answer.call.clone(),
            said: said.clone(),
            written: answer.as_written(),
            name: caught.name.clone(),
            id: id.clone(),
            turn: self.firings,
            made,
            waited,
        });
        self.stipulated.push(Stipulation { said, through: self.standing_on.clone() });
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
        if let Some(why) = self.no_module(&answer.call) {
            self.notes.push(Outcome {
                line,
                verdict: Verdict::Undecided,
                about: format!("given {}", answer.as_written()),
                detail: Some(format!("{why}, so this answers no call the specification can ask")),
            });
            if !listed {
                self.refused_in_the_world.push(line);
            }
            return;
        }
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
        // Said of the journey's own lines, as `stipulate` says it: a file's
        // world is not this journey's to be told about.
        if listed && let Some(why) = shadowed(&self.world, &answer.call, &arguments) {
            self.notes.push(Outcome {
                line,
                verdict: Verdict::Remark,
                about: format!("given {}", answer.as_written()),
                detail: Some(why),
            });
        }
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
                if let Some(why) = self.no_module(name) {
                    return Outcome {
                        line,
                        verdict: Verdict::Undecided,
                        about,
                        detail: Some(format!(
                            "{why}, so this answers no call the specification can ask"
                        )),
                    };
                }
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
                let detail = [self.too_late(name, terms), shadowed(&self.world, name, &arguments)]
                    .into_iter()
                    .flatten()
                    .reduce(|first, then| format!("{first}; {then}"));
                self.world.tell(Answer { call: name.clone(), arguments, value });
                // In the journey's words, as every answer to a call is
                // listed, wherever it is said.
                let said = format!("{} = {}", subject.as_written(), stipulated.as_written());
                self.stipulated.push(Stipulation { said, through: self.standing_on.clone() });
                Outcome { line, verdict: Verdict::Specified, about, detail }
            }
        }
    }

    /// See [`whose::no_module`].
    fn no_module(&self, call: &str) -> Option<String> {
        no_module(self.spec, call)
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
                let remark = if !reaches(&landed.call, rule, expression) {
                    // Another module's call of the same name: the qualifier
                    // said this answer was not about it, and the reader who
                    // sees it unsettled should be told that is why.
                    whose::elsewhere(rule, expression, &landed.written, &landed.call)
                } else if rule.turn <= landed.turn
                    && rule.turn > landed.made
                    && let Some(sibling) = &landed.waited
                {
                    // Held back for another line's instance, and asked in
                    // the meantime: what it set off asked, not the maker.
                    format!(
                        "`{}` asked `{expression}` before `where {}` was in force — the answer \
                         reads `{sibling}`, so it waited until `{sibling}` was made, and `{}` \
                         asked no later than that",
                        rule.rule, landed.said, rule.rule
                    )
                } else if rule.turn <= landed.made {
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
        if !self
            .undecided
            .iter()
            .chain(&self.unapplied)
            .any(|rule| asked_of(rule, call).is_some_and(|asked| reaches(call, rule, asked)))
        {
            return None;
        }
        let made = arguments.iter().find_map(|term| match term {
            Term::Path(path) => self
                .bound
                .get(&path.root)
                .filter(|id| self.created.iter().any(|(made, _)| made == *id))
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
    use super::{awaiting::catch, *};
    use crate::journey::Caught;
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

    /// Made in `home`, as the walk records what an act made.
    fn made_in(home: &str, made: &[EntityId]) -> Vec<(EntityId, String)> {
        made.iter().map(|id| (id.clone(), home.to_owned())).collect()
    }

    #[test]
    fn each_line_catches_the_first_of_its_kind_no_line_above_caught() {
        let made = made_in(
            "pulls",
            &[EntityId::new("Review", 1), EntityId::new("Comment", 1), EntityId::new("Comment", 2)],
        );
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

    /// A line naming a module catches only what that module declares: of two
    /// modules' `Order`s, `warehouse/Order` is the warehouse's whichever was
    /// made first, and a bare `Order` takes whatever is left.
    #[test]
    fn a_qualified_line_catches_only_its_own_module_s_instance() {
        let made = [
            (EntityId::new("Order", 1), "shop".to_owned()),
            (EntityId::new("Order", 2), "warehouse".to_owned()),
        ];
        let w = caught_as("w", "warehouse/Order");
        let s = caught_as("s", "shop/Order");
        assert_eq!(
            catch(&made, &[&w, &s]),
            [Some(EntityId::new("Order", 2)), Some(EntityId::new("Order", 1))]
        );
        let tally = caught_as("t", "tally/Order");
        let bare = caught_as("o", "Order");
        assert_eq!(catch(&made, &[&tally, &bare]), [None, Some(EntityId::new("Order", 1))]);
    }

    /// In the order made, not the order ids sort in as text.
    #[test]
    fn the_first_made_is_caught_before_one_that_sorts_first() {
        let made = made_in("shop", &[EntityId::new("Order", 9), EntityId::new("Order", 10)]);
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
        assert_eq!(catch(&made_in("repos", &[EntityId::new("Repo", 1)]), &[&caught()]), [None]);
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
