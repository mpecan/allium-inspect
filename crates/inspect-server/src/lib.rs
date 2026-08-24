//! The embedded HTTP server: a thin JSON shell around the two pure crates.
//!
//! Everything with behaviour worth testing lives in `inspect-model` and
//! `inspect-sim`. This crate exists to put those behind a handful of routes and
//! to serve the built UI out of the binary itself, so `allium-inspect` is one
//! file with no asset path to resolve.
//!
//! Simulation is stateless on purpose: the browser owns the world and posts it
//! with each step. There are no sessions and no shared mutable state beyond the
//! current inspection, which is replaced wholesale on a reload rather than
//! mutated in place.

#![forbid(unsafe_code)]

mod evidence;
mod journeys;
mod routes;
mod sim;
mod state;

pub use evidence::Evidence;
pub use journeys::{JourneyFile, JourneyReport};
pub use routes::{router, serve};
pub use state::{AppState, Inspection, ModuleSource};
