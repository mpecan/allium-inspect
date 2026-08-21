//! Reaching allium as a library rather than as a process.
//!
//! `allium-parser` is allium's own parser and analyser, published from the same
//! repository as the CLI and pinned here to the tag the fixtures were recorded
//! from. Calling it directly removes a process launch, a JSON serialisation and
//! a JSON re-parse per file per command — but that is not the reason to do it.
//!
//! The reason is that a shape change upstream becomes a **compile error**.
//! Everything in [`crate::runner`] — the recorded fixtures, the version stamp,
//! the test that fails when the installed CLI differs — exists to catch, after
//! the fact, a drift that the type system can catch before the build finishes.
//!
//! Two of the four commands still go through the process. `model` and `plan`
//! are built in `crates/allium`, which declares only a `[[bin]]` target, so
//! they are not importable at any price. This runner delegates those and
//! answers the other two itself, which is why it holds a [`ProcessRunner`]
//! rather than replacing it.
//!
//! What it returns is the document the CLI would have printed, not a shape of
//! its own: ingestion is a pure function of those documents and none of it
//! needs to know which side of the seam a document came from. The one edit is
//! turning each diagnostic's byte span into the `line`/`col` location the CLI
//! reports, because the browser shows a diagnostic against a line and only
//! this side of the wire has the source text to work it out.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::{
    runner::{AlliumRunner, Command, ProcessRunner, RunError},
    span::LineIndex,
};

/// Runs `parse` and `analyse` in-process, and the rest through the CLI.
#[derive(Debug, Clone)]
pub struct LibraryRunner {
    process: ProcessRunner,
}

impl LibraryRunner {
    /// A runner that falls back to `binary` for `model` and `plan`.
    #[must_use]
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self { process: ProcessRunner::new(binary) }
    }

    /// The `parse` document, as the CLI prints it.
    fn parse(path: &Path) -> Result<Value, RunError> {
        let source = read(path)?;
        let parsed = allium_parser::parse(&source);
        let mut document = to_json(&parsed, Command::Parse, path)?;
        locate(&mut document, &source);
        Ok(document)
    }

    /// The `analyse` document, as the CLI prints it.
    ///
    /// Single-file, which is what this runner is asked for: cross-module
    /// analysis needs the whole set at once and is a different call. The
    /// unresolved-import warnings that produces are already reported by the
    /// linking pass, which sees every module.
    fn analyse(path: &Path) -> Result<Value, RunError> {
        let source = read(path)?;
        let parsed = allium_parser::parse(&source);
        let analysed = allium_parser::analyse(&parsed.module, &source);
        let mut document = to_json(&analysed, Command::Analyse, path)?;
        locate(&mut document, &source);
        Ok(document)
    }
}

impl AlliumRunner for LibraryRunner {
    fn run(&self, command: Command, path: &Path) -> Result<Value, RunError> {
        match command {
            Command::Parse => Self::parse(path),
            Command::Analyse => Self::analyse(path),
            // Built in allium's binary crate, which has no library target.
            Command::Model | Command::Plan => self.process.run(command, path),
        }
    }

    fn version(&self) -> Result<String, RunError> {
        // The CLI's, deliberately. It is still the source of `model` and
        // `plan`, and the version stamp exists to catch drift in *recorded*
        // shapes — which is now exactly those two.
        self.process.version()
    }
}

fn read(path: &Path) -> Result<String, RunError> {
    std::fs::read_to_string(path).map_err(|source| RunError::Spawn {
        command: Command::Parse,
        path: path.to_path_buf(),
        source,
    })
}

fn to_json<T: serde::Serialize>(
    value: &T,
    command: Command,
    path: &Path,
) -> Result<Value, RunError> {
    serde_json::to_value(value).map_err(|error| RunError::NotJson {
        command,
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

/// Give every diagnostic the `line`/`col` location the CLI reports.
///
/// The library reports a byte span, which is the honest thing for it to carry —
/// a parser has no opinion about how a reader counts lines. The CLI resolves it
/// against the source before printing, and so does this, because a diagnostic
/// with no line is one the browser cannot point at.
fn locate(document: &mut Value, source: &str) {
    let index = LineIndex::new(source);
    let Some(diagnostics) = document.get_mut("diagnostics").and_then(Value::as_array_mut) else {
        return;
    };
    for diagnostic in diagnostics {
        let Some(start) = diagnostic
            .get("span")
            .and_then(|span| span.get("start"))
            .and_then(Value::as_u64)
            .and_then(|start| usize::try_from(start).ok())
        else {
            continue;
        };
        let at = index.position(source, start);
        let Some(object) = diagnostic.as_object_mut() else { continue };
        let mut location = Map::new();
        location.insert("line".to_owned(), Value::from(at.line));
        location.insert("col".to_owned(), Value::from(at.column));
        object.insert("location".to_owned(), Value::Object(location));
    }
}
