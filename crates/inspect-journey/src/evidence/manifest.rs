//! What a run left behind, before and after it is sealed.
//!
//! A harness writes a [`Shot`] per picture, one JSON object a line, appended as
//! it goes. Append-only because a walk that is killed half way through is the
//! interesting case: what it managed to photograph before it stopped is exactly
//! the evidence somebody wants, and a document written once at the end would
//! have none of it.
//!
//! [`seal`] turns that log into a [`Manifest`]: it resolves every step id
//! against the journeys and stamps each frame with the step as it currently
//! reads. Sealing here rather than in each harness is what keeps one answer to
//! "what does this step say" — a harness that computed it in its own language
//! would be a second implementation, free to disagree.
//!
//! Sealing **refuses** rather than dropping. A frame naming a step no journey
//! has is a rename with half the work done, or a typo, and either way the
//! picture is filed under something that does not exist. Silently discarding it
//! would leave the step reading as never covered, which is the same lie this
//! module exists to prevent.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::evidence::StepId;

/// The version this crate writes and understands.
///
/// A manifest is read by a tool the harness that wrote it does not ship with,
/// so the two versions move independently and the number is what lets the
/// reader say so instead of misreading the fields.
pub const VERSION: u32 = 1;

/// One picture, as the harness recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    /// Which step this shows.
    pub step: StepId,
    /// The image file, relative to the directory the log is in.
    pub image: String,
    /// What the picture is of, in the harness's words.
    pub caption: Option<String>,
    /// Whether the run was still passing when this was taken.
    ///
    /// A picture from a failing run is not worthless — it is usually the most
    /// informative one there is, because it shows where the walk stopped. So it
    /// is kept and marked rather than dropped.
    pub passed: bool,
    /// When, as the harness spelled it. Never parsed here, only shown.
    pub taken_at: String,
    /// Where in the harness it was taken, for going there.
    pub source: Option<String>,
}

/// One picture, once the step it names has been resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Frame {
    pub step: StepId,
    pub image: String,
    pub caption: Option<String>,
    pub passed: bool,
    pub taken_at: String,
    pub source: Option<String>,
    /// The step as it read when this was taken.
    ///
    /// Stored rather than hashed, and the reason is what a reader is owed when
    /// it stops matching: a digest can say *something changed*, and this can
    /// show them what the step said then beside what it says now.
    pub said: String,
}

/// A sealed run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Manifest {
    pub version: u32,
    /// When the seal happened, as the app layer spelled it.
    pub sealed_at: String,
    /// Which walk this was, when the harness names itself.
    pub walk: Option<String>,
    pub frames: Vec<Frame>,
}

/// Why a run could not be sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// A frame naming a step no journey has.
    NoSuchStep { step: StepId, image: String },
    /// Two frames of one step with the same image, which is a double append.
    Duplicate { step: StepId, image: String },
}

impl fmt::Display for SealError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SealError::NoSuchStep { step, image } => {
                write!(out, "{image} says it shows `{step}`, and no journey has that step",)
            }
            SealError::Duplicate { step, image } => {
                write!(out, "{image} is filed under `{step}` twice")
            }
        }
    }
}

