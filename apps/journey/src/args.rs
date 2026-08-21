//! What the command accepts.
//!
//! Shaped after `allium` rather than after this repository's own binary,
//! because the point of this one is to sit beside the others in somebody's
//! Makefile: `allium check specs/ && allium-journey walk specs/ journeys/`.
//!
//! Three things carry that over, and each is a decision the sibling made:
//!
//! - a bare list of paths, mixing files and directories, searched
//!   **recursively** — `allium-inspect` searches one level, deliberately, but a
//!   spec set under a directory tree is what the CLI is pointed at;
//! - JSON on stdout, one document per input file, streamed rather than wrapped
//!   in an array, which is what `allium analyse specs/` prints;
//! - exit 0 for nothing to say, 1 for something reported, 2 for no usable
//!   input.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Walk user journeys against an Allium specification.
///
/// Each PATH is a `.allium` spec, a `.journey` file, or a directory holding
/// either. Directories are searched recursively.
///
/// Exit codes:
///   0  Nothing reported
///   1  One or more journeys reported something
///   2  No paths given, or no journeys and specs among them
#[derive(Debug, Parser)]
#[command(name = "allium-journey", version, about, long_about = None, verbatim_doc_comment)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run each journey against the spec and report what holds.
    Walk(Run),
    /// Check each journey against the spec without running it.
    ///
    /// Answers only what can be answered from the graph: whether the acts,
    /// surfaces and actors a journey names exist. Cheaper than a walk, and the
    /// half of the answer that never depends on a world.
    Check(Run),
}

impl Command {
    /// The subcommand name, as the document reports it.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Command::Walk(_) => "walk",
            Command::Check(_) => "check",
        }
    }

    #[must_use]
    pub fn options(&self) -> &Run {
        match self {
            Command::Walk(run) | Command::Check(run) => run,
        }
    }
}

#[derive(Debug, clap::Args)]
pub struct Run {
    /// Spec files, journey files, or directories holding them.
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,

    /// Exit 0 even when the spec cannot support a step.
    ///
    /// The mode a journey is *written* in: the steps a spec does not support
    /// yet are the backlog rather than a failure. A journey that could not be
    /// read is still an error, because that is a fault in the journey rather
    /// than a gap in the spec.
    #[arg(long)]
    pub report: bool,

    /// Print a report for a person instead of JSON.
    #[arg(long)]
    pub text: bool,

    /// The allium binary to run for `model` and `plan`.
    ///
    /// Those two are built in allium's binary crate, which declares no library
    /// target; `parse` and `analyse` are called directly and need nothing on
    /// PATH.
    #[arg(long, default_value = "allium", value_name = "PATH")]
    pub allium: PathBuf,
}
