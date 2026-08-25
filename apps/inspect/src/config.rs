//! A file that says what to run, so nobody has to type it every time.
//!
//! `allium-inspect --journeys specs/journeys --evidence target/evidence --code
//! crates --code ui/src specs/` is the real command for a repository that uses
//! all of this, and nobody types it twice. A `.allium-inspect.toml` beside the
//! specs says it once, and `allium-inspect` on its own means the same thing.
//!
//! Three rules hold it together.
//!
//! **The command line wins.** Every key here has a flag, and a flag that was
//! given beats the file. A configuration file is a default, not an override:
//! the alternative is a person typing `--port 8080`, watching it be ignored,
//! and having no way to see why.
//!
//! **Paths are relative to the file, not to the shell.** `specs = "specs/"`
//! means the directory beside the configuration file, so the command works
//! from anywhere under the project rather than only from its root — which is
//! most of the reason to have the file at all.
//!
//! **A key it does not know is an error.** `journies = "specs/journeys"` is a
//! typo, and a file that shrugs at it is a file that silently does something
//! other than what it says. That is the same failure as a gate that cannot
//! fail, one layer out: the reader believes a thing was configured.
//!
//! What is *not* here is the modes — `--check`, `--strict`, `--json`,
//! `--print-graph`. Each of them answers at the terminal and exits, and a
//! configuration file that quietly turned the browser tool into a JSON printer
//! would be a surprise a reader could not attribute to anything they typed.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::args::Args;

/// The file this looks for.
///
/// Suffixed, though it is a dotfile: an editor highlights it, and the name
/// says what is inside without opening it.
pub const FILE: &str = ".allium-inspect.toml";

/// One path or several, because both spellings are the obvious one.
///
/// `specs = "specs/"` is what a person writes for one directory and
/// `code = ["crates", "ui/src"]` for two, and a file that accepts only the
/// second answers the first with a type error about a sequence.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum Paths {
    One(PathBuf),
    Many(Vec<PathBuf>),
}

impl Paths {
    fn into_vec(self) -> Vec<PathBuf> {
        match self {
            Paths::One(path) => vec![path],
            Paths::Many(paths) => paths,
        }
    }
}

/// A `.allium-inspect.toml`, as written.
///
/// Every field is optional and absence means *nothing said*, which is what
/// lets the command line and the built-in defaults fill in behind it. A `bool`
/// here would make "not mentioned" and "mentioned as false" the same thing.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Spec files, or directories to search for them.
    specs: Option<Paths>,
    /// Journeys to walk against them: a `.journey` file, or a directory.
    journeys: Option<PathBuf>,
    /// A directory holding a sealed `manifest.json` and its pictures.
    evidence: Option<PathBuf>,
    /// Where to look for `journey:` markers in source.
    code: Option<Paths>,
    /// Bind this port instead of an arbitrary free one.
    port: Option<u16>,
    /// Open a browser. Written positively, because a file is read rather than
    /// typed and `no_open = false` is a sentence nobody should have to parse.
    open: Option<bool>,
    /// Reload when a spec file changes.
    watch: Option<bool>,
    /// The allium binary to run for `model` and `plan`.
    allium: Option<PathBuf>,
}

impl Config {
    /// Read one, naming the file in anything that goes wrong.
    ///
    /// A configuration file is read before anything else happens, so its errors
    /// are the first thing a person sees and are worth spelling out: the path,
    /// and whatever `toml` says about the line.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let mut config: Self = toml::from_str(&text)
            .map_err(|error| format!("{} is not valid: {error}", path.display()))?;

        // Relative to the file rather than to the shell, which is what makes
        // the command work from a subdirectory.
        let dir = path.parent().unwrap_or(Path::new("."));
        config.rebase(dir);
        Ok(config)
    }

    /// Resolve every path against the directory the file was found in.
    fn rebase(&mut self, dir: &Path) {
        let beside = |path: &PathBuf| -> PathBuf {
            if path.is_absolute() { path.clone() } else { dir.join(path) }
        };
        self.specs = self
            .specs
            .take()
            .map(|paths| Paths::Many(paths.into_vec().iter().map(&beside).collect()));
        self.code = self
            .code
            .take()
            .map(|paths| Paths::Many(paths.into_vec().iter().map(&beside).collect()));
        self.journeys = self.journeys.as_ref().map(&beside);
        self.evidence = self.evidence.as_ref().map(&beside);
    }
}

