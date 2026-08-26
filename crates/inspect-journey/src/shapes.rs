//! What a walk comes back as.
//!
//! The shapes a report is made of, kept apart from the walking that fills them
//! in. Every one of them crosses the wire — the browser draws this and the CLI
//! prints it — so they are the contract, and a change here is a change two
//! consumers see.
//!
//! Read them in the order a reader meets them: what the journey stands on,
//! what it was told, what its file laid out, who is in it, and then the steps.

use std::collections::BTreeSet;

use inspect_sim::world::World;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    check::{self, Verdict},
    journey::{Clause, Journey},
    run::Outcome,
};

/// Where a name in a journey came from.
///
/// Worth distinguishing because the three are read differently. A cast member
/// is somebody the journey declared up front; a given is an instance it
/// described in detail; and a catch is a thing that did not exist until a step
/// created it — which is the only one whose absence is a fault rather than a
/// choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Origin {
    /// Named in the `cast` block.
    Cast,
    /// Described in the `given` block.
    Given,
    /// Caught by a step: `creating loan: Loan`.
    Caught,
}

/// Something a journey was told rather than shown, and by whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Stipulation {
    /// The fact, as the journey put it: `ada.is_at_limit = false`.
    pub said: String,
    /// The journey that said it, when it was not this one.
    ///
    /// A chain carries its stipulations forward. Without this a reader has no
    /// way to tell what *this* journey was told from what it walked in on, and
    /// the rule that nothing passes invisibly would hold for one journey and
    /// not for two.
    pub through: Option<String>,
}

/// The journey this one continues from, and how that one came out.
///
/// `Ground` rather than anything about standing: `Standing` is already the
/// word for where a *step's* evidence stands, and two of them in one report
/// would be two vocabularies for a reader to keep apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Ground {
    pub journey: String,
    /// The same name with its words apart, for a heading.
    pub title: String,
    /// How the journey it stands on came out, over all of its steps.
    pub verdict: Verdict,
    pub held: usize,
    pub of: usize,
}

/// One line the file's world laid out, and what this journey did with it.
///
/// The three cases a reader has to be able to tell apart, and the third is the
/// dangerous one: what the journey said, what it inherited, and what it
/// inherited **and then changed**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Inherited {
    /// The line as the world wrote it: `ada.status = active`.
    pub said: String,
    /// Where in the file it was written.
    pub line: usize,
    /// Whether this journey went on to set the same thing itself.
    ///
    /// Allowed and reported, not forbidden. Eight journeys wanting the same
    /// two-member membership and one wanting it `departed` is the ordinary
    /// case; forbidding the override splits the file, and allowing it silently
    /// is the thing that must not happen.
    pub overridden: bool,
}

/// Somebody or something the journey named, and what it resolved to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct CastMember {
    /// What the journey calls them: `ada`, `loan`.
    pub name: String,
    /// As written, so `catalogue/Copy` keeps its module.
    pub type_expr: String,
    /// The instance in the world, once there was one.
    ///
    /// `None` when a step that was supposed to create it did not — which is
    /// the whole reason this is an option rather than a string.
    pub entity: Option<String>,
    pub origin: Origin,
    pub line: usize,
}

/// What became of one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Walked {
    pub number: u32,
    pub title: String,
    /// Where the step is written, for going there.
    ///
    /// The step's own heading rather than its first clause, because that is the
    /// line a reader scrubbing the walk wants the source strip to land on.
    pub line: usize,
    pub outcomes: Vec<Outcome>,
    /// The world as it stood when this step finished.
    ///
    /// Kept per step rather than only at the end, because the question a
    /// journey raises about a value is *when* it became that — and a single
    /// final state answers "what is Ada's loan now" while hiding the step that
    /// made it so.
    pub world: World,
}

impl Walked {
    /// The worst thing that happened in this step.
    /// How the journey came out, over its steps, its notes — and the ground.
    ///
    /// A journey that continues from another can never come out better than
    /// the one it stands on. Every step here was answered in a world that one
    /// built, so a green report over a broken foundation is the invisible pass
    /// this design exists to refuse — just with more distance between the two
    /// halves of it than usual.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        worst(self.outcomes.iter().map(|outcome| outcome.verdict))
    }
}