/// Turn a run's log into a manifest, or say why it cannot be one.
///
/// `steps` is every step there is, keyed by id — [`super::step_texts`] over
/// each journey file. `sealed_at` is passed in rather than read from a clock,
/// so this stays pure and a test can seal the same log twice and compare.
///
/// Every fault is reported, not just the first: a harness that renamed a
/// journey has one fault per picture, and fixing them one run at a time is a
/// worse afternoon than being told all of them.
///
/// # Errors
///
/// Returns every [`SealError`] found, in the order the shots were logged.
pub fn seal(
    shots: Vec<Shot>,
    steps: &BTreeMap<StepId, String>,
    sealed_at: impl Into<String>,
    walk: Option<String>,
) -> Result<Manifest, Vec<SealError>> {
    let mut frames = Vec::with_capacity(shots.len());
    let mut faults = Vec::new();
    let mut seen: Vec<(StepId, String)> = Vec::new();

    for shot in shots {
        let Some(said) = steps.get(&shot.step) else {
            faults.push(SealError::NoSuchStep { step: shot.step, image: shot.image });
            continue;
        };

        let key = (shot.step.clone(), shot.image.clone());
        if seen.contains(&key) {
            faults.push(SealError::Duplicate { step: shot.step, image: shot.image });
            continue;
        }
        seen.push(key);

        frames.push(Frame {
            step: shot.step,
            image: shot.image,
            caption: shot.caption,
            passed: shot.passed,
            taken_at: shot.taken_at,
            source: shot.source,
            said: said.clone(),
        });
    }

    if faults.is_empty() {
        Ok(Manifest { version: VERSION, sealed_at: sealed_at.into(), walk, frames })
    } else {
        Err(faults)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> BTreeMap<StepId, String> {
        BTreeMap::from([
            (StepId::new("First", 1), "1. she points at it".to_owned()),
            (StepId::new("First", 2), "2. and it answers".to_owned()),
        ])
    }

    fn shot(step: StepId, image: &str) -> Shot {
        Shot {
            step,
            image: image.to_owned(),
            caption: Some("a caption".to_owned()),
            passed: true,
            taken_at: "2026-08-24T09:00:00Z".to_owned(),
            source: Some("walk.ts:12".to_owned()),
        }
    }

    #[test]
    fn a_sealed_frame_carries_what_the_step_said() {
        let manifest = seal(
            vec![shot(StepId::new("First", 1), "01.png")],
            &steps(),
            "2026-08-24T09:01:00Z",
            Some("reading".to_owned()),
        )
        .expect("a clean log seals");

        assert_eq!(manifest.version, VERSION);
        assert_eq!(manifest.walk.as_deref(), Some("reading"));
        assert_eq!(
            manifest.frames.first().map(|frame| frame.said.as_str()),
            Some("1. she points at it")
        );
    }

    #[test]
    fn a_frame_naming_a_step_nothing_has_refuses() {
        let faults = seal(vec![shot(StepId::new("First", 9), "09.png")], &steps(), "now", None)
            .expect_err("a frame naming a missing step cannot seal");

        assert_eq!(
            faults,
            vec![SealError::NoSuchStep {
                step: StepId::new("First", 9),
                image: "09.png".to_owned()
            }]
        );
    }

    /// The direction that matters: a rename leaves every picture orphaned, and
    /// being told about one of them is a worse afternoon than all of them.
    #[test]
    fn every_fault_is_reported_not_only_the_first() {
        let faults = seal(
            vec![
                shot(StepId::new("Renamed", 1), "01.png"),
                shot(StepId::new("Renamed", 2), "02.png"),
            ],
            &steps(),
            "now",
            None,
        )
        .expect_err("two orphaned frames cannot seal");

        assert_eq!(faults.len(), 2);
    }

    #[test]
    fn the_same_picture_filed_twice_refuses() {
        let faults = seal(
            vec![shot(StepId::new("First", 1), "01.png"), shot(StepId::new("First", 1), "01.png")],
            &steps(),
            "now",
            None,
        )
        .expect_err("a double append cannot seal");

        assert!(matches!(faults.as_slice(), [SealError::Duplicate { .. }]));
    }

    #[test]
    fn two_pictures_of_one_step_are_ordinary() {
        let manifest = seal(
            vec![shot(StepId::new("First", 1), "01.png"), shot(StepId::new("First", 1), "02.png")],
            &steps(),
            "now",
            None,
        )
        .expect("two devices photographing one step is normal");

        assert_eq!(manifest.frames.len(), 2);
    }

    #[test]
    fn a_failing_run_still_seals_and_says_so() {
        let mut failing = shot(StepId::new("First", 2), "02.png");
        failing.passed = false;
        let manifest = seal(vec![failing], &steps(), "now", None).expect("a failing run seals");
        assert_eq!(manifest.frames.first().map(|frame| frame.passed), Some(false));
    }

    #[test]
    fn an_empty_log_seals_to_an_empty_manifest() {
        let manifest = seal(Vec::new(), &steps(), "now", None).expect("nothing is not a fault");
        assert!(manifest.frames.is_empty());
    }

    #[test]
    fn a_fault_says_which_picture_and_which_step() {
        let fault =
            SealError::NoSuchStep { step: StepId::new("Gone", 3), image: "07.png".to_owned() };
        let said = fault.to_string();
        assert!(said.contains("07.png"), "{said}");
        assert!(said.contains("Gone.3"), "{said}");
    }

    #[test]
    fn a_shot_reads_back_from_the_json_a_harness_would_write() {
        let line = r#"{"step":"First.1","image":"01.png","caption":null,"passed":true,"taken_at":"2026-08-24T09:00:00Z","source":null}"#;
        let shot: Shot = serde_json::from_str(line).expect("the documented shape reads");
        assert_eq!(shot.step, StepId::new("First", 1));
        assert!(shot.passed);
    }
}
