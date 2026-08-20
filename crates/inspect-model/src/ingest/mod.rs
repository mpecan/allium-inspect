//! Turning four CLI documents per spec file into one linked graph.
//!
//! The order of the passes is load-bearing and worth stating once:
//!
//! 1. **`model`** describes entities better than the AST does, so entity nodes
//!    are built from it — but it carries no spans.
//! 2. **`parse`** supplies those spans, and everything `model` does not report:
//!    rules, surfaces, actors, invariants, value types and imports.
//! 3. **`plan`** says what each rule creates, reads and emits. That is the flow
//!    graph, already computed by analysis this crate should not repeat.
//! 4. **`analyse`** contributes the findings, and every document contributes
//!    diagnostics.
//!
//! Steps 1 to 4 run per file and see one file each. Linking then runs once over
//! the whole set, which is where a collection of files becomes a graph.

mod analyse;
mod json;
mod link;
mod model;
mod parse;
mod plan;
mod rules;
mod surfaces;
mod text;
mod writes;

use std::path::{Path, PathBuf};

pub use link::is_unresolved;

use crate::{
    graph::SpecGraph,
    program::Program,
    runner::{AlliumRunner, Command, RunError},
};

/// Why a spec set could not be turned into a graph.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// The CLI could not be run, or printed something unusable.
    #[error(transparent)]
    Run(#[from] RunError),

    /// A spec file could not be read from disk.
    #[error("could not read {}: {source}", path.display())]
    Source {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// No spec files were given, or none were found.
    #[error(
        "no .allium files found.\n\
         Point allium-inspect at a spec file, or at a directory containing some."
    )]
    NoSpecs,
}

/// Reads a spec file's text. Separated so ingestion can be driven from memory.
pub trait SourceReader {
    /// The text of the spec at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::Source`] when the file cannot be read.
    fn read(&self, path: &Path) -> Result<String, IngestError>;
}

/// Reads spec text from disk.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileReader;

impl SourceReader for FileReader {
    fn read(&self, path: &Path) -> Result<String, IngestError> {
        std::fs::read_to_string(path)
            .map_err(|source| IngestError::Source { path: path.to_path_buf(), source })
    }
}

/// Reads spec text from a list of `(path, text)` pairs.
///
/// The whole pipeline is then drivable from memory, which is how the ingestion
/// tests run real recorded CLI output with no filesystem involved.
#[derive(Debug, Clone, Default)]
pub struct MemoryReader(Vec<(PathBuf, String)>);

impl MemoryReader {
    /// A reader holding `text` for `path`.
    #[must_use]
    pub fn with(mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        self.0.push((path.into(), text.into()));
        self
    }
}

impl SourceReader for MemoryReader {
    fn read(&self, path: &Path) -> Result<String, IngestError> {
        self.0.iter().find(|(held, _)| held == path).map(|(_, text)| text.clone()).ok_or_else(
            || IngestError::Source {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no text was provided for this path",
                ),
            },
        )
    }
}

/// What one run of the CLI over a spec set produces.
///
/// Two halves with different audiences. The graph is what the browser draws and
/// is kept small enough to send whole; the program is the expression trees, an
/// order of magnitude larger, which only the simulator reads and which
/// therefore never leaves the process.
#[derive(Debug)]
pub struct Ingestion {
    pub graph: SpecGraph,
    pub program: Program,
}

impl Ingestion {
    /// An empty ingestion attributed to `allium_version`.
    ///
    /// The passes write into one of these rather than taking a graph and a
    /// program separately: they all contribute to the same result, and threading
    /// two `&mut` arguments through five call sites is how they drift apart.
    #[must_use]
    pub fn empty(allium_version: impl Into<String>) -> Self {
        Self { graph: SpecGraph::new(allium_version), program: Program::new() }
    }
}

/// Build one graph from every spec file in `paths`.
///
/// # Errors
///
/// Returns [`IngestError`] when `paths` is empty, when the CLI cannot be run, or
/// when a spec file cannot be read.
pub fn ingest<R: AlliumRunner, S: SourceReader>(
    runner: &R,
    reader: &S,
    paths: &[PathBuf],
) -> Result<Ingestion, IngestError> {
    if paths.is_empty() {
        return Err(IngestError::NoSpecs);
    }

    let mut into = Ingestion::empty(runner.version()?);

    for path in paths {
        let module = module_name(path);
        let source = reader.read(path)?;

        // Order matters; see the module documentation.
        let model = runner.run(Command::Model, path)?;
        model::ingest(&model, &module, &mut into.graph);
        analyse::ingest_diagnostics(&model, &module, &mut into.graph);

        let parsed = runner.run(Command::Parse, path)?;
        parse::ingest(&parsed, &module, &path.to_string_lossy(), &source, &mut into);
        analyse::ingest_diagnostics(&parsed, &module, &mut into.graph);

        let planned = runner.run(Command::Plan, path)?;
        plan::ingest(&planned, &module, &mut into.graph);
        analyse::ingest_diagnostics(&planned, &module, &mut into.graph);

        let analysed = runner.run(Command::Analyse, path)?;
        analyse::ingest_diagnostics(&analysed, &module, &mut into.graph);
        analyse::ingest_findings(&analysed, &module, &mut into.graph);

        // Last for this module, because it needs every node of it to have the
        // span the parse pass gave it.
        analyse::attribute(&mut into.graph, &module, &source);
    }

    link::link(&mut into.graph);
    Ok(into)
}

