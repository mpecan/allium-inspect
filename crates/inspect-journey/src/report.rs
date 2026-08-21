//! Saying what became of a journey, to a person or to an agent.
//!
//! A journey is the demand written first, so the report is a ledger of what the
//! specification still owes rather than a pass or a fail. What a reader is
//! looking for, in order: what this journey was told rather than shown, which
//! steps hold, and what the rest are waiting on.

use crate::{check::Verdict, run::Walk};

/// How strictly a run should be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strictness {
    /// Every step gets a status and the run succeeds. The mode a journey is
    /// *written* in: you write the walk, and the missing steps are the backlog.
    #[default]
    Report,
    /// A step the spec cannot support is a failure. The mode a finished journey
    /// is defended in, and the one a build gate would use — on the journeys
    /// somebody has decided are done, rather than on all of them.
    Strict,
}

impl Verdict {
    /// The word a report prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Specified => "holds",
            Verdict::Undecided => "undecided",
            Verdict::Refused => "refused",
            Verdict::Unspecified => "unspecified",
            Verdict::Unexposed => "unexposed",
            Verdict::Remark => "remark",
        }
    }

    /// Whether `Strictness::Strict` should fail on this.
    ///
    /// Undecided is not a failure in either mode. A real spec cannot decide
    /// derived values, and a gate that failed on those would fail on every
    /// journey ever written.
    #[must_use]
    pub fn is_failure(self) -> bool {
        matches!(self, Verdict::Refused | Verdict::Unspecified | Verdict::Unexposed)
    }
}

/// Whether a set of walks passes at this strictness.
#[must_use]
pub fn passes(walks: &[Walk], strictness: Strictness) -> bool {
    match strictness {
        Strictness::Report => true,
        Strictness::Strict => !walks.iter().any(|walk| walk.verdict().is_failure()),
    }
}

/// The walks, for a person.
#[must_use]
pub fn render(walks: &[Walk]) -> String {
    let mut out = String::new();
    for walk in walks {
        let holds = walk.steps.iter().filter(|step| step.verdict() == Verdict::Specified).count();
        out.push_str(&format!("{}  —  {} of {} steps hold\n", walk.name, holds, walk.steps.len()));

        // What it was told rather than shown, first and always. An agent can
        // make any journey pass; it cannot make one pass invisibly.
        for stipulation in &walk.stipulated {
            out.push_str(&format!("    stipulated  {stipulation}\n"));
        }

        for step in &walk.steps {
            out.push_str(&format!(
                "  {:>2}. {:<48} {}\n",
                step.number,
                truncate(&step.title, 48),
                step.verdict().as_str()
            ));
            for outcome in &step.outcomes {
                if outcome.verdict == Verdict::Specified {
                    continue;
                }
                out.push_str(&format!("        {}\n", outcome.about));
                if let Some(detail) = &outcome.detail {
                    out.push_str(&format!("          {detail}\n"));
                }
            }
        }
        out.push('\n');
    }
    out
}

