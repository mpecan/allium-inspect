//! Deciding whether a `then` or a `sees` line holds.
//!
//! Split from the walk itself because the two ask different questions. The walk
//! asks what happens next; this asks what is so afterwards, and it never
//! changes the world — every method here takes `&self`.

use inspect_model::NodeKind;
use inspect_sim::{Truth, Value};

use crate::{
    check::Verdict,
    journey::{Assertion, Comparison, Path},
    run::{Outcome, Walker},
};

impl Walker<'_> {
    /// Evaluate one assertion against the world the last step left.
    pub(crate) fn assert(&self, assertion: &Assertion, line: usize, about: String) -> Outcome {
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
    pub(crate) fn observe(
        &self,
        path: &Path,
        negated: bool,
        line: usize,
        about: String,
    ) -> Outcome {
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
