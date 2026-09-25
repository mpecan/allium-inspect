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
