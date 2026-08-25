//! Journeys written against an Allium spec: parsed, checked, and walked.
//!
//! Allium says what each rule does when its trigger happens, and has no
//! construct for saying what a person does next. This crate is the missing
//! script: a journey is one or more actors' paths through acts the spec
//! permits, and running one asks the specification whether it supports what
//! somebody says a person should be able to do.
//!
//! The direction matters. A journey is not a test written after a spec — it is
//! the demand written first, and a step naming a surface the spec does not have
//! is not an error but a requirement nobody has met. So a report is a ledger of
//! what the specification still owes, and `Verdict::Unspecified` is a first
//! class answer rather than a failure to parse.
//!
//! Pure, like the two crates it sits on. No clock, no filesystem, no process.

mod assert;
pub mod check;
pub mod evidence;
pub mod journey;
pub mod outcome;
pub mod parse;
pub mod report;
pub mod run;
mod world;

pub use check::{Note, Verdict, check};
pub use evidence::{
    Claim, Declared, Frame, Manifest, Resolution, Shot, Standing, StepEvidence, StepId, Undeclared,
    claims, resolve, seal, step_texts,
};
pub use journey::{
    Assertion, Axis, Cast, Clause, Comparison, Given, Journey, Path, Step, Stipulated, Term,
};
pub use parse::{ParseError, parse};
pub use report::{Strictness, as_json, passes, render};
pub use run::{CastMember, Origin, Outcome, Walk, Walked, notes_outside_steps, walk};