/// Find the file, starting `from` and going up as far as the repository root.
///
/// **As far as, and no further.** A configuration file is a fact about a
/// project; one sitting in `$HOME` that quietly governed every directory below
/// it would change what a command does for reasons its reader cannot see, and
/// no message would ever mention the file they do not know exists. So the
/// search is bounded by the thing that says *this is one project*: the
/// directory holding `.git` is the last one asked, and outside a repository
/// only the directory the command was run from is.
pub fn discover(from: &Path) -> Result<Option<(PathBuf, Config)>, String> {
    let last = repository_root(from).unwrap_or_else(|| from.to_owned());

    let mut here = from.to_owned();
    loop {
        let candidate = here.join(FILE);
        if candidate.is_file() {
            return Config::read(&candidate).map(|config| Some((candidate, config)));
        }
        if here == last {
            return Ok(None);
        }
        match here.parent() {
            Some(up) => here = up.to_owned(),
            None => return Ok(None),
        }
    }
}

/// The nearest directory at or above `from` that holds a `.git`.
fn repository_root(from: &Path) -> Option<PathBuf> {
    let mut here = from;
    loop {
        if here.join(".git").exists() {
            return Some(here.to_owned());
        }
        here = here.parent()?;
    }
}

/// What the run needs, after the file and the command line have met.
///
/// A separate type rather than a mutated `Args`, because the two are different
/// shapes: on the command line `--no-open` and silence are distinguishable and
/// have to be, and by the time anything runs there is only *open a browser, or
/// do not*. Collapsing that in one named place is what keeps the rest of the
/// binary from asking twice.
#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    pub paths: Vec<PathBuf>,
    pub journeys: Option<PathBuf>,
    pub evidence: Option<PathBuf>,
    pub code: Vec<PathBuf>,
    pub port: Option<u16>,
    pub open: bool,
    pub watch: bool,
    pub allium: PathBuf,
    /// The file the defaults came from, when one was read. Printed, so that a
    /// run behaving unexpectedly names the thing that told it to.
    pub config: Option<PathBuf>,
}

