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
#[command(
    name = "allium-journey",
    version,
    about,
    long_about = None,
    verbatim_doc_comment,
    after_help = "Writing journeys? `allium-journey guide` carries the documentation."
)]
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
    /// What a test run showed of a journey, and what it did not.
    #[command(subcommand)]
    Evidence(Evidence),
    /// Print the documentation this binary was built with.
    ///
    /// An agent in somebody else's repository has this command on PATH and no
    /// checkout of the project that made it. The instructions travel with the
    /// binary so they cannot describe a different version of the grammar from
    /// the one it implements.
    Guide {
        #[arg(value_name = "TOPIC")]
        topic: Option<Topic>,
    },
}

impl Command {
    /// The subcommand name, as the document reports it.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Command::Walk(_) => "walk",
            Command::Check(_) => "check",
            Command::Evidence(_) => "evidence",
            Command::Guide { .. } => "guide",
        }
    }

    /// The walk options, for the two subcommands that take a specification.
    ///
    /// `evidence` reads journeys and never ingests a spec — it is asking what a
    /// *run* showed, which is a question about pictures and source rather than
    /// about what the specification permits. So it needs no `allium` on PATH,
    /// and this is `Option` rather than a fourth field nobody would fill in.
    #[must_use]
    pub fn options(&self) -> Option<&Run> {
        match self {
            Command::Walk(run) | Command::Check(run) => Some(run),
            Command::Evidence(_) | Command::Guide { .. } => None,
        }
    }
}

/// Which document to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Topic {
    /// Adding journeys to a repository that has none.
    Adopting,
    /// The grammar: every form a journey can contain.
    Reference,
    /// Pictures of a journey actually happening.
    Evidence,
    /// Why it is shaped this way, and what is left out on purpose.
    Design,
    /// A ready-made Claude Code skill, to paste into a repository.
    Skill,
}

/// Pictures of a journey actually happening.
///
/// A harness photographs a running product and appends a line per picture to
/// `frames.jsonl`; `seal` turns that log into a `manifest.json` that names the
/// step each picture shows and what that step said at the time. `check` reads
/// the manifest back, together with the markers in the code, and says where
/// every step stands.
#[derive(Debug, Subcommand)]
pub enum Evidence {
    /// Turn a run's log into a manifest, refusing anything that does not resolve.
    Seal(Sealing),
    /// Say where every step stands: shown, stale, claimed, or nothing.
    Check(Checking),
}

#[derive(Debug, clap::Args)]
pub struct Sealing {
    /// The directory holding `frames.jsonl` and the pictures.
    #[arg(value_name = "EVIDENCE")]
    pub evidence: PathBuf,

    /// Journey files, or directories holding them.
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,

    /// What to call this run in the manifest.
    #[arg(long, value_name = "NAME")]
    pub walk: Option<String>,

    /// When the seal happened, instead of now.
    ///
    /// For tests and for a reproducible build: a manifest is a file people
    /// commit and diff, and one that differs between two runs of the same walk
    /// only by a timestamp is a diff nobody can read.
    #[arg(long, value_name = "ISO8601")]
    pub at: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct Checking {
    /// The directory holding `manifest.json`.
    ///
    /// Optional, because a journey with no pictures at all is the ordinary
    /// starting state and the markers alone are worth reporting.
    #[arg(value_name = "EVIDENCE")]
    pub evidence: Option<PathBuf>,

    /// Journey files, or directories holding them.
    #[arg(long, value_name = "PATH", required = true)]
    pub journeys: Vec<PathBuf>,

    /// Where to look for `journey:` markers in source.
    #[arg(long, value_name = "PATH")]
    pub code: Vec<PathBuf>,

    /// Exit 0 even when a step is claimed, stale, or shows a failing run.
    #[arg(long)]
    pub report: bool,
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
