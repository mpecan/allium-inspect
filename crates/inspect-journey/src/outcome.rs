//! Reading a step outcome as a journey verdict.
//!
//! The simulator answers about rules; a journey asks about a line somebody
//! wrote. This is the translation, and the whole of what it adds is care about
//! which of the two undecided-looking answers it is giving: a rule that refused
//! and a rule nobody could evaluate are different things, and only one of them
//! is the specification saying no.

use inspect_sim::{Disposition, Truth, step::StepOutcome};

use crate::check::Verdict;

/// How a step outcome reads as a verdict.
pub(crate) fn verdict_of(outcome: &StepOutcome) -> Verdict {
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Fired) {
        return Verdict::Specified;
    }
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Undecided) {
        return Verdict::Undecided;
    }
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Refused) {
        return Verdict::Refused;
    }
    // Nothing waits for it. The trigger exists — the checker said so — and no
    // rule consumes it, which is a fact about the spec rather than about this
    // world.
    Verdict::Remark
}

/// Why a step did not go through, in the spec's own words.
pub(crate) fn refusal(outcome: &StepOutcome) -> Option<String> {
    for rule in &outcome.rules {
        match rule.disposition {
            Disposition::Refused => {
                let blocking: Vec<&str> = rule
                    .requires
                    .iter()
                    .filter(|clause| clause.truth == Truth::False)
                    .map(|clause| clause.text.as_str())
                    .collect();
                return Some(format!("`{}` refused: {}", rule.name, blocking.join("; ")));
            }
            Disposition::Undecided => {
                let reason = rule.unresolved.first().map_or_else(
                    || "something could not be evaluated".to_owned(),
                    // The sub-expression as well as the reason, when the note
                    // located one. "`Member#1` has no `is_at_limit` set" says
                    // what is missing; it does not say which clause asked, and
                    // a rule with four preconditions has four candidates. The
                    // quote comes from the spec text, which is why the walker
                    // is handed it.
                    |note| match &note.expression {
                        Some(expression) => format!("{} — in `{expression}`", note.reason),
                        None => note.reason.clone(),
                    },
                );
                return Some(format!("`{}` could not be decided: {reason}", rule.name));
            }
            Disposition::Unsimulatable => {
                return Some(format!("`{}`'s clauses did not parse", rule.name));
            }
            Disposition::Fired => {}
        }
    }
    if outcome.rules.is_empty() {
        return Some("no rule waits for this".to_owned());
    }
    None
}