fn truncate(text: &str, at: usize) -> String {
    if text.chars().count() <= at {
        return text.to_owned();
    }
    let kept: String = text.chars().take(at.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// The walks, for whatever wrote them.
#[must_use]
pub fn as_json(walks: &[Walk]) -> serde_json::Value {
    serde_json::Value::Array(
        walks
            .iter()
            .map(|walk| {
                serde_json::json!({
                    "journey": walk.name,
                    "verdict": walk.verdict().as_str(),
                    "stipulated": walk.stipulated,
                    "steps": walk.steps.iter().map(|step| serde_json::json!({
                        "number": step.number,
                        "title": step.title,
                        "verdict": step.verdict().as_str(),
                        "outcomes": step.outcomes.iter().map(|outcome| serde_json::json!({
                            "line": outcome.line,
                            "verdict": outcome.verdict.as_str(),
                            "about": outcome.about,
                            "detail": outcome.detail,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{Outcome, Walked};

    fn outcome(verdict: Verdict, about: &str, detail: Option<&str>) -> Outcome {
        Outcome { line: 7, verdict, about: about.to_owned(), detail: detail.map(ToOwned::to_owned) }
    }

    fn walk(name: &str, steps: Vec<Walked>, stipulated: Vec<String>) -> Walk {
        Walk {
            name: name.to_owned(),
            goal: vec!["she does the thing".to_owned()],
            ends: vec!["the thing is done".to_owned()],
            line: 1,
            steps,
            stipulated,
        }
    }

    fn step(number: u32, title: &str, outcomes: Vec<Outcome>) -> Walked {
        Walked { number, title: title.to_owned(), outcomes }
    }

    fn borrowing(verdict: Verdict) -> Walk {
        walk(
            "ACopyGoesOut",
            vec![step(1, "she borrows it", vec![outcome(verdict, "ada does X on S", Some("why"))])],
            Vec::new(),
        )
    }

    #[test]
    fn every_verdict_has_a_word_of_its_own() {
        // A report where two of them print the same is a report that cannot be
        // read, and `unspecified` and `refused` are the two a reader most needs
        // to tell apart: one is work to do, the other is a disagreement.
        let words: Vec<&str> = [
            Verdict::Specified,
            Verdict::Undecided,
            Verdict::Refused,
            Verdict::Unspecified,
            Verdict::Unexposed,
            Verdict::Remark,
        ]
        .into_iter()
        .map(Verdict::as_str)
        .collect();
        let mut unique = words.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), words.len(), "{words:?}");
        assert!(words.iter().all(|word| !word.is_empty()));
        assert_eq!(Verdict::Specified.as_str(), "holds");
        assert_eq!(Verdict::Unspecified.as_str(), "unspecified");
    }

    #[test]
    fn undecided_is_not_a_failure_in_either_mode() {
        // A real spec cannot decide its derived values, so a gate that failed
        // on those would fail on every journey ever written — and the writer
        // would turn the gate off, which is worse than not having one.
        assert!(!Verdict::Undecided.is_failure());
        assert!(!Verdict::Specified.is_failure());
        assert!(!Verdict::Remark.is_failure());
    }

    #[test]
    fn what_the_spec_cannot_support_is_a_failure() {
        assert!(Verdict::Refused.is_failure());
        assert!(Verdict::Unspecified.is_failure());
        assert!(Verdict::Unexposed.is_failure());
    }

    #[test]
    fn report_mode_passes_whatever_happened() {
        // The mode a journey is *written* in: you write the walk, and the steps
        // the spec cannot support are the backlog rather than an error.
        for verdict in [Verdict::Refused, Verdict::Unspecified, Verdict::Unexposed] {
            assert!(passes(&[borrowing(verdict)], Strictness::Report), "{verdict:?}");
        }
    }

    #[test]
    fn strict_mode_fails_on_what_the_spec_cannot_support() {
        assert!(!passes(&[borrowing(Verdict::Unspecified)], Strictness::Strict));
        assert!(!passes(&[borrowing(Verdict::Refused)], Strictness::Strict));
        assert!(!passes(&[borrowing(Verdict::Unexposed)], Strictness::Strict));
    }

    #[test]
    fn strict_mode_passes_a_journey_that_only_could_not_be_decided() {
        assert!(passes(&[borrowing(Verdict::Undecided)], Strictness::Strict));
        assert!(passes(&[borrowing(Verdict::Specified)], Strictness::Strict));
    }

    #[test]
    fn nothing_to_walk_passes_either_way() {
        assert!(passes(&[], Strictness::Strict));
        assert!(passes(&[], Strictness::Report));
    }

    #[test]
    fn a_report_counts_the_steps_that_hold() {
        // Deliberately lopsided. Two of four counted either way round reads as
        // correct whichever verdict is being counted, and the number a person
        // acts on is how much of their journey the spec already supports.
        let result = walk(
            "J",
            vec![
                step(1, "one", vec![outcome(Verdict::Specified, "a", None)]),
                step(2, "two", vec![outcome(Verdict::Specified, "b", None)]),
                step(3, "three", vec![outcome(Verdict::Refused, "c", Some("no"))]),
            ],
            Vec::new(),
        );
        let text = render(&[result]);
        assert!(text.contains("2 of 3 steps hold"), "{text}");
    }

    #[test]
    fn a_report_shows_only_the_lines_that_did_not_hold() {
        // A journey of forty steps that works is one line. What a reader came
        // for is the four that did not.
        let result = walk(
            "J",
            vec![step(
                1,
                "one",
                vec![
                    outcome(Verdict::Specified, "the part that worked", None),
                    outcome(Verdict::Refused, "the part that did not", Some("because")),
                ],
            )],
            Vec::new(),
        );
        let text = render(&[result]);
        assert!(!text.contains("the part that worked"), "{text}");
        assert!(text.contains("the part that did not"), "{text}");
        assert!(text.contains("because"), "{text}");
    }

    #[test]
    fn a_report_leads_with_what_it_was_told_rather_than_shown() {
        // The guardrail. An agent can make any journey pass; it cannot make one
        // pass invisibly, and this is where that is enforced.
        let result = walk(
            "J",
            vec![step(1, "one", vec![outcome(Verdict::Specified, "a", None)])],
            vec!["ada.is_at_limit = false".to_owned()],
        );
        let text = render(&[result]);
        let stipulation = text.find("stipulated").expect("the stipulation is reported");
        let first_step = text.find("1.").expect("the step is reported");
        assert!(stipulation < first_step, "the stipulations come first:\n{text}");
    }

    #[test]
    fn a_long_step_title_is_shortened_rather_than_wrapped() {
        let long = "she does a thing and then another thing and then a third thing entirely";
        let result = walk(
            "J",
            vec![step(1, long, vec![outcome(Verdict::Specified, "a", None)])],
            Vec::new(),
        );
        let text = render(&[result]);
        assert!(text.contains('…'), "{text}");
        assert!(!text.contains(long), "{text}");
        // The verdict still lines up, which is the whole reason for shortening.
        assert!(text.contains("holds"), "{text}");
    }

    #[test]
    fn a_title_that_fits_is_left_exactly_as_written() {
        let result = walk(
            "J",
            vec![step(1, "she borrows it", vec![outcome(Verdict::Specified, "a", None)])],
            Vec::new(),
        );
        let text = render(&[result]);
        assert!(text.contains("she borrows it"), "{text}");
        assert!(!text.contains('…'), "{text}");
    }

    #[test]
    fn the_json_carries_every_verdict_and_the_line_it_is_about() {
        // What an agent iterates on. A report it cannot get a line number out
        // of is one it cannot act on.
        let result = walk(
            "J",
            vec![step(
                3,
                "she waits",
                vec![outcome(Verdict::Unspecified, "ada does X on S", Some("no surface"))],
            )],
            vec!["ada.x = 1".to_owned()],
        );
        let document = as_json(&[result]);
        assert_eq!(document[0]["journey"], "J");
        assert_eq!(document[0]["verdict"], "unspecified");
        assert_eq!(document[0]["stipulated"][0], "ada.x = 1");
        assert_eq!(document[0]["steps"][0]["number"], 3);
        assert_eq!(document[0]["steps"][0]["outcomes"][0]["line"], 7);
        assert_eq!(document[0]["steps"][0]["outcomes"][0]["verdict"], "unspecified");
        assert_eq!(document[0]["steps"][0]["outcomes"][0]["detail"], "no surface");
    }

    #[test]
    fn the_json_keeps_the_lines_that_held_too() {
        // Unlike the text, which is for reading. An agent checking off what it
        // has achieved needs the whole walk.
        let result = walk(
            "J",
            vec![step(1, "one", vec![outcome(Verdict::Specified, "a", None)])],
            Vec::new(),
        );
        let document = as_json(&[result]);
        assert_eq!(document[0]["steps"][0]["outcomes"].as_array().expect("an array").len(), 1);
        assert_eq!(document[0]["steps"][0]["outcomes"][0]["detail"], serde_json::Value::Null);
    }
}
