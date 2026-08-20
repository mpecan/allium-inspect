//! A deterministic, three-valued simulator for `allium` rules.
//!
//! Allium is a specification language, not a programming language: its
//! expressions describe intent and are not all mechanically decidable. A
//! simulator therefore has three honest answers, not two — and the third one is
//! the point. Every expression evaluates to `Truth::True`, `Truth::False`
//! or `Truth::Unknown`, and `Unknown` carries the sub-expression and source
//! span that could not be decided, so the UI can show precisely what the
//! simulator did not know rather than guessing on the user's behalf.
//!
//! Determinism is a hard requirement, not an aspiration. Ordered maps
//! throughout, monotonic entity ids, and `now` is a field the user advances —
//! never a reading of the system clock. The same world and the same event
//! always produce a byte-identical outcome, which is what makes both snapshot
//! testing and mutation testing meaningful here.

#![forbid(unsafe_code)]

pub mod apply;
pub mod eval;
pub mod seed;
pub mod step;
pub mod truth;
pub mod value;
pub mod world;

pub use apply::{Application, Applied, Effect};
pub use eval::{Env, Evaluation, Unresolved, eval};
pub use seed::seed;
pub use step::{
    ClauseVerdict, Disposition, Enabled, InvariantVerdict, RuleOutcome, Sources, StepOutcome, step,
};
pub use truth::Truth;
pub use value::{EntityId, Instance, Value};
pub use world::{Event, World};
