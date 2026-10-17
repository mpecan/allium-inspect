//! An act's `creating` lines while the act runs (issue #31): which instance
//! each line catches, and each line's `where` answers waiting until what they
//! read has been made.

use inspect_sim::value::EntityId;

use crate::{
    journey::{Cast, Caught, Where},
    run::Walker,
};

/// The answers an act in progress will give once the instances they are
/// about exist.
#[derive(Debug, Clone)]
pub(crate) struct Awaiting {
    /// Every line the act catches, in order — those with nothing left to
    /// answer too, because which instance a line catches depends on the lines
    /// of its kind above it. An answer is taken out of its line once it lands.
    pub(super) lines: Vec<Caught>,
    /// The engine turn each line first caught its instance in.
    pub(super) seen: Vec<Option<usize>>,
}

impl Awaiting {
    /// `None` when there is nothing to wait for: no `creating`, or no `where`
    /// under any of them.
    pub(crate) fn of(creating: &[Caught]) -> Option<Self> {
        Self { lines: creating.to_vec(), seen: vec![None; creating.len()] }.waiting()
    }

    /// Itself while some line still has an answer to give, and `None` once
    /// every answer has landed.
    pub(super) fn waiting(self) -> Option<Self> {
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
/// By the entity's name, and by the module that declares it when the line
/// names one: `tally/Tag` catches `tally`'s tag and never `parcels`', as
/// `tally/watchers` answers only `tally`'s call. Two modules may each declare
/// a `Tag`, and a line qualified by one catching the other's would bind the
/// name by order of creation, not by what the journey wrote. A bare `Tag`
/// catches either.
pub(crate) fn catch(made: &[(EntityId, String)], lines: &[&Cast]) -> Vec<Option<EntityId>> {
    let mut claimed: Vec<&EntityId> = Vec::new();
    let mut caught = Vec::new();
    for line in lines {
        let (module, bare) = inspect_sim::eval::split_module(&line.type_expr);
        let id = made
            .iter()
            .find(|(id, home)| {
                id.entity() == bare
                    && module.is_none_or(|module| module == home)
                    && !claimed.contains(&id)
            })
            .map(|(id, _)| id);
        if let Some(id) = id {
            claimed.push(id);
        }
        caught.push(id.cloned());
    }
    caught
}

/// Whether `answer` reads `name`, in an argument or in what it answers.
fn reads(answer: &Where, name: &str) -> bool {
    answer.arguments.iter().any(|term| term.mentions(name)) || answer.value.mentions(name)
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
        self.caught_by(lines)
            .into_iter()
            .map(|id| id.filter(|id| self.world.instance(id).is_some()))
            .collect()
    }

    /// The instance each of `lines` caught, removed since or not.
    pub(crate) fn caught_by(&self, lines: &[Caught]) -> Vec<Option<EntityId>> {
        let casts: Vec<&Cast> = lines.iter().map(|line| &line.cast).collect();
        catch(&self.created, &casts)
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
        for (seen, id) in awaiting.seen.iter_mut().zip(&made) {
            if seen.is_none() && id.is_some() {
                *seen = Some(self.firings);
            }
        }
        let seen = awaiting.seen.clone();
        for ((line, id), at) in awaiting.lines.iter_mut().zip(&made).zip(&seen) {
            let (Some(id), Some(at)) = (id, at) else {
                continue;
            };
            let (ready, waiting): (Vec<Where>, Vec<Where>) = std::mem::take(&mut line.answering)
                .into_iter()
                .partition(|answer| uncaught(answer, (&names, &made)).is_none());
            line.answering = waiting;
            for answer in ready {
                // Waited, when its own instance was made in an earlier turn:
                // for the line it reads that was caught last.
                let waited = (*at < self.firings)
                    .then(|| {
                        names.iter().zip(&seen).find_map(|(name, turn)| {
                            (*turn == Some(self.firings) && reads(&answer, name))
                                .then(|| name.clone())
                        })
                    })
                    .flatten();
                self.answer_for(&line.cast, id, answer, (*at, waited));
            }
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
}