impl Settings {
    /// The command line first, then the file, then the built-in defaults.
    #[must_use]
    pub fn resolve(args: &Args, found: Option<(PathBuf, Config)>) -> Self {
        let (path, config) = match found {
            Some((path, config)) => (Some(path), config),
            None => (None, Config::default()),
        };

        let from_file = |paths: Option<Paths>| paths.map(Paths::into_vec).unwrap_or_default();
        let specs = from_file(config.specs);
        let code = from_file(config.code);

        Settings {
            paths: if args.paths.is_empty() {
                if specs.is_empty() { vec![PathBuf::from(".")] } else { specs }
            } else {
                args.paths.clone()
            },
            journeys: args.journeys.clone().or(config.journeys),
            evidence: args.evidence.clone().or(config.evidence),
            code: if args.code.is_empty() { code } else { args.code.clone() },
            port: args.port.or(config.port),
            open: args.opening().or(config.open).unwrap_or(true),
            watch: args.watching().or(config.watch).unwrap_or(true),
            allium: args
                .allium
                .clone()
                .or(config.allium)
                .unwrap_or_else(|| PathBuf::from("allium")),
            config: path,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::Parser;

    use super::*;

    /// A scratch directory, emptied first so a previous run cannot pass a test.
    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("allium-inspect-config-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn write(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join(FILE);
        fs::write(&path, text).expect("write");
        path
    }

    fn args(from: &[&str]) -> Args {
        let mut whole = vec!["allium-inspect"];
        whole.extend_from_slice(from);
        Args::try_parse_from(whole).expect("the arguments parse")
    }

    // --- reading --------------------------------------------------------

    #[test]
    fn a_file_says_what_the_flags_would_have() {
        let dir = tree("full");
        let path = write(
            &dir,
            "specs = \"specs/\"\n\
             journeys = \"specs/journeys\"\n\
             evidence = \"target/evidence\"\n\
             code = [\"crates\", \"ui/src\"]\n\
             port = 7777\n\
             open = false\n\
             watch = false\n\
             allium = \"/opt/allium\"\n",
        );

        let settings = Settings::resolve(
            &args(&[]),
            Some((path.clone(), Config::read(&path).expect("reads"))),
        );
        assert_eq!(settings.paths, [dir.join("specs/")]);
        assert_eq!(settings.journeys, Some(dir.join("specs/journeys")));
        assert_eq!(settings.evidence, Some(dir.join("target/evidence")));
        assert_eq!(settings.code, [dir.join("crates"), dir.join("ui/src")]);
        assert_eq!(settings.port, Some(7777));
        assert!(!settings.open && !settings.watch);
        assert_eq!(settings.allium, PathBuf::from("/opt/allium"));
        assert_eq!(settings.config, Some(path));
        let _ = fs::remove_dir_all(dir);
    }

    /// One path or several. A file that took only the list would answer the
    /// spelling everybody reaches for first with a type error about sequences.
    #[test]
    fn one_path_and_a_list_of_them_both_read() {
        let dir = tree("shapes");
        let one = Config::read(&write(&dir, "code = \"crates\"\n")).expect("reads");
        let many = Config::read(&write(&dir, "code = [\"crates\"]\n")).expect("reads");
        assert_eq!(one, many);
        let _ = fs::remove_dir_all(dir);
    }

    /// Relative to the file, not to the shell. This is most of the reason to
    /// have the file: the command means the same thing from a subdirectory.
    #[test]
    fn a_relative_path_is_beside_the_file() {
        let dir = tree("relative");
        let config = Config::read(&write(&dir, "specs = \"specs/\"\n")).expect("reads");
        let settings = Settings::resolve(&args(&[]), Some((dir.join(FILE), config)));
        assert_eq!(settings.paths, [dir.join("specs/")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_absolute_path_is_left_alone() {
        let dir = tree("absolute");
        let config = Config::read(&write(&dir, "specs = \"/opt/specs\"\n")).expect("reads");
        let settings = Settings::resolve(&args(&[]), Some((dir.join(FILE), config)));
        assert_eq!(settings.paths, [PathBuf::from("/opt/specs")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_file_says_nothing_and_is_not_an_error() {
        let dir = tree("empty");
        assert_eq!(Config::read(&write(&dir, "")).expect("reads"), Config::default());
        let _ = fs::remove_dir_all(dir);
    }

    // --- and the ways it refuses ----------------------------------------

    /// A typo that is quietly ignored is a file that does something other than
    /// what it says, and a reader who believes it was configured.
    #[test]
    fn a_key_it_does_not_know_is_refused_by_name() {
        let dir = tree("typo");
        let error =
            Config::read(&write(&dir, "journies = \"specs/journeys\"\n")).expect_err("refused");
        assert!(error.contains("journies"), "{error}");
        assert!(error.contains(FILE), "the file is named: {error}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_not_toml_is_refused_by_name() {
        let dir = tree("garbage");
        let error = Config::read(&write(&dir, "specs = [\n")).expect_err("refused");
        assert!(error.contains(FILE), "{error}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_refused() {
        let dir = tree("wrong-kind");
        assert!(Config::read(&write(&dir, "port = \"seven\"\n")).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_not_there_is_refused_by_name() {
        let error =
            Config::read(Path::new("/nowhere/at/all/.allium-inspect.toml")).expect_err("refused");
        assert!(error.contains("/nowhere/at/all"), "{error}");
    }

    // --- who wins -------------------------------------------------------

    #[test]
    fn the_command_line_beats_the_file() {
        let dir = tree("precedence");
        let config = Config::read(&write(
            &dir,
            "specs = \"specs/\"\nport = 7777\nopen = true\nallium = \"/opt/allium\"\n",
        ))
        .expect("reads");

        let settings = Settings::resolve(
            &args(&["elsewhere/", "--port", "8080", "--no-open", "--allium", "/usr/bin/allium"]),
            Some((dir.join(FILE), config)),
        );
        assert_eq!(settings.paths, [PathBuf::from("elsewhere/")]);
        assert_eq!(settings.port, Some(8080));
        assert!(!settings.open);
        assert_eq!(settings.allium, PathBuf::from("/usr/bin/allium"));
        let _ = fs::remove_dir_all(dir);
    }

    /// And in the other direction, which is what makes the flag worth having:
    /// a file that turned the browser off has to be overrulable for one run.
    #[test]
    fn the_positive_flag_overrules_a_file_that_said_no() {
        let dir = tree("overrule");
        let config = Config::read(&write(&dir, "open = false\nwatch = false\n")).expect("reads");
        let settings =
            Settings::resolve(&args(&["--open", "--watch"]), Some((dir.join(FILE), config)));
        assert!(settings.open && settings.watch);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_key_the_command_line_did_not_mention_comes_from_the_file() {
        let dir = tree("partial");
        let config = Config::read(&write(&dir, "journeys = \"specs/journeys\"\nport = 7777\n"))
            .expect("reads");
        // Specs on the command line, journeys from the file: the keys are
        // independent, which is what a default is.
        let settings = Settings::resolve(&args(&["elsewhere/"]), Some((dir.join(FILE), config)));
        assert_eq!(settings.paths, [PathBuf::from("elsewhere/")]);
        assert_eq!(settings.journeys, Some(dir.join("specs/journeys")));
        assert_eq!(settings.port, Some(7777));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_file_at_all_leaves_the_built_in_defaults() {
        let settings = Settings::resolve(&args(&[]), None);
        assert_eq!(settings.paths, [PathBuf::from(".")]);
        assert_eq!(settings.journeys, None);
        assert_eq!(settings.port, None);
        assert!(settings.open && settings.watch);
        assert_eq!(settings.allium, PathBuf::from("allium"));
        assert_eq!(settings.config, None);
    }

    // --- finding it -----------------------------------------------------

    #[test]
    fn a_file_beside_the_command_is_found() {
        let dir = tree("here");
        write(&dir, "port = 7777\n");
        let (path, config) = discover(&dir).expect("no error").expect("found");
        assert_eq!(path, dir.join(FILE));
        assert_eq!(config.port, Some(7777));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_at_the_repository_root_is_found_from_a_subdirectory() {
        let dir = tree("upward");
        fs::create_dir_all(dir.join(".git")).expect("a repository");
        fs::create_dir_all(dir.join("crates/deep")).expect("subdirectories");
        write(&dir, "port = 7777\n");

        let (path, _) = discover(&dir.join("crates/deep")).expect("no error").expect("found");
        assert_eq!(path, dir.join(FILE));
        let _ = fs::remove_dir_all(dir);
    }

    /// The nearer one, because it is the more specific statement about where
    /// the command is being run.
    #[test]
    fn the_nearest_file_wins() {
        let dir = tree("nearest");
        fs::create_dir_all(dir.join(".git")).expect("a repository");
        let inner = dir.join("crates");
        fs::create_dir_all(&inner).expect("subdirectory");
        write(&dir, "port = 7777\n");
        write(&inner, "port = 8888\n");

        let (path, config) = discover(&inner).expect("no error").expect("found");
        assert_eq!(path, inner.join(FILE));
        assert_eq!(config.port, Some(8888));
        let _ = fs::remove_dir_all(dir);
    }

    /// The bound that makes the search explicable. A file above the repository
    /// — in `$HOME`, say — would change what the command does for a reason its
    /// reader cannot see, and no message would name a file they do not know
    /// exists.
    #[test]
    fn the_search_stops_at_the_repository_root() {
        let dir = tree("bounded");
        write(&dir, "port = 7777\n");
        let repository = dir.join("project");
        fs::create_dir_all(repository.join(".git")).expect("a repository");

        assert!(discover(&repository).expect("no error").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    /// And outside a repository the search is one directory deep, for the same
    /// reason: there is nothing to say where the project stops.
    #[test]
    fn outside_a_repository_only_this_directory_is_asked() {
        let dir = tree("loose");
        write(&dir, "port = 7777\n");
        let inner = dir.join("inner");
        fs::create_dir_all(&inner).expect("subdirectory");

        assert!(discover(&inner).expect("no error").is_none());
        assert!(discover(&dir).expect("no error").is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn finding_nothing_is_not_an_error() {
        let dir = tree("nothing");
        fs::create_dir_all(dir.join(".git")).expect("a repository");
        assert!(discover(&dir).expect("no error").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    /// Found and unreadable is different from not found. The file exists and
    /// says something wrong, and serving as though it were absent would be the
    /// tool deciding to ignore it.
    #[test]
    fn a_file_that_is_found_and_broken_stops_the_run() {
        let dir = tree("broken");
        write(&dir, "specs = [\n");
        assert!(discover(&dir).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
