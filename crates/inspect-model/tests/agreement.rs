//! Does the allium we call agree with the allium people run?
//!
//! `parse` and `analyse` are `allium_parser` function calls now, so nothing at
//! runtime consults a recording of them. The recordings stay anyway, because
//! the question they answer changed rather than went away: a reader who runs
//! `allium check` and then opens this tool has to be shown the same spec.
//!
//! `allium-parser` is pinned to the tag the recordings were made from. If the
//! two ever disagree, either the library is not what the CLI runs or the CLI
//! reshapes what the library returns before printing it — and both are things
//! to find out here rather than in a graph that quietly came out different.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

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
fn recorded(module: &str, command: &str) -> Value {
    serde_json::from_str(&read(&fixtures().join(format!("cli/{module}.{command}.json"))))
        .expect("the recording is JSON")
}

fn source_of(module: &str) -> String {
    read(&fixtures().join(format!("specs/{module}.allium")))
}

/// What the library returns for the same file, right now.
fn parsed(module: &str) -> Value {
    serde_json::to_value(allium_parser::parse(&source_of(module))).expect("the tree serialises")
}

fn analysed(module: &str) -> Value {
    let source = source_of(module);
    let tree = allium_parser::parse(&source);
    serde_json::to_value(allium_parser::analyse(&tree.module, &source))
        .expect("the analysis serialises")
}

#[test]
fn the_library_parses_to_the_same_ast_the_cli_prints() {
    // The equality the whole migration rests on. `ingest::parse` reads
    // `document["module"]` and nothing else, so if the trees match then every
    // rule, surface, actor and invariant in the graph is built from the same
    // bytes it was built from when this shelled out.
    for module in MODULES {
        assert_eq!(parsed(module)["module"], recorded(module, "parse")["module"], "{module}");
    }
}

#[test]
fn the_trees_are_not_trivially_equal() {
    // Guards the test above. Two nulls compare equal, and a parse that
    // silently returned nothing would pass it in silence.
    for module in MODULES {
        let tree = parsed(module);
        let declarations = tree["module"]["declarations"]
            .as_array()
            .unwrap_or_else(|| panic!("{module} has declarations"));
        assert!(declarations.len() > 5, "{module}: {}", declarations.len());
    }
}

#[test]
fn the_library_finds_what_the_cli_found() {
    // Findings are what the browser badges nodes with, so a set that came out
    // different would change the picture without changing the spec.
    for module in MODULES {
        assert_eq!(
            analysed(module)["findings"],
            recorded(module, "analyse")["findings"],
            "{module}"
        );
    }
}

#[test]
fn the_only_diagnostic_the_library_drops_is_one_the_cli_got_wrong() {
    // `allium analyse <file>` is told about one file, so a `use` of the file
    // beside it "does not resolve to a file in the current check set" — false,
    // because this tool is looking at the whole set and the file is in it.
    // Calling per file drops the false positive and keeps the rest.
    //
    // Pinned rather than waved past: this is the one place the two disagree,
    // and an unexplained difference during a migration is how a real
    // regression gets filed under "expected".
    let messages = |document: &Value| -> Vec<String> {
        document["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|each| each["message"].as_str().map(ToOwned::to_owned))
            .collect()
    };
    let library = messages(&analysed("lending"));
    let cli = messages(&recorded("lending", "analyse"));

    let dropped: Vec<&String> = cli.iter().filter(|message| !library.contains(message)).collect();
    assert_eq!(dropped.len(), 1, "{dropped:#?}");
    assert!(dropped[0].contains("does not resolve to a file"), "{}", dropped[0]);

    let added: Vec<&String> = library.iter().filter(|message| !cli.contains(message)).collect();
    assert!(added.is_empty(), "{added:#?}");
}

#[test]
fn genuine_cross_module_analysis_is_still_owed() {
    // The real fix is `analyse_with_cross_module`, which wants the resolved use
    // paths, imported triggers, entity fields and statuses of the whole set —
    // nine inputs a per-file pass has none of. Until it is given them, an
    // import that genuinely resolves to nothing is reported by the linking
    // pass, and this test is the note saying so.
    let document = analysed("lending");
    let messages: Vec<&str> = document["diagnostics"]
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
