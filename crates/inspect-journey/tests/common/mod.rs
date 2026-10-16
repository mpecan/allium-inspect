//! The fixture spec set, ingested exactly as the server ingests it.
//!
//! One loader for every test file here. It was four copies that differed only
//! in which modules they read and which halves they kept.

#![allow(dead_code, reason = "each test file uses the subset it needs")]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::step::Sources;

/// The fixture specs and the `allium` output recorded for them.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The lending fixture: catalogue and lending.
pub const LENDING: &[&str] = &["catalogue", "lending"];

/// The lending fixture and the desk beside it.
pub const DESK: &[&str] = &["catalogue", "lending", "desk"];

/// The packing fixture, alone: a set asked about through `all` and `any`.
pub const PACKING: &[&str] = &["packing"];

/// The grants fixture, alone: literals a journey says back to a spec.
pub const GRANTS: &[&str] = &["grants"];

/// The guards fixture, alone: exposures shown only while a `when` holds.
pub const GUARDS: &[&str] = &["guards"];

/// The orders fixture, alone: a rule that reacts to a creation.
pub const ORDERS: &[&str] = &["orders"];

/// The pinboard fixture and the module whose entity it creates.
pub const PINBOARD: &[&str] = &["notes", "pinboard"];

/// The migrations fixture and the module it emits a trigger into, written
/// `admin/FailureReported(…)` (issue #17).
pub const FAILURES: &[&str] = &["failures", "migrations"];

/// The cells fixture, alone: a reaction that makes another of its own kind.
pub const CELLS: &[&str] = &["cells"];

/// The tickets fixture, alone: reactions that refuse, split, and are reached
/// one emission away from the act.
pub const TICKETS: &[&str] = &["tickets"];

/// Two modules that each declare an `Order`.
pub const SHOPS: &[&str] = &["shop", "warehouse"];

/// The chains fixture and the module that listens to it across a boundary:
/// emitted triggers, and how what they carry reaches a listener's names.
pub const CHAINS: &[&str] = &["chains", "relay"];

/// The discards fixture and the module that listens to it across a boundary:
/// listeners that discard a parameter with `_` (issue #16).
pub const DISCARDS: &[&str] = &["discards", "hush"];

/// The deadlines fixture, alone: temporal triggers over optional timestamps
/// that may be `null` (issue #20).
pub const DEADLINES: &[&str] = &["deadlines"];

/// The cleanup fixture, alone: an instance removed while the clock moves, and
/// the state rules that were watching it (issue #22).
pub const CLEANUP: &[&str] = &["cleanup"];

/// The watches fixture, alone: a relationship filtered on a field each member
/// computes, and a loop over it (issue #23).
pub const WATCHES: &[&str] = &["watches"];

/// The fanout fixture, alone: a rule that iterates an entity collection and
/// emits once per member, and the rule chained from that emission.
pub const FANOUT: &[&str] = &["fanout"];

/// The switches fixture, alone: behaviour that is off until configuration
/// turns it on.
pub const SWITCHES: &[&str] = &["switches"];

/// The admins fixture and the module that faces its actor from outside: who a
/// surface faces, and whether somebody is one.
pub const FACING: &[&str] = &["admins", "portal"];

/// Two modules that each declare an `enum Status` and an `Item`, with
/// different states: which one a field means is the declaring module's.
pub const QUEUE: &[&str] = &["drafts", "queue"];

/// Listeners for triggers emitted from elsewhere, and the module that `use`s
/// theirs and emits one of them: who hears an emission, and a listener whose
/// own `let` binds what its `when` names.
pub const NEWSROOM: &[&str] = &["newsroom", "press"];

/// The runs fixture, alone: a status derived from words and compared against
/// the stored one (issue #21).
pub const RUNS: &[&str] = &["runs"];

/// The audience fixture, alone: a reaction to a creation that asks a black
/// box about the instance just made (issue #18).
pub const AUDIENCE: &[&str] = &["audience"];

/// The siblings fixture, alone: rules woken by one trigger, each reading the
/// world as the trigger found it (issue #24).
pub const SIBLINGS: &[&str] = &["siblings"];

/// `modules`, replayed from their recordings, with each module's source.
pub fn library(modules: &[&str]) -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();

    for module in modules {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        let text = read(&path);
        sources.insert((*module).to_owned(), text.clone());
        reader = reader.with(&path, text);
        paths.push(path);
    }

    let Ingestion { graph, program } =
        ingest(&runner, &reader, &paths).expect("the fixtures ingest");
    (graph, program, sources)
}
