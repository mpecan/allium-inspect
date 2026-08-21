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
                let value = self.read(path);
                // An unknown is not an absence, and this is the one arm where
                // reading it as one goes wrong in *both* directions at once: a
                // path that ran out would make `does not exist` hold on a world
                // nothing described, and `exists` refuse — the spec saying no to
                // a question nobody put to it. Stipulation 1 names those two
                // failure modes; here they are the same line.
                //
                // An unbound bare name is a different thing and stays decidable:
                // `read` answers it as a state name rather than as unknown, so a
                // journey that never caught a reservation can still say so.
                if value.is_unknown() {
                    (Truth::Unknown, Some(format!("{} is unknown", path.as_written())))
                } else {
                    let found =
                        matches!(&value, Value::Ref(id) if self.world.instance(id).is_some());
                    (Truth::from_bool(found != *negated), None)
                }
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
    /// Two questions, and only the first is answerable today. Whether the
    /// boundary carries the field at all is a fact about the surface, and it
    /// decides a `cannot see` outright: not "no instance matched" but "this
    /// boundary does not carry it", which is the strongest form of the claim.
    ///
    /// Whether the surface's own filter admits *this* actor needs the
    /// `exposes` clause as an expression rather than as text, and that is not
    /// read yet. So once the field *is* carried, neither direction can be
    /// settled — including the negative one. That last part is the whole
    /// reason this reads the surface itself rather than the value: a field
    /// nothing has set used to make `cannot see` come back satisfied, so
    /// `ada cannot see ada.open_loan_count on MemberShelf` held against a
    /// surface that exposes it on the line above. A privacy claim that passes
    /// because nothing checked it is the worst answer this tool could give.
    pub(crate) fn observe(&self, sight: &Sight<'_>, about: String) -> Outcome {
        let Sight { path, surface, negated, line } = *sight;
        let written = path.as_written();
        let carried = crate::check::surface_named(self.spec, surface)
            .is_some_and(|detail| crate::check::exposes(detail, &written));
        if !carried {
            return Outcome {
                line,
                verdict: if negated { Verdict::Specified } else { Verdict::Unexposed },
                about,
                detail: Some(format!("`{surface}` exposes nothing like `{written}`")),
            };
        }
        Outcome {
            line,
            verdict: Verdict::Undecided,
            about,
            detail: Some(format!(
                "`{surface}` exposes `{written}` — whether its filter admits this actor is not \
                 read yet"
            )),
        }
    }
}

/// One `sees` or `cannot see` line, as the walker asks it.
///
/// Grouped rather than passed loose because the four travel together and the
/// checker already asks the same question under the same shape.
pub(crate) struct Sight<'a> {
    pub(crate) path: &'a Path,
    pub(crate) surface: &'a str,
    pub(crate) negated: bool,
    pub(crate) line: usize,
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
