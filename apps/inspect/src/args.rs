//! What the binary accepts, and how spec files are found.

use std::path::{Path, PathBuf};

use clap::Parser;

/// Explore an allium specification in a browser.
#[derive(Debug, Parser)]
#[command(name = "allium-inspect", version, about, long_about = None)]
pub struct Args {
    /// Spec files, or directories to search for them.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Bind this port instead of an arbitrary free one.
    #[arg(long)]
    pub port: Option<u16>,

    /// Do not open a browser.
    #[arg(long)]
    pub no_open: bool,

    /// Do not reload when a spec file changes.
    #[arg(long)]
    pub no_watch: bool,

    /// Print the whole graph as JSON and exit.
    #[arg(long)]
    pub print_graph: bool,

    /// Journeys to check against the spec: a `.journey` file, or a directory.
    ///
    /// Journeys are the demand written first, so a step naming something the
    /// spec does not have is reported as a requirement rather than refused.
    #[arg(long, value_name = "PATH")]
    pub journeys: Option<PathBuf>,

    /// Fail when a journey names something the spec does not have.
    ///
    /// Off by default, because report is the mode a journey is *written* in.
    /// This is the mode a finished one is defended in.
    #[arg(long, requires = "journeys")]
    pub strict: bool,

    /// Print the journey report as JSON and exit.
    #[arg(long, requires = "journeys")]
    pub json: bool,

    /// The allium binary to run.
    #[arg(long, default_value = "allium")]
    pub allium: PathBuf,
}

/// Every `.journey` file under `path`, sorted.
///
/// One level deep, like the spec search and for the same reason: a directory of
/// journeys is a directory of journeys, and recursing sweeps in whatever else
/// happens to be under it.
#[must_use]
pub fn journeys(path: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else { return found };
        for entry in entries.flatten() {
            let candidate = entry.path();
            if candidate.extension().is_some_and(|extension| extension == "journey") {
                found.push(candidate);
            }
        }
    } else if path.extension().is_some_and(|extension| extension == "journey") {
        found.push(path.to_owned());
    }
    found.sort();
    found
}

/// Every `.allium` file under `paths`, sorted.
///
/// Directories are searched one level deep rather than recursively. A spec set
/// is a directory of files; recursing would sweep in vendored copies and
/// examples from a `node_modules` or a `target` and present them as one system.
///
/// Sorted because the ingestion order decides nothing but the graph is compared
/// between runs, and an unsorted directory read is a different order on a
/// different machine.
#[must_use]
pub fn resolve(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else { continue };
            for entry in entries.flatten() {
                let candidate = entry.path();
                if is_spec(&candidate) {
                    found.push(candidate);
                }
            }
        } else if is_spec(path) {
            found.push(path.clone());
        }
    }
    found.sort();
    found.dedup();
    found
}

fn is_spec(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|extension| extension == "allium")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A scratch directory holding `files`.
    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("allium-inspect-args-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("nested")).expect("scratch dir");
        for file in files {
            fs::write(dir.join(file), "-- allium: 3\n").expect("write");
        }
        dir
    }

    #[test]
    fn a_directory_yields_the_specs_inside_it() {
        let dir = tree("plain", &["b.allium", "a.allium", "notes.md"]);
        let found = resolve(std::slice::from_ref(&dir));
        let names: Vec<String> = found
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into())
            .collect();
        assert_eq!(names, ["a.allium", "b.allium"], "sorted, and only specs");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_named_file_is_taken_as_given() {
        let dir = tree("named", &["a.allium"]);
        assert_eq!(resolve(&[dir.join("a.allium")]), [dir.join("a.allium")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_named_file_that_is_not_a_spec_is_ignored() {
        let dir = tree("wrong-kind", &["notes.md"]);
        assert!(resolve(&[dir.join("notes.md")]).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_search_does_not_recurse() {
        // A spec set is a directory of files. Recursing sweeps in vendored
        // copies and examples and presents them as one system.
        let dir = tree("nested", &["a.allium", "nested/deep.allium"]);
        let found = resolve(std::slice::from_ref(&dir));
        assert_eq!(found, [dir.join("a.allium")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_same_file_named_twice_is_read_once() {
        let dir = tree("dupes", &["a.allium"]);
        let file = dir.join("a.allium");
        assert_eq!(resolve(&[file.clone(), file.clone()]), [file]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_directory_and_a_file_inside_it_do_not_double_up() {
        let dir = tree("overlap", &["a.allium"]);
        assert_eq!(resolve(&[dir.clone(), dir.join("a.allium")]).len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_path_that_does_not_exist_yields_nothing_rather_than_failing() {
        // The caller reports "no specs found" with the paths it looked in,
        // which is a better message than one about a single missing directory.
        assert!(resolve(&[PathBuf::from("/nowhere/at/all")]).is_empty());
    }

    #[test]
    fn no_paths_yields_nothing() {
        assert!(resolve(&[]).is_empty());
    }

    #[test]
    fn the_arguments_parse_with_their_defaults() {
        let args = Args::try_parse_from(["allium-inspect"]).expect("defaults parse");
        assert_eq!(args.paths, [PathBuf::from(".")]);
        assert_eq!(args.port, None);
        assert!(!args.no_open);
        assert!(!args.print_graph);
        assert_eq!(args.allium, PathBuf::from("allium"));
    }

    #[test]
    fn the_flags_are_accepted() {
        let args = Args::try_parse_from([
            "allium-inspect",
            "specs/",
            "--port",
            "8080",
            "--no-open",
            "--no-watch",
            "--print-graph",
            "--allium",
            "/opt/allium",
        ])
        .expect("flags parse");
        assert_eq!(args.paths, [PathBuf::from("specs/")]);
        assert_eq!(args.port, Some(8080));
        assert!(args.no_open && args.no_watch && args.print_graph);
        assert_eq!(args.allium, PathBuf::from("/opt/allium"));
    }
}
