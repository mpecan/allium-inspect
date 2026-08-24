//! Where a step stands: shown, claimed, stale, or nothing.
//!
//! Three inputs — the journeys as they now read, the manifest a run wrote, and
//! the markers scanned out of source — resolve to one standing per step. The
//! whole value of the feature is in telling the last two apart from the first,
//! and from each other.
//!
//! The precedence is worst-first, the same shape as a journey verdict: a step
//! with one stale picture and one fresh one is stale, because a reader who is
//! shown the fresh one and not told about the other has been told the
//! flattering half.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::evidence::{Claim, Frame, Manifest, StepId};

/// How well a step is evidenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Standing {
    /// A picture, from a run that was still passing.
    Shown,
    /// A picture from a run that stopped here. Kept, and marked: it is usually
    /// the most informative frame there is.
    Failing,
    /// A picture taken before the step was reworded, so it may no longer show
    /// what the step says.
    Stale,
    /// A test says it demonstrates this and produced nothing.
    Claimed,
    /// Nobody says they demonstrate this.
    Unclaimed,
}

impl Standing {
    /// Whether this is something a reader should look at.
    #[must_use]
    pub fn needs_attention(self) -> bool {
        matches!(self, Standing::Failing | Standing::Stale | Standing::Claimed)
    }

    /// Whether a gate should fail on it.
    ///
    /// Unclaimed is not a failure: most steps of most journeys are demand
    /// nobody has built yet, and a gate that failed on those would fail on
    /// every journey ever written — the same reasoning that keeps `undecided`
    /// out of `Verdict::is_failure`.
    #[must_use]
    pub fn is_failure(self) -> bool {
        matches!(self, Standing::Failing | Standing::Stale | Standing::Claimed)
    }
}

/// One step, and everything anybody has said about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct StepEvidence {
    pub standing: Standing,
    /// The pictures of this step, in the order the run took them.
    pub frames: Vec<Frame>,
    /// Where the code says it demonstrates this.
    pub claims: Vec<Claim>,
    /// What the step says now, when a frame disagrees with it.
    ///
    /// Only carried when something is stale, because that is the only time a
    /// reader needs both halves in front of them.
    pub says_now: Option<String>,
}

/// Every step's standing, and the markers that name no step at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Resolution {
    pub steps: BTreeMap<String, StepEvidence>,
    /// Markers naming a step no journey has.
    ///
    /// A rename with half the work done, or a typo. Either way the test still
    /// passes while claiming to cover something that does not exist, which is
    /// the case worth catching.
    pub unknown: Vec<Claim>,
}

impl Resolution {
    /// The standing of one step, for a caller that has an id in hand.
    #[must_use]
    pub fn at(&self, step: &StepId) -> Option<&StepEvidence> {
        self.steps.get(&step.to_string())
    }

    /// How many steps stand this way.
    #[must_use]
    pub fn count(&self, standing: Standing) -> usize {
        self.steps.values().filter(|evidence| evidence.standing == standing).count()
    }

    /// Whether a gate should fail on this resolution.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        !self.unknown.is_empty()
            || self.steps.values().any(|evidence| evidence.standing.is_failure())
    }
}

