//! Does calling allium as a library give the same answer as running it?
//!
//! The recordings in `fixtures/cli/` are the real CLI's output, stamped with
//! the version that produced them. `allium-parser` is pinned to that same tag.
//! If the two disagree, one of the two beliefs this migration rests on is
//! wrong — either the library is not what the CLI runs, or the CLI reshapes
//! what the library returns before printing it — and this is where that shows
//! up rather than as a graph that quietly comes out different.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inspect_model::{
    Command,
    library::LibraryRunner,
    runner::{AlliumRunner, MapRunner},
};
use serde_json::Value;

const MODULES: [&str; 2] = ["catalogue", "lending"];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// What the CLI printed, as recorded.
fn recorded(module: &str, command: Command) -> Value {
    let path = fixtures().join(format!("cli/{module}.{command}.json"));
    serde_json::from_str(&read(&path)).expect("the recording is JSON")
}

/// What the library returns, right now.
fn from_library(module: &str, command: Command) -> Value {
    let path = fixtures().join(format!("specs/{module}.allium"));
    // The binary name is never reached: `parse` and `analyse` are answered in
    // process, and this test asks for nothing else.
    LibraryRunner::new("allium-not-on-path")
        .run(command, &path)
        .unwrap_or_else(|error| panic!("{command} on {module}: {error}"))
}

#[test]
fn the_library_parses_to_the_same_ast_the_cli_prints() {
    // The whole migration rests on this one equality. `ingest::parse` reads
    // `document["module"]` and nothing else from a parse document, so if the
    // trees match, every rule, surface, actor and invariant in the graph is
    // built from the same bytes it is today.
    for module in MODULES {
        let library = from_library(module, Command::Parse);
        let cli = recorded(module, Command::Parse);
        assert_eq!(library["module"], cli["module"], "{module}");
    }
}

#[test]
fn the_trees_are_not_trivially_equal() {
    // Guards the test above. Two `null`s compare equal, and a runner that
    // silently returned nothing would pass it in silence.
    for module in MODULES {
        let library = from_library(module, Command::Parse);
        let declarations = library["module"]["declarations"]
            .as_array()
            .unwrap_or_else(|| panic!("{module} has declarations"));
        assert!(declarations.len() > 5, "{module}: {}", declarations.len());
    }
}

#[test]
fn the_library_finds_what_the_cli_found() {
    // Analysis is the other half. Findings are what the browser badges nodes
    // with, so a set that came out different would change the picture without
    // changing the spec.
    for module in MODULES {
        let library = from_library(module, Command::Analyse);
        let cli = recorded(module, Command::Analyse);
        assert_eq!(library["findings"], cli["findings"], "{module}");
    }
}

#[test]
fn the_only_diagnostic_the_library_drops_is_one_the_cli_got_wrong() {
    // `allium analyse <file>` is told about one file, so a `use` of the file
    // beside it "does not resolve to a file in the current check set" — which
    // is false, because allium-inspect is looking at the whole set and that
    // file is in it. Calling the library per file drops the false positive and
    // keeps everything else.
    //
    // Pinned rather than waved past: this is the one place the two paths give
    // different answers, and an unexplained difference in a migration is how a
    // real regression gets filed under "expected".
    let library = from_library("lending", Command::Analyse);
    let cli = recorded("lending", Command::Analyse);

    let messages = |document: &Value| -> Vec<String> {
        document["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|each| each["message"].as_str().map(ToOwned::to_owned))
            .collect()
    };

    let dropped: Vec<String> = messages(&cli)
        .into_iter()
        .filter(|message| !messages(&library).contains(message))
        .collect();
    assert_eq!(dropped.len(), 1, "{dropped:#?}");
    assert!(dropped[0].contains("does not resolve to a file"), "{}", dropped[0]);

    // And nothing was invented on the way.
    let added: Vec<String> = messages(&library)
        .into_iter()
        .filter(|message| !messages(&cli).contains(message))
        .collect();
    assert!(added.is_empty(), "{added:#?}");
}

#[test]
fn genuine_cross_module_analysis_is_still_owed() {
    // The real fix is `analyse_with_cross_module`, which takes the resolved use
    // paths, imported triggers, entity fields and statuses of the whole set —
    // nine inputs this per-file runner has none of. Until it does, an import
    // that genuinely resolves to nothing is reported by the linking pass rather
    // than by allium, and this test is the note saying so.
    let library = from_library("lending", Command::Analyse);
    let messages: Vec<&str> = library["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|each| each["message"].as_str())
        .collect();
    assert!(
        !messages.iter().any(|message| message.contains("does not resolve to a file")),
        "{messages:#?}"
    );
}

#[test]
fn a_diagnostic_from_the_library_carries_a_line_to_point_at() {
    // The one edit this runner makes. The library reports a byte span, which
    // is the honest thing for a parser to carry; the browser shows a
    // diagnostic against a line, and a diagnostic with no line is one it
    // cannot point at.
    let path = fixtures().join("specs/lending.allium");
    let document = LibraryRunner::new("allium-not-on-path")
        .run(Command::Analyse, &path)
        .expect("analyse runs in process");
    for diagnostic in document["diagnostics"].as_array().into_iter().flatten() {
        assert!(
            diagnostic["location"]["line"].as_u64().is_some(),
            "every diagnostic has a line: {diagnostic}"
        );
    }
}

#[test]
fn model_and_plan_are_still_the_cli_s_to_answer() {
    // Both are built in `crates/allium`, which declares only a `[[bin]]`
    // target. Asking the library runner for one has to reach the process — and
    // with no binary on PATH, that has to fail rather than quietly return an
    // empty document that ingestion would read as a spec with no entities.
    for command in [Command::Model, Command::Plan] {
        let path = fixtures().join("specs/lending.allium");
        let outcome = LibraryRunner::new("allium-not-on-path").run(command, &path);
        assert!(outcome.is_err(), "{command} should have needed the CLI");
    }
}

#[test]
fn a_recorded_runner_still_answers_all_four() {
    // The fixture path is unchanged: ingestion is a pure function of four
    // documents and does not know which side of the seam any of them came
    // from. That is what makes the migration safe to do one command at a time.
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let path = root.join("specs/lending.allium");
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/lending.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    for command in Command::ALL {
        assert!(runner.run(command, &path).is_ok(), "{command}");
    }
}
