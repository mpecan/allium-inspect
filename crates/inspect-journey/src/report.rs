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
