//! What the binary accepts, and how spec files are found.

use std::path::{Path, PathBuf};

use clap::Parser;

/// Explore an allium specification in a browser.
#[derive(Debug, Parser)]
#[command(name = "allium-inspect", version, about, long_about = None)]
pub struct Args {
    /// Spec files, or directories to search for them.
    ///
    /// No clap default: empty has to mean *nothing was given*, so that a
    /// `.allium-inspect.toml` can say `specs = "specs/"` and be heard. The
    /// fallback to `.` is applied once, in [`crate::config::Settings`], where
    /// every other default is.
    pub paths: Vec<PathBuf>,

    /// Bind this port instead of an arbitrary free one.
    #[arg(long)]
    pub port: Option<u16>,

    /// Do not open a browser.
    #[arg(long)]
    pub no_open: bool,

    /// Open a browser, whatever the configuration file says.
    #[arg(long, conflicts_with = "no_open")]
    pub open: bool,

    /// Do not reload when a spec file changes.
    #[arg(long)]
    pub no_watch: bool,

    /// Reload when a spec file changes, whatever the configuration file says.
    #[arg(long, conflicts_with = "no_watch")]
    pub watch: bool,

    /// Read this configuration file instead of looking for one.
    ///
    /// Named explicitly, so a file that does not exist is an error rather than
    /// silence — the opposite of the search, where finding nothing is ordinary.
    #[arg(long, value_name = "PATH", conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    /// Ignore any `.allium-inspect.toml`.
    ///
    /// The escape hatch for a file two directories up doing something
    /// unexpected: run once without it and compare.
    #[arg(long)]
    pub no_config: bool,

    /// Print the whole graph as JSON and exit.
    #[arg(long)]
    pub print_graph: bool,

    /// Journeys to walk against the spec: a `.journey` file, or a directory.
    ///
    /// Journeys are the demand written first, so a step naming something the
    /// spec does not have is reported as a requirement rather than refused.
    /// Served in the browser's Journeys view, and re-walked on every reload.
    #[arg(long, value_name = "PATH")]
    pub journeys: Option<PathBuf>,

    /// Print the journey report at the terminal and exit, instead of serving.
    #[arg(long)]
    pub check: bool,

    /// Fail when a journey names something the spec does not have.
    ///
    /// Off by default, because report is the mode a journey is *written* in.
    /// This is the mode a finished one is defended in. Implies `--check`.
    #[arg(long)]
    pub strict: bool,

    /// Print the journey report as JSON and exit. Implies `--check`.
    #[arg(long)]
    pub json: bool,

    /// A directory holding a sealed `manifest.json` and its pictures.
    ///
    /// What a test run *showed* of a journey, beside what the specification
    /// says about it. Written by `allium-journey evidence seal`.
    #[arg(long, value_name = "PATH")]
    pub evidence: Option<PathBuf>,

    /// Where to look for `journey:` markers in source.
    ///
    /// Separate from `--evidence` because the case worth reporting is a test
    /// that claims a step and produced no picture — and in exactly that case
    /// the run wrote nothing, so the claim cannot be derived from the manifest.
    #[arg(long, value_name = "PATH")]
    pub code: Vec<PathBuf>,

    /// The allium binary to run.
    #[arg(long, value_name = "PATH")]
    pub allium: Option<PathBuf>,
}

impl Args {
    /// Whether a browser was asked for, or asked against, or neither.
    ///
    /// `None` is the answer the configuration file gets to fill in, and it is
    /// the reason there is an `--open` at all: without it a file saying
    /// `open = false` could not be overruled for a single run.
    #[must_use]
    pub fn opening(&self) -> Option<bool> {
        either(self.open, self.no_open)
    }

    /// The same, for watching.
    #[must_use]
    pub fn watching(&self) -> Option<bool> {
        either(self.watch, self.no_watch)
    }
}

/// A pair of opposed flags as one answer. Both set is refused by clap.
fn either(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
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
    // journey: SomebodyMeetsASpecTheyDidNotWrite.1 — she points at a directory
    // and the set is what is inside it. A command-line act, so no browser walk
    // can photograph it; this is what claims the step instead.
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
    fn nothing_given_is_nothing_given() {
        // Every one of these has to be distinguishable from a value, because a
        // `.allium-inspect.toml` fills in exactly what the command line did
        // not say. `paths` used to default to `.` here, which made "no specs
        // named" and "specs named as the current directory" the same thing —
        // and a file saying `specs = "specs/"` unheard.
        let args = Args::try_parse_from(["allium-inspect"]).expect("defaults parse");
        assert!(args.paths.is_empty());
        assert_eq!(args.port, None);
        assert_eq!(args.allium, None);
        assert_eq!(args.opening(), None);
        assert_eq!(args.watching(), None);
        assert!(!args.print_graph);
        assert!(!args.no_config);
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
        assert!(args.print_graph);
        assert_eq!(args.opening(), Some(false));
        assert_eq!(args.watching(), Some(false));
        assert_eq!(args.allium, Some(PathBuf::from("/opt/allium")));
    }

    /// The positive halves, which exist so a file saying `open = false` can be
    /// overruled for one run. Without them the file would be the last word.
    #[test]
    fn the_positive_flags_say_the_opposite() {
        let args =
            Args::try_parse_from(["allium-inspect", "--open", "--watch"]).expect("flags parse");
        assert_eq!(args.opening(), Some(true));
        assert_eq!(args.watching(), Some(true));
    }

    /// The five flags that are about journeys no longer demand `--journeys` at
    /// the command line, because a `.allium-inspect.toml` may have said it.
    /// The relationship is checked once both have been heard; here it must
    /// only parse.
    #[test]
    fn a_journey_flag_alone_parses_and_is_answered_later() {
        for flag in ["--check", "--strict", "--json"] {
            assert!(
                Args::try_parse_from(["allium-inspect", flag]).is_ok(),
                "{flag} must reach the merge to be judged"
            );
        }
        assert!(Args::try_parse_from(["allium-inspect", "--evidence", "e"]).is_ok());
        assert!(Args::try_parse_from(["allium-inspect", "--code", "."]).is_ok());
    }

    /// Both at once is refused rather than resolved. Which one wins would be a
    /// rule nobody could guess, and guessing is what this repository does not.
    #[test]
    fn a_flag_and_its_opposite_together_are_refused() {
        assert!(Args::try_parse_from(["allium-inspect", "--open", "--no-open"]).is_err());
        assert!(Args::try_parse_from(["allium-inspect", "--watch", "--no-watch"]).is_err());
        assert!(
            Args::try_parse_from(["allium-inspect", "--config", "a.toml", "--no-config"]).is_err()
        );
    }

    #[test]
    fn nothing_given_is_the_current_directory_by_the_time_it_matters() {
        // The default did not go away; it moved to the one place every other
        // default lives, so that "not given" survives long enough to be heard.
        let args = Args::try_parse_from(["allium-inspect"]).expect("defaults parse");
        let settings = crate::config::Settings::resolve(&args, None);
        assert_eq!(settings.paths, [PathBuf::from(".")]);
        assert_eq!(settings.allium, PathBuf::from("allium"));
        assert!(settings.open && settings.watch);
    }
}
