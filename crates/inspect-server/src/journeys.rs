//! Authored journeys, walked against the spec the server is holding.
//!
//! A journey is the demand written first: somebody says what a person sets out
//! to do, and the tool answers with which steps this specification already
//! supports. So the report is rebuilt with the inspection rather than cached
//! beside it — edit the spec and the verdicts move, which is the whole loop.
//!
//! A file that does not parse is reported as itself rather than dropped. The
//! alternative is a journey silently vanishing from the list, which reads as
//! "it passed".

use std::path::{Path, PathBuf};

use inspect_journey::{Verdict, Walk, parse, walk};
use inspect_model::{Program, SpecGraph};
use inspect_sim::step::Sources;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One `.journey` file: what it holds, or why it could not be read.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct JourneyFile {
    /// As given on the command line, so a reader can find it on disk.
    pub path: String,
    /// The file's own name, which is what the list shows.
    pub name: String,
    /// Why the file could not be read, if it could not.
    pub error: Option<String>,
    pub walks: Vec<Walk>,
}

/// The whole report, and the one number that summarises it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct JourneyReport {
    pub files: Vec<JourneyFile>,
    /// How many journeys hold end to end, out of how many were walked.
    pub holding: usize,
    pub total: usize,
}

impl JourneyReport {
    /// Walk every journey in `files` against `graph`.
    #[must_use]
    pub fn build(
        files: &[PathBuf],
        graph: &SpecGraph,
        program: &Program,
        sources: &Sources,
    ) -> Self {
        let files: Vec<JourneyFile> =
            files.iter().map(|path| read_one(path, graph, program, sources)).collect();
        let walks = files.iter().flat_map(|file| file.walks.iter());
        let total = walks.clone().count();
        let holding = walks.filter(|walk| walk.verdict() == Verdict::Specified).count();
        Self { files, holding, total }
    }

    /// A report with nothing in it, for a server started without `--journeys`.
    #[must_use]
    pub fn empty() -> Self {
        Self { files: Vec::new(), holding: 0, total: 0 }
    }
}

fn read_one(path: &Path, graph: &SpecGraph, program: &Program, sources: &Sources) -> JourneyFile {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let mut file = JourneyFile {
        path: path.to_string_lossy().into_owned(),
        name,
        error: None,
        walks: Vec::new(),
    };

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            file.error = Some(format!("could not be read: {error}"));
            return file;
        }
    };
    match parse(&text) {
        // The parse error already names its own line, which is what turns it
        // into somewhere to go rather than a complaint about a file.
        Err(error) => file.error = Some(error.to_string()),
        Ok(journeys) => {
            file.walks =
                journeys.iter().map(|journey| walk(journey, graph, program, sources)).collect();
        }
    }
    file
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use inspect_model::{Node, NodeKind, SpecGraph};

    use super::*;

    /// A file under a fresh directory, written for one test.
    fn written(name: &str, text: &str) -> (tempdir::Dir, PathBuf) {
        let dir = tempdir::Dir::new();
        let path = dir.path().join(name);
        std::fs::write(&path, text).expect("the fixture is writable");
        (dir, path)
    }

    fn spec() -> SpecGraph {
        let mut graph = SpecGraph::new("allium 3.5.3");
        graph.nodes.push(Node::new("lending", NodeKind::Entity, "Member"));
        graph
    }

    fn build(paths: &[PathBuf]) -> JourneyReport {
        JourneyReport::build(paths, &spec(), &Program::new(), &BTreeMap::new())
    }

    const ONE: &str =
        "journey SheJoins {\n    goal: She joins.\n\n    cast:\n        ada: Member\n}\n";

    #[test]
    fn a_journey_is_walked_and_named_by_its_file() {
        let (_dir, path) = written("lending.journey", ONE);
        let report = build(&[path]);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].name, "lending.journey");
        assert_eq!(report.files[0].error, None);
        assert_eq!(report.files[0].walks.len(), 1);
        assert_eq!(report.files[0].walks[0].name, "SheJoins");
    }

    #[test]
    fn a_journey_with_no_steps_holds_vacuously_and_is_counted() {
        let (_dir, path) = written("lending.journey", ONE);
        let report = build(&[path]);
        assert_eq!(report.total, 1);
        assert_eq!(report.holding, 1);
    }

    #[test]
    fn a_file_that_does_not_parse_is_reported_rather_than_dropped() {
        // A journey silently vanishing from the list reads as "it passed",
        // which is the worst answer available: the reader believes the spec
        // supports something nothing checked.
        let (_dir, path) = written("broken.journey", "journey Unclosed {\n    cast:\n");
        let report = build(&[path]);
        assert_eq!(report.files.len(), 1);
        assert!(report.files[0].error.is_some(), "{:?}", report.files[0]);
        assert!(report.files[0].walks.is_empty());
        // And it is not counted as holding, which would be the same lie in a
        // number.
        assert_eq!(report.holding, 0);
        assert_eq!(report.total, 0);
    }

    #[test]
    fn the_error_names_the_line_so_there_is_somewhere_to_go() {
        let (_dir, path) = written("broken.journey", "journey J {\n    cast:\n        ada:\n}\n");
        let report = build(&[path]);
        let error = report.files[0].error.clone().expect("an error");
        assert!(error.contains("line 3"), "{error}");
    }

    #[test]
    fn a_file_that_cannot_be_read_says_so_against_its_own_name() {
        let dir = tempdir::Dir::new();
        let missing = dir.path().join("gone.journey");
        let report = build(&[missing]);
        assert_eq!(report.files[0].name, "gone.journey");
        assert!(
            report.files[0].error.as_deref().is_some_and(|e| e.contains("could not be read")),
            "{:?}",
            report.files[0].error
        );
    }

    #[test]
    fn a_server_started_without_journeys_reports_an_empty_one() {
        // Not an error and not a missing route: nobody asked for any.
        let report = JourneyReport::empty();
        assert!(report.files.is_empty());
        assert_eq!(report.total, 0);
        assert_eq!(report.holding, 0);
    }

    #[test]
    fn holding_counts_journeys_rather_than_files() {
        let two = format!("{ONE}\njourney SheLeaves {{\n    cast:\n        ada: Member\n}}\n");
        let (_dir, path) = written("two.journey", &two);
        let report = build(&[path]);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.total, 2);
        assert_eq!(report.holding, 2);
    }

    /// A directory that removes itself, so a failing test leaves nothing behind.
    ///
    /// Home-grown because the alternative is a dependency for six tests, and
    /// the rent comment it would need in `Cargo.toml` would be longer than this.
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new() -> Self {
                // The process id and a counter, not a clock or a random number:
                // the same reasons the two pure crates have neither.
                use std::sync::atomic::{AtomicU32, Ordering};
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "inspect-journeys-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).expect("a temp directory");
                Self(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