/// What became of a journey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Walk {
    /// The journey's name, which is its identity: an evidence marker says
    /// `journey: <name>.3`, and the panel finds a walk by it.
    pub name: String,
    /// The same name with its words apart, for a heading.
    ///
    /// Derived rather than declared. A journey already says what it is for in
    /// `goal:`, in the author's own words; a second place to write the same
    /// thing differently is a second place for them to disagree.
    pub title: String,
    /// Everybody the journey named, in the order the names were bound.
    pub cast: Vec<CastMember>,
    /// What the journey said it was for, and how it said it ends.
    ///
    /// Carried on the report rather than left in the source, because the whole
    /// question a reader has is whether the spec delivers *this* — and a list
    /// of verdicts with the intent stripped off cannot be read against it.
    pub goal: Vec<String>,
    pub ends: Vec<String>,
    /// Where the journey starts in its file, for going there.
    pub line: usize,
    pub steps: Vec<Walked>,
    /// What the journey was told rather than shown, always reported.
    ///
    /// Including everything the journeys it stands on were told, because an
    /// agent that could make this one pass by stipulating in the one before it
    /// would have found the loophole this rule exists to close.
    pub stipulated: Vec<Stipulation>,
    /// The journey this one continues from, and how that one came out.
    ///
    /// A world is a list of lines and is reported line by line; the end state
    /// of another journey is a world and cannot be. What can be said is that
    /// the ground under this is itself checked, and what its verdict was —
    /// which is stronger than a list, and is why [`Walk::verdict`] can never
    /// come out better than this.
    pub after: Option<Ground>,
    /// Every line the file's `world` laid out, and what became of it.
    ///
    /// Empty unless the journey inherits one, because otherwise this would
    /// print a journey's own `given` block back at it and say nothing. When it
    /// does inherit, this is the whole of what makes that safe: a step holding
    /// because of a line somewhere else in the file is passing invisibly, and
    /// this is where it stops being invisible.
    pub inherited: Vec<Inherited>,
    /// What was wrong with the journey outside its steps.
    ///
    /// Counted in the verdict, because the flagship case of this whole design
    /// is a requirement nobody has met — and a cast naming a type the spec
    /// does not have is exactly that, one line before the steps begin.
    pub notes: Vec<Outcome>,
}

impl Walk {
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        worst(
            self.steps
                .iter()
                .map(Walked::verdict)
                .chain(self.notes.iter().map(|note| note.verdict))
                .chain(self.after.iter().map(|standing| standing.verdict)),
        )
    }
}

/// Check notes that sit on no step clause, as outcomes.
///
/// `walk_step` reports the notes on a clause line and nothing else, so a
/// cast member whose type the spec does not have was computed and then
/// dropped by every consumer. Shared with `--check`, because a cast the spec
/// cannot supply is the same fault whether or not anybody ran the journey.
#[must_use]
pub fn notes_outside_steps(journey: &Journey, notes: &[check::Note]) -> Vec<Outcome> {
    let clause_lines: BTreeSet<usize> =
        journey.steps.iter().flat_map(|step| step.clauses.iter().map(Clause::line)).collect();
    notes
        .iter()
        .filter(|note| note.verdict != Verdict::Specified && !clause_lines.contains(&note.line))
        .map(|note| Outcome {
            line: note.line,
            verdict: note.verdict,
            about: describes(journey, note.line),
            detail: Some(note.message.clone()),
        })
        .collect()
}

/// What a journey line outside the steps is about, for reporting.
///
/// The check reports a line and a message; a reader needs to know which part
/// of the journey the line *was*. Cast and `given` are the only two places a
/// note can land outside a step, so those are the two it names.
fn describes(journey: &Journey, line: usize) -> String {
    if let Some(member) = journey.cast.iter().find(|member| member.line == line) {
        return format!("cast {}: {}", member.name, member.type_expr);
    }
    if let Some(crate::journey::Given::Instance { name, type_expr, .. }) =
        journey.given.iter().find(|given| given.line() == line)
    {
        return format!("given {name}: {type_expr}");
    }
    format!("line {line}")
}

/// Whether the world stopped changing on its own.
///
/// Reaching the bound used to push a sentence into `undecided`, which is a list
/// of *rule names* matched by exact equality — so it matched nothing, nothing
/// read it, and the two tests asserting it never appeared could not fail.
pub(crate) enum Settled {
    Yes,
    No { rounds: usize },
}

/// The worst of several, in the order a reader cares about them.
pub(crate) fn worst(verdicts: impl Iterator<Item = Verdict>) -> Verdict {
    fn rank(verdict: Verdict) -> u8 {
        match verdict {
            Verdict::Specified => 0,
            Verdict::Remark => 1,
            Verdict::Unexposed => 2,
            Verdict::Undecided => 3,
            Verdict::Unspecified => 4,
            Verdict::Refused => 5,
        }
    }
    verdicts.max_by_key(|verdict| rank(*verdict)).unwrap_or(Verdict::Specified)
}
