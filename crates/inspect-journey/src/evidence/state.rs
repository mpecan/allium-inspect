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

use crate::{
    evidence::{Claim, Frame, Manifest, StepId},
    journey::Axis,
};

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

/// One way a journey says it should be shown, on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Declared {
    pub key: String,
    /// In the order the author wrote them: they chose which question a reader
    /// meets first, and sorting would take that away.
    pub values: Vec<String>,
    /// The values no picture of this journey carries yet.
    ///
    /// A declaration is a demand, so this is the backlog for it — the same
    /// shape as a step the spec does not support, one level down.
    pub missing: Vec<String>,
    pub line: usize,
}

/// A tag outside what its journey declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Undeclared {
    pub step: StepId,
    pub image: String,
    pub key: String,
    pub value: String,
    /// Whether the *key* is unknown, rather than the value on a known key.
    ///
    /// Read differently: an unknown key is usually a typo for a declared one,
    /// and an unknown value is usually a way of showing the journey that
    /// somebody added without saying so.
    pub key_undeclared: bool,
}

/// Every step's standing, and everything that answers to nothing.
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
    /// The ways each journey says it should be shown, by journey name.
    ///
    /// Keyed by journey rather than pooled, because the question belongs to the
    /// journey that asked it: a set where one journey cares about `platform`
    /// should not offer that control on the ones that do not.
    pub axes: BTreeMap<String, Vec<Declared>>,
    /// Tags outside what their journey declared.
    ///
    /// Empty for a journey that declared nothing — declaring nothing constrains
    /// nothing, and a set that has never used this feature must not suddenly
    /// report every tag it has. Declaring one axis is opting in to being told
    /// about the rest.
    pub undeclared: Vec<Undeclared>,
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
    ///
    /// A *declared* value nobody has photographed is not a failure: a
    /// declaration is a demand written before the thing it asks for, the same
    /// as a journey step, and a gate failing on those would fail on every
    /// journey the day it was written.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        !self.unknown.is_empty()
            || !self.undeclared.is_empty()
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
    declared: &BTreeMap<String, Vec<Axis>>,
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

    let unknown: Vec<Claim> =
        claims.iter().filter(|claim| !steps.contains_key(&claim.step)).cloned().collect();

    Resolution {
        steps: resolved,
        unknown,
        axes: axes(declared, frames),
        undeclared: undeclared(declared, frames),
    }
}

/// What each journey declared, and which of it nothing has answered yet.
fn axes(
    declared: &BTreeMap<String, Vec<Axis>>,
    frames: &[Frame],
) -> BTreeMap<String, Vec<Declared>> {
    declared
        .iter()
        .map(|(journey, axes)| {
            let mine: Vec<&Frame> =
                frames.iter().filter(|frame| &frame.step.journey == journey).collect();

            let listed = axes
                .iter()
                .map(|axis| Declared {
                    key: axis.key.clone(),
                    values: axis.values.clone(),
                    missing: axis
                        .values
                        .iter()
                        .filter(|value| {
                            !mine.iter().any(|frame| {
                                frame.tags.get(&axis.key).is_some_and(|had| &had == value)
                            })
                        })
                        .cloned()
                        .collect(),
                    line: axis.line,
                })
                .collect();

            (journey.clone(), listed)
        })
        .collect()
}