/// Resolve the three inputs into one standing per step.
///
/// `steps` is every step there is and what it currently says —
/// [`super::step_texts`] over each journey file. Keyed by the rendered id
/// rather than by [`StepId`] because this crosses the wire, and a map with a
/// structured key is an array of pairs in JSON, which nothing on the other side
/// wants to index.
#[must_use]
pub fn resolve(
    steps: &BTreeMap<StepId, String>,
    manifest: Option<&Manifest>,
    claims: &[Claim],
) -> Resolution {
    let frames: &[Frame] = manifest.map_or(&[], |manifest| &manifest.frames);
    let mut resolved = BTreeMap::new();

    for (id, says_now) in steps {
        let mine: Vec<Frame> = frames.iter().filter(|frame| &frame.step == id).cloned().collect();
        let claimed: Vec<Claim> =
            claims.iter().filter(|claim| &claim.step == id).cloned().collect();

        let stale = mine.iter().any(|frame| &frame.said != says_now);
        let standing = if stale {
            Standing::Stale
        } else if mine.iter().any(|frame| !frame.passed) {
            Standing::Failing
        } else if !mine.is_empty() {
            Standing::Shown
        } else if claimed.is_empty() {
            Standing::Unclaimed
        } else {
            Standing::Claimed
        };

        resolved.insert(
            id.to_string(),
            StepEvidence {
                standing,
                frames: mine,
                claims: claimed,
                says_now: stale.then(|| says_now.clone()),
            },
        );
    }

    let unknown = claims.iter().filter(|claim| !steps.contains_key(&claim.step)).cloned().collect();

    Resolution { steps: resolved, unknown }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::manifest::VERSION;

    fn steps() -> BTreeMap<StepId, String> {
        BTreeMap::from([
            (StepId::new("R", 1), "1. she points at it".to_owned()),
            (StepId::new("R", 2), "2. and it answers".to_owned()),
            (StepId::new("R", 3), "3. she reads it".to_owned()),
        ])
    }

    fn frame(step: StepId, said: &str, passed: bool) -> Frame {
        Frame {
            step,
            image: "01.png".to_owned(),
            caption: None,
            passed,
            taken_at: "2026-08-24T09:00:00Z".to_owned(),
            source: None,
            said: said.to_owned(),
        }
    }

    fn manifest(frames: Vec<Frame>) -> Manifest {
        Manifest { version: VERSION, sealed_at: "now".to_owned(), walk: None, frames }
    }

    fn claim(step: StepId) -> Claim {
        Claim { step, file: "walk.ts".to_owned(), line: 12 }
    }

    fn standing(resolution: &Resolution, step: StepId) -> Option<Standing> {
        resolution.at(&step).map(|evidence| evidence.standing)
    }

    #[test]
    fn a_fresh_picture_from_a_passing_run_is_shown() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at it", true)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Shown));
    }

    #[test]
    fn a_picture_of_what_the_step_used_to_say_is_stale() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", true)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    /// Both halves in front of the reader, which is the whole reason the step
    /// text is stored rather than hashed.
    #[test]
    fn a_stale_step_carries_what_it_says_now() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", true)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        let evidence = resolution.at(&StepId::new("R", 1)).expect("step 1 resolved");

        assert_eq!(evidence.says_now.as_deref(), Some("1. she points at it"));
        assert_eq!(
            evidence.frames.first().map(|frame| frame.said.as_str()),
            Some("1. she points at the shelf")
        );
    }

    #[test]
    fn a_fresh_step_does_not_carry_a_second_copy_of_itself() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at it", true)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        assert_eq!(resolution.at(&StepId::new("R", 1)).and_then(|e| e.says_now.clone()), None);
    }

    #[test]
    fn a_picture_from_a_run_that_stopped_here_is_marked_and_kept() {
        let sealed = manifest(vec![frame(StepId::new("R", 2), "2. and it answers", false)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);

        assert_eq!(standing(&resolution, StepId::new("R", 2)), Some(Standing::Failing));
        assert_eq!(resolution.at(&StepId::new("R", 2)).map(|e| e.frames.len()), Some(1));
    }

    /// The state the source scan is for.
    #[test]
    fn a_marker_with_no_picture_is_claimed_not_absent() {
        let resolution = resolve(&steps(), None, &[claim(StepId::new("R", 3))]);
        assert_eq!(standing(&resolution, StepId::new("R", 3)), Some(Standing::Claimed));
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Unclaimed));
    }

    #[test]
    fn a_picture_outranks_a_marker_for_the_same_step() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at it", true)]);
        let resolution = resolve(&steps(), Some(&sealed), &[claim(StepId::new("R", 1))]);

        let evidence = resolution.at(&StepId::new("R", 1)).expect("step 1 resolved");
        assert_eq!(evidence.standing, Standing::Shown);
        assert_eq!(evidence.claims.len(), 1, "the marker is still reported");
    }

    /// Worst-first: the flattering half is not the whole answer.
    #[test]
    fn one_stale_picture_makes_the_step_stale_however_many_are_fresh() {
        let sealed = manifest(vec![
            frame(StepId::new("R", 1), "1. she points at it", true),
            frame(StepId::new("R", 1), "1. she points at the shelf", true),
        ]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    #[test]
    fn stale_outranks_failing() {
        let sealed =
            manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", false)]);
        let resolution = resolve(&steps(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    #[test]
    fn a_marker_naming_no_step_is_reported_separately() {
        let resolution = resolve(&steps(), None, &[claim(StepId::new("Renamed", 1))]);

        assert_eq!(resolution.unknown, vec![claim(StepId::new("Renamed", 1))]);
        assert!(
            resolution.steps.values().all(|e| e.standing == Standing::Unclaimed),
            "an unknown marker must not attach itself to a real step"
        );
    }

    #[test]
    fn every_step_gets_a_standing_even_with_nothing_to_go_on() {
        let resolution = resolve(&steps(), None, &[]);
        assert_eq!(resolution.steps.len(), 3);
        assert_eq!(resolution.count(Standing::Unclaimed), 3);
    }

    #[test]
    fn nothing_to_look_at_is_not_a_failure() {
        let resolution = resolve(&steps(), None, &[]);
        assert!(!resolution.is_failure(), "an unevidenced journey is a backlog, not a fault");
    }

    #[test]
    fn a_claim_with_no_picture_is_a_failure() {
        let resolution = resolve(&steps(), None, &[claim(StepId::new("R", 1))]);
        assert!(resolution.is_failure());
    }

    #[test]
    fn a_marker_naming_no_step_is_a_failure() {
        let resolution = resolve(&steps(), None, &[claim(StepId::new("Gone", 1))]);
        assert!(resolution.is_failure());
    }

    #[test]
    fn a_fully_shown_journey_is_not_a_failure() {
        let sealed =
            manifest(steps().iter().map(|(id, said)| frame(id.clone(), said, true)).collect());
        let resolution = resolve(&steps(), Some(&sealed), &[]);

        assert_eq!(resolution.count(Standing::Shown), 3);
        assert!(!resolution.is_failure());
    }

    #[test]
    fn a_step_id_is_a_plain_string_on_the_wire() {
        let resolution = resolve(&steps(), None, &[]);
        let json = serde_json::to_string(&resolution).expect("a resolution serialises");
        assert!(json.contains("\"R.1\""), "{json}");
    }
}
