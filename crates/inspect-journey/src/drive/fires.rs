//! `then <Rule> fires`, and `does not fire`: what the step ran, refused,
//! could not decide — and, for a rule nobody fires, what the clock's listings
//! said about it.

use inspect_model::NodeKind;
use inspect_sim::Truth;

use crate::run::Walker;

impl Walker<'_> {
    /// `then <Rule> fires`, or `does not fire`, against what the last step ran.
    pub(crate) fn fires(&self, rule: &str, negated: bool) -> (Truth, Option<String>) {
        let ran = self.fired.iter().any(|name| name == rule);
        // "did not run" and "could not be told whether it should" are
        // different answers, and only one of them is about the spec.
        if !ran && let Some(it) = self.undecided.iter().find(|it| it.rule == rule) {
            return (Truth::Unknown, Some(format!("`{rule}` could not be decided: {}", it.why)));
        }
        // Woken and refused: the spec's own reason, in its own words, rather
        // than a bare "did not run" a reader has to go and explain.
        //
        // Unless the step is unsettled. Refused on the firings that happened
        // says nothing about one that did not: a `for` member nobody could
        // decide about, or a cascade cut off, may have woken it again with
        // something it would have taken.
        if !ran && let Some(it) = self.refused.iter().find(|it| it.rule == rule) {
            let refused = format!("`{rule}` refused: {}", it.why);
            return match self.unsettled() {
                Some(why) => (
                    Truth::Unknown,
                    Some(format!(
                        "{refused} — but whether it would have run on what was not settled is \
                         not known: {why}"
                    )),
                ),
                None => (Truth::from_bool(negated), Some(refused)),
            };
        }
        // Unless the clock settled the world this step and no round of it
        // left the rule undecided: then every listing that asked said *no*,
        // and the answer is the spec's — a task with no `stopped_at` does
        // not expire (issue #20). Only for a rule a listing reads at all: one
        // whose `when` binds nothing — `when: Task.due_at <= now` — is never
        // listed, and silence about it is not a *no*.
        let decided = self.listed.as_ref().is_some_and(|unsure| !unsure.contains(rule))
            && self.listing_reads(rule);
        if !ran && !decided && self.waits_on_the_world(rule) {
            // Nobody fires a state-condition rule; it becomes true or it does
            // not. Outside a settled clock nothing listed it, so a rule whose
            // condition is *false* and one whose condition could not be
            // *decided* look identical from here — and reporting the second as
            // a flat no is the failure this whole design refuses.
            return (
                Truth::Unknown,
                Some(format!(
                    "`{rule}` never became true, and whether its condition is false or could \
                     not be decided is not visible from here"
                )),
            );
        }
        // Not run, while something this step ran left part of what it ensures
        // unapplied or could not be decided at all. Whatever it did not emit
        // or write is exactly what might have woken this rule, so "did not
        // run" is a fact about this world and not about the spec — and `does
        // not fire` holding on it would be the simulator guessing.
        if !ran && let Some(why) = self.unsettled() {
            return (
                Truth::Unknown,
                Some(format!(
                    "`{rule}` did not run, but whether it would have is not settled — {why}"
                )),
            );
        }
        // Said either way. A refused `does not fire` with nothing under it
        // reads, in a report, like a line that held.
        (
            Truth::from_bool(ran != negated),
            Some(if ran { format!("`{rule}` ran") } else { format!("`{rule}` did not run") }),
        )
    }

    /// Whether the clock's listing reads `rule` at all, and so whether its
    /// silence about it says anything.
    fn listing_reads(&self, rule: &str) -> bool {
        self.spec
            .nodes_of(NodeKind::Rule)
            .find(|node| node.name == rule)
            .is_some_and(|node| inspect_sim::step::lists(self.program, node))
    }

    /// Whether a rule waits on the world rather than on somebody acting.
    ///
    /// Not a creation rule, although the model files one as a state rule: a
    /// creation is followed whenever a step makes one, so one that did not run
    /// was either never woken — nothing of its kind was made — or refused, and
    /// both of those are visible from here.
    fn waits_on_the_world(&self, rule: &str) -> bool {
        use inspect_model::graph::TriggerSource;
        self.spec.nodes_of(NodeKind::Rule).find(|node| node.name == rule).is_some_and(|node| {
            let creation = self
                .program
                .rule(node.id.as_str())
                .is_some_and(|ast| inspect_sim::creation::creation_of(ast.when.as_ref()).is_some());
            !creation
                && node.detail.as_rule().is_some_and(|detail| {
                    matches!(detail.source, TriggerSource::State | TriggerSource::Temporal)
                })
        })
    }
}