/// A spec file's module name: its file stem.
///
/// The same rule the `use` resolver applies to an import path, so a file and the
/// declaration importing it agree on what the module is called.
#[must_use]
pub fn module_name(path: &Path) -> String {
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        graph::{NodeId, NodeKind},
        runner::MapRunner,
    };

    #[test]
    fn a_module_is_named_after_its_file() {
        assert_eq!(module_name(Path::new("specs/catalogue.allium")), "catalogue");
        assert_eq!(module_name(Path::new("catalogue.allium")), "catalogue");
        assert_eq!(module_name(Path::new("")), "");
    }

    #[test]
    fn ingesting_no_paths_says_so_rather_than_returning_an_empty_graph() {
        // An empty graph would render as a blank canvas and look like a spec
        // with nothing in it.
        let runner = MapRunner::new("allium 3.5.3");
        let error = ingest(&runner, &MemoryReader::default(), &[]).unwrap_err();
        assert!(matches!(error, IngestError::NoSpecs));
        assert!(error.to_string().contains("no .allium files found"));
    }

    #[test]
    fn a_missing_document_is_reported_rather_than_skipped() {
        let runner = MapRunner::new("v");
        let reader = MemoryReader::default().with("a.allium", "");
        let error = ingest(&runner, &reader, &[PathBuf::from("a.allium")]).unwrap_err();
        assert!(matches!(error, IngestError::Run(RunError::NoFixture { .. })));
    }

    #[test]
    fn unreadable_source_names_the_file_it_could_not_read() {
        let runner = MapRunner::new("v").with(Command::Model, "a.allium", json!({}));
        let error =
            ingest(&runner, &MemoryReader::default(), &[PathBuf::from("a.allium")]).unwrap_err();
        match error {
            IngestError::Source { path, .. } => assert_eq!(path, PathBuf::from("a.allium")),
            other => panic!("expected a Source error, got {other}"),
        }
    }

    /// A runner holding the four documents a one-entity module produces.
    fn minimal_runner(path: &str) -> MapRunner {
        MapRunner::new("allium 3.5.3")
            .with(Command::Model, path, json!({"entities": [{"name": "Book", "kind": "internal"}]}))
            .with(
                Command::Parse,
                path,
                json!({"module": {"version": 3, "declarations": [{"Block": {
                    "span": {"start": 0, "end": 20},
                    "kind": "Entity",
                    "name": {"span": {"start": 7, "end": 11}, "name": "Book"},
                    "items": [],
                }}]}}),
            )
            .with(Command::Plan, path, json!({"obligations": []}))
            .with(Command::Analyse, path, json!({"diagnostics": [], "findings": []}))
    }

    #[test]
    fn the_graph_is_attributed_to_the_cli_that_produced_it() {
        let path = "catalogue.allium";
        let reader = MemoryReader::default().with(path, "entity Book {}\n");
        let graph =
            ingest(&minimal_runner(path), &reader, &[PathBuf::from(path)]).expect("ingests").graph;
        assert_eq!(graph.allium_version, "allium 3.5.3");
    }

    #[test]
    fn the_parse_pass_gives_the_model_pass_node_its_span() {
        // The single most important thing the two-pass order buys: `model`
        // describes the entity and `parse` is the only source of where it is.
        let path = "catalogue.allium";
        let reader = MemoryReader::default().with(path, "entity Book {}\n");
        let graph =
            ingest(&minimal_runner(path), &reader, &[PathBuf::from(path)]).expect("ingests").graph;
        let node =
            graph.node(&NodeId::new("catalogue", NodeKind::Entity, "Book")).expect("the entity");
        assert_eq!(node.span, Some(crate::span::Span::new(0, 20)));
        assert_eq!(graph.nodes.len(), 1, "and does not duplicate it");
    }

    #[test]
    fn a_module_row_is_recorded_for_every_file() {
        let path = "catalogue.allium";
        let reader = MemoryReader::default().with(path, "");
        let graph =
            ingest(&minimal_runner(path), &reader, &[PathBuf::from(path)]).expect("ingests").graph;
        assert_eq!(graph.modules.len(), 1);
        assert_eq!(graph.modules[0].name, "catalogue");
        assert_eq!(graph.modules[0].path, "catalogue.allium");
    }

    #[test]
    fn the_file_reader_returns_the_file_it_was_pointed_at() {
        // The one place this crate touches the filesystem, and the source it
        // returns is what every clause in the inspector is sliced from. A
        // reader that returned the empty string would silently blank every
        // clause and every source preview.
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/specs/catalogue.allium");
        let text = FileReader.read(&path).expect("the fixture spec is readable");
        assert!(text.contains("entity Book {"), "the real contents came back");
        assert!(text.len() > 500, "and not a truncated or invented string");
    }

    #[test]
    fn the_file_reader_names_the_file_it_could_not_read() {
        let error = FileReader.read(Path::new("/nonexistent/nope.allium")).unwrap_err();
        match error {
            IngestError::Source { path, .. } => {
                assert_eq!(path, PathBuf::from("/nonexistent/nope.allium"));
            }
            other => panic!("expected a Source error, got {other}"),
        }
    }

    #[test]
    fn a_memory_reader_returns_what_it_was_given() {
        let reader = MemoryReader::default().with("a.allium", "entity A {}");
        assert_eq!(reader.read(Path::new("a.allium")).expect("held"), "entity A {}");
        assert!(reader.read(Path::new("b.allium")).is_err());
    }
}