/// Tags outside what their journey declared.
fn undeclared(declared: &BTreeMap<String, Vec<Axis>>, frames: &[Frame]) -> Vec<Undeclared> {
    let mut found = Vec::new();

    for frame in frames {
        // A journey that declared nothing constrains nothing.
        let Some(axes) = declared.get(&frame.step.journey).filter(|axes| !axes.is_empty()) else {
            continue;
        };

        for (key, value) in &frame.tags {
            let known = axes.iter().find(|axis| &axis.key == key);
            let key_undeclared = known.is_none();
            if key_undeclared || known.is_some_and(|axis| !axis.values.contains(value)) {
                found.push(Undeclared {
                    step: frame.step.clone(),
                    image: frame.image.clone(),
                    key: key.clone(),
                    value: value.clone(),
                    key_undeclared,
                });
            }
        }
    }

    found
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
            tags: BTreeMap::new(),
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
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Shown));
    }

    #[test]
    fn a_picture_of_what_the_step_used_to_say_is_stale() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", true)]);
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    /// Both halves in front of the reader, which is the whole reason the step
    /// text is stored rather than hashed.
    #[test]
    fn a_stale_step_carries_what_it_says_now() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", true)]);
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
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
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
        assert_eq!(resolution.at(&StepId::new("R", 1)).and_then(|e| e.says_now.clone()), None);
    }

    #[test]
    fn a_picture_from_a_run_that_stopped_here_is_marked_and_kept() {
        let sealed = manifest(vec![frame(StepId::new("R", 2), "2. and it answers", false)]);
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);

        assert_eq!(standing(&resolution, StepId::new("R", 2)), Some(Standing::Failing));
        assert_eq!(resolution.at(&StepId::new("R", 2)).map(|e| e.frames.len()), Some(1));
    }

    /// The state the source scan is for.
    #[test]
    fn a_marker_with_no_picture_is_claimed_not_absent() {
        let resolution = resolve(&steps(), &BTreeMap::new(), None, &[claim(StepId::new("R", 3))]);
        assert_eq!(standing(&resolution, StepId::new("R", 3)), Some(Standing::Claimed));
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Unclaimed));
    }

    #[test]
    fn a_picture_outranks_a_marker_for_the_same_step() {
        let sealed = manifest(vec![frame(StepId::new("R", 1), "1. she points at it", true)]);
        let resolution =
            resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[claim(StepId::new("R", 1))]);

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
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    #[test]
    fn stale_outranks_failing() {
        let sealed =
            manifest(vec![frame(StepId::new("R", 1), "1. she points at the shelf", false)]);
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);
        assert_eq!(standing(&resolution, StepId::new("R", 1)), Some(Standing::Stale));
    }

    #[test]
    fn a_marker_naming_no_step_is_reported_separately() {
        let resolution =
            resolve(&steps(), &BTreeMap::new(), None, &[claim(StepId::new("Renamed", 1))]);

        assert_eq!(resolution.unknown, vec![claim(StepId::new("Renamed", 1))]);
        assert!(
            resolution.steps.values().all(|e| e.standing == Standing::Unclaimed),
            "an unknown marker must not attach itself to a real step"
        );
    }

    #[test]
    fn every_step_gets_a_standing_even_with_nothing_to_go_on() {
        let resolution = resolve(&steps(), &BTreeMap::new(), None, &[]);
        assert_eq!(resolution.steps.len(), 3);
        assert_eq!(resolution.count(Standing::Unclaimed), 3);
    }

    #[test]
    fn nothing_to_look_at_is_not_a_failure() {
        let resolution = resolve(&steps(), &BTreeMap::new(), None, &[]);
        assert!(!resolution.is_failure(), "an unevidenced journey is a backlog, not a fault");
    }

    #[test]
    fn a_claim_with_no_picture_is_a_failure() {
        let resolution = resolve(&steps(), &BTreeMap::new(), None, &[claim(StepId::new("R", 1))]);
        assert!(resolution.is_failure());
    }

    #[test]
    fn a_marker_naming_no_step_is_a_failure() {
        let resolution =
            resolve(&steps(), &BTreeMap::new(), None, &[claim(StepId::new("Gone", 1))]);
        assert!(resolution.is_failure());
    }

    #[test]
    fn a_fully_shown_journey_is_not_a_failure() {
        let sealed =
            manifest(steps().iter().map(|(id, said)| frame(id.clone(), said, true)).collect());
        let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);

        assert_eq!(resolution.count(Standing::Shown), 3);
        assert!(!resolution.is_failure());
    }

    mod declared {
        use super::*;

        fn tagged(step: StepId, key: &str, value: &str) -> Frame {
            let mut frame = frame(step, "1. she points at it", true);
            frame.image = format!("{key}-{value}.png");
            frame.tags = BTreeMap::from([(key.to_owned(), value.to_owned())]);
            frame
        }

        fn says(values: &[&str]) -> BTreeMap<String, Vec<Axis>> {
            BTreeMap::from([(
                "R".to_owned(),
                vec![Axis {
                    key: "theme".to_owned(),
                    values: values.iter().map(|v| (*v).to_owned()).collect(),
                    line: 4,
                }],
            )])
        }

        #[test]
        fn a_declaration_reaches_the_panel_whether_or_not_anything_answered_it() {
            let resolution = resolve(&steps(), &says(&["dark", "light"]), None, &[]);
            let axes = resolution.axes.get("R").expect("R declared something");

            assert_eq!(axes.len(), 1);
            assert_eq!(axes[0].values, ["dark", "light"]);
            assert_eq!(axes[0].line, 4);
        }

        /// The backlog for a declaration: the values it asks for that nothing
        /// has photographed. The same shape as a step the spec does not
        /// support, one level down.
        #[test]
        fn a_value_nothing_answers_is_reported_missing() {
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "theme", "dark")]);
            let resolution = resolve(&steps(), &says(&["dark", "light"]), Some(&sealed), &[]);

            assert_eq!(resolution.axes["R"][0].missing, ["light"]);
        }

        #[test]
        fn a_value_something_answers_is_not_missing() {
            let sealed = manifest(vec![
                tagged(StepId::new("R", 1), "theme", "dark"),
                tagged(StepId::new("R", 2), "theme", "light"),
            ]);
            let resolution = resolve(&steps(), &says(&["dark", "light"]), Some(&sealed), &[]);

            assert!(resolution.axes["R"][0].missing.is_empty());
        }

        /// A demand nobody has met yet is the ordinary state of a journey, and
        /// a gate failing on it would fail on every one the day it was written.
        #[test]
        fn a_missing_value_is_not_a_failure() {
            let resolution = resolve(&steps(), &says(&["dark", "light"]), None, &[]);
            assert!(!resolution.is_failure());
        }

        #[test]
        fn a_value_nobody_declared_is_reported() {
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "theme", "sepia")]);
            let resolution = resolve(&steps(), &says(&["dark", "light"]), Some(&sealed), &[]);

            assert_eq!(resolution.undeclared.len(), 1);
            assert_eq!(resolution.undeclared[0].value, "sepia");
            assert!(!resolution.undeclared[0].key_undeclared, "the key is fine, the value is not");
            assert!(resolution.is_failure());
        }

        /// The typo this exists to catch. Told apart from an undeclared value
        /// because the two are usually different mistakes.
        #[test]
        fn a_key_nobody_declared_is_reported_as_a_key() {
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "them", "dark")]);
            let resolution = resolve(&steps(), &says(&["dark", "light"]), Some(&sealed), &[]);

            assert_eq!(resolution.undeclared.len(), 1);
            assert_eq!(resolution.undeclared[0].key, "them");
            assert!(resolution.undeclared[0].key_undeclared);
        }

        #[test]
        fn a_tag_the_journey_declared_is_not_reported() {
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "theme", "dark")]);
            let resolution = resolve(&steps(), &says(&["dark", "light"]), Some(&sealed), &[]);

            assert!(resolution.undeclared.is_empty());
            assert!(!resolution.is_failure());
        }

        /// Declaring nothing constrains nothing. A set that has never used the
        /// feature must not suddenly report every tag it has.
        #[test]
        fn a_journey_that_declares_nothing_reports_no_tag() {
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "anything", "at-all")]);
            let resolution = resolve(&steps(), &BTreeMap::new(), Some(&sealed), &[]);

            assert!(resolution.undeclared.is_empty());
            assert!(resolution.axes.is_empty());
            assert!(!resolution.is_failure());
        }

        /// One journey's question is not another's.
        #[test]
        fn a_declaration_does_not_reach_across_journeys() {
            let mut steps = steps();
            steps.insert(StepId::new("Other", 1), "1. elsewhere".to_owned());

            let mut frame = tagged(StepId::new("Other", 1), "platform", "ios");
            frame.said = "1. elsewhere".to_owned();
            let sealed = manifest(vec![frame]);

            let resolution = resolve(&steps, &says(&["dark", "light"]), Some(&sealed), &[]);

            // `Other` declared nothing, so its tag is nobody's business.
            assert!(resolution.undeclared.is_empty());
            assert!(!resolution.axes.contains_key("Other"));
        }

        /// A value answered on one journey does not answer for another's.
        #[test]
        fn a_picture_of_another_journey_does_not_fill_this_one_s_demand() {
            let mut steps = steps();
            steps.insert(StepId::new("Other", 1), "1. elsewhere".to_owned());

            let mut frame = tagged(StepId::new("Other", 1), "theme", "light");
            frame.said = "1. elsewhere".to_owned();
            let sealed = manifest(vec![tagged(StepId::new("R", 1), "theme", "dark"), frame]);

            let resolution = resolve(&steps, &says(&["dark", "light"]), Some(&sealed), &[]);
            assert_eq!(resolution.axes["R"][0].missing, ["light"]);
        }
    }

    #[test]
    fn a_step_id_is_a_plain_string_on_the_wire() {
        let resolution = resolve(&steps(), &BTreeMap::new(), None, &[]);
        let json = serde_json::to_string(&resolution).expect("a resolution serialises");
        assert!(json.contains("\"R.1\""), "{json}");
    }
}
