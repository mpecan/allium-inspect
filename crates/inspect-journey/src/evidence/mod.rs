//! Pictures of a journey actually happening, and what is missing.
//!
//! A walk answers whether the *specification* supports a step. That is a
//! different claim from *the software does this*, and the second is the one a
//! reader wants when they open a journey. Nothing in this repository could make
//! it until now: the harness that photographs a running product and the journey
//! that says what it should do had no name in common.
//!
//! This is that name. A step is `<Journey>.<number>`, a harness writes a line
//! per picture, and a marker in source says which steps a test *claims* to
//! demonstrate. Three inputs, resolved into one standing per step.
//!
//! The claim half is what makes the absence legible. Without it, a test that
//! quietly stopped taking its picture and a step nobody ever covered leave the
//! same trace — none — and the tool would report the first as the second.
//!
//! Pure, like the rest of the crate. No filesystem and no clock: a timestamp is
//! read from a manifest, never taken. The reading is what the app layer does.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

pub mod manifest;
pub mod markers;
pub mod state;
pub mod text;

pub use manifest::{Frame, Manifest, SealError, Shot, seal};
pub use markers::{Claim, claims};
pub use state::{Declared, Resolution, Standing, StepEvidence, Undeclared, resolve};
pub use text::step_texts;

/// Which step of which journey, as a marker and a manifest both spell it.
///
/// The number rather than the position, and this is the reason the parser
/// insists on one: a step number is a citation, and a citation that moves when
/// somebody inserts a step above it quietly starts pointing elsewhere. A
/// photograph filed under a position would be re-filed by an edit that did not
/// touch it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/", type = "string")]
pub struct StepId {
    pub journey: String,
    pub number: u32,
}

impl StepId {
    #[must_use]
    pub fn new(journey: impl Into<String>, number: u32) -> Self {
        Self { journey: journey.into(), number }
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}.{}", self.journey, self.number)
    }
}

/// Why a step id could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadStepId {
    pub written: String,
}

impl fmt::Display for BadStepId {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "`{}` is not a step — expected `<Journey>.<number>`", self.written)
    }
}

impl std::str::FromStr for StepId {
    type Err = BadStepId;

    /// `SomebodyMeetsASpec.3`.
    ///
    /// Split at the **last** dot, so a journey name that contains one is read
    /// the way it was written rather than truncated at the first.
    fn from_str(written: &str) -> Result<Self, Self::Err> {
        let bad = || BadStepId { written: written.to_owned() };
        let (journey, number) = written.rsplit_once('.').ok_or_else(bad)?;
        if journey.is_empty() {
            return Err(bad());
        }
        let number: u32 = number.parse().map_err(|_| bad())?;
        Ok(Self { journey: journey.to_owned(), number })
    }
}

/// One string on the wire and in the manifest, because that is how a person
/// writes it in a comment and a manifest a person may have to read by hand.
impl Serialize for StepId {
    fn serialize<S: Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StepId {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        let written = String::deserialize(input)?;
        written.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_step_id() {
        assert_eq!("Journey.3".parse(), Ok(StepId::new("Journey", 3)));
    }

    #[test]
    fn splits_at_the_last_dot_so_a_dotted_name_survives() {
        assert_eq!("a.b.12".parse(), Ok(StepId::new("a.b", 12)));
    }

    #[test]
    fn refuses_what_is_not_a_step() {
        for written in ["Journey", "Journey.", ".3", "Journey.x", "Journey.-1", ""] {
            assert!(written.parse::<StepId>().is_err(), "`{written}` should not read as a step id");
        }
    }

    #[test]
    fn round_trips_through_json() {
        let id = StepId::new("SomebodyMeetsASpec", 4);
        let json = serde_json::to_string(&id).expect("a step id serialises");
        assert_eq!(json, "\"SomebodyMeetsASpec.4\"");
        assert_eq!(serde_json::from_str::<StepId>(&json).expect("and reads back"), id);
    }

    #[test]
    fn a_bad_id_says_what_it_wanted() {
        let error = "nope".parse::<StepId>().expect_err("`nope` is not a step id");
        assert!(error.to_string().contains("<Journey>.<number>"), "{error}");
    }
}
