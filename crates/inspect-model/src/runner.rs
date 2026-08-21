//! Reaching the `allium` CLI, behind a trait so nothing else has to.
//!
//! Ingestion needs four JSON documents per spec file. Where they come from is
//! the only impure thing in this crate, so it is the only thing behind a trait:
//! [`ProcessRunner`] shells out, [`MapRunner`] replays recorded fixtures, and
//! everything downstream is a pure function of the documents either produces.
//!
//! That seam is what lets the ingestion and projection tests run with no
//! `allium` binary installed, deterministically, in a fraction of a second.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use serde_json::Value;

/// The `allium` subcommands this tool still has to launch a process for.
///
/// Two, not four. `parse` and `analyse` are `allium_parser` function calls —
/// see [`mod@crate::ingest`] — which leaves the ones allium builds in its binary
/// crate: `crates/allium` declares only a `[[bin]]` target, so `model` and
/// `plan` are not importable at any price.
///
/// `check` is deliberately absent for a different reason: its diagnostics are a
/// subset of what the others already carry, so running it would be a process
/// launch for information we would then have to de-duplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Command {
    /// Entities, fields, relationships, transition graphs, enums, config.
    Model,
    /// Test obligations, including each rule's trigger and entity dependencies.
    Plan,
}

impl Command {
    /// Every command, in the order ingestion runs them.
    pub const ALL: [Command; 2] = [Command::Model, Command::Plan];

    /// The subcommand name as the CLI spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Command::Model => "model",
            Command::Plan => "plan",
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a command could not be turned into a JSON document.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The binary is not on `PATH`.
    #[error(
        "the `allium` CLI was not found on PATH.\n\
         allium-inspect drives the real CLI rather than reimplementing its parser, so it \
         needs one installed.\n\
         Install it from https://github.com/juxt/allium-tools, or pass --allium <path>."
    )]
    NotFound,

    /// The process could not be spawned or did not run to completion.
    #[error("could not run `{command}` on {}: {source}", path.display())]
    Spawn {
        command: Command,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The process ran but printed something that is not JSON.
    #[error(
        "`allium {command}` on {} printed output that is not JSON.\n\
         {detail}\n\
         This usually means the installed CLI is a different major version than this tool \
         expects.",
        path.display()
    )]
    NotJson { command: Command, path: PathBuf, detail: String },

    /// The process failed and printed nothing usable.
    #[error("`allium {command}` on {} failed ({status}): {stderr}", path.display())]
    Failed { command: Command, path: PathBuf, status: String, stderr: String },

    /// A fixture runner was asked for a document it does not hold.
    #[error("no recorded output for `allium {command}` on {}", path.display())]
    NoFixture { command: Command, path: PathBuf },
}

/// Produces the JSON document for one `allium` subcommand over one spec file.
pub trait AlliumRunner {
    /// Run `command` against `path` and return the JSON it printed.
    ///
    /// # Errors
    ///
    /// Returns [`RunError`] when the binary is missing, the process cannot run,
    /// or the output is not JSON.
    fn run(&self, command: Command, path: &Path) -> Result<Value, RunError>;

    /// The CLI version string, for stamping fixtures and reporting mismatches.
    ///
    /// # Errors
    ///
    /// Returns [`RunError`] when the version cannot be determined.
    fn version(&self) -> Result<String, RunError>;
}

/// Runs the real `allium` binary.
#[derive(Debug, Clone)]
pub struct ProcessRunner {
    binary: PathBuf,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        Self::new("allium")
    }
}

impl ProcessRunner {
    /// A runner invoking `binary`, which may be a bare name resolved on `PATH`.
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self { binary: binary.into() }
    }

    fn invoke(&self, args: &[&str]) -> Result<std::process::Output, std::io::Error> {
        ProcessCommand::new(&self.binary).args(args).output()
    }
}

impl AlliumRunner for ProcessRunner {
    fn run(&self, command: Command, path: &Path) -> Result<Value, RunError> {
        let path_arg = path.to_string_lossy().into_owned();
        let output = self.invoke(&[command.as_str(), &path_arg]).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                RunError::NotFound
            } else {
                RunError::Spawn { command, path: path.to_path_buf(), source }
            }
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parsed before the exit status is consulted, not after. A non-zero
        // exit from these commands means "the spec has a problem", and the
        // document describing that problem is on stdout — discarding it would
        // blank the screen exactly when there is something to show.
        match serde_json::from_str::<Value>(&stdout) {
            Ok(value) => Ok(value),
            Err(parse_error) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                if output.status.success() {
                    Err(RunError::NotJson {
                        command,
                        path: path.to_path_buf(),
                        detail: parse_error.to_string(),
                    })
                } else {
                    Err(RunError::Failed {
                        command,
                        path: path.to_path_buf(),
                        status: output.status.to_string(),
                        stderr: if stderr.is_empty() { stdout.trim().to_owned() } else { stderr },
                    })
                }
            }
        }
    }

    fn version(&self) -> Result<String, RunError> {
        let output = self.invoke(&["--version"]).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                RunError::NotFound
            } else {
                RunError::Spawn { command: Command::Model, path: self.binary.clone(), source }
            }
        })?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if text.is_empty() {
            return Err(RunError::Failed {
                command: Command::Model,
                path: self.binary.clone(),
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(text)
    }
}

/// Replays recorded documents, keyed by spec file and command.
///
/// This is how every ingestion and projection test runs: real CLI output,
/// recorded once, replayed deterministically with no process launch.
#[derive(Debug, Clone, Default)]
pub struct MapRunner {
    documents: BTreeMap<(PathBuf, Command), Value>,
    version: String,
}

impl MapRunner {
    /// An empty runner reporting `version`.
    pub fn new(version: impl Into<String>) -> Self {
        Self { documents: BTreeMap::new(), version: version.into() }
    }

    /// Record `value` as the output of `command` over `path`.
    #[must_use]
    pub fn with(mut self, command: Command, path: impl Into<PathBuf>, value: Value) -> Self {
        self.documents.insert((path.into(), command), value);
        self
    }

    /// The spec paths this runner holds documents for, in sorted order.
    #[must_use]
    pub fn paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.documents.keys().map(|(path, _)| path.clone()).collect();
        paths.dedup();
        paths
    }
}

impl AlliumRunner for MapRunner {
    fn run(&self, command: Command, path: &Path) -> Result<Value, RunError> {
        self.documents
            .get(&(path.to_path_buf(), command))
            .cloned()
            .ok_or_else(|| RunError::NoFixture { command, path: path.to_path_buf() })
    }

    fn version(&self) -> Result<String, RunError> {
        Ok(self.version.clone())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_command_names_its_subcommand() {
        // Two, not four. `parse` and `analyse` are library calls; these are
        // the ones allium builds in a crate with no library target.
        let names: Vec<&str> = Command::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names, ["model", "plan"]);
    }

    #[test]
    fn command_displays_as_its_subcommand_name() {
        assert_eq!(Command::Plan.to_string(), "plan");
    }

    #[test]
    fn map_runner_replays_what_it_was_given() {
        let runner = MapRunner::new("allium 3.5.3").with(
            Command::Model,
            "specs/a.allium",
            json!({"entities": []}),
        );
        let value = runner.run(Command::Model, Path::new("specs/a.allium")).expect("recorded");
        assert_eq!(value, json!({"entities": []}));
    }

    #[test]
    fn map_runner_reports_a_missing_document_rather_than_an_empty_one() {
        // An absent fixture must be an error, not `{}`. A silently empty
        // document would look like a spec with no entities, and a test asserting
        // "no entities" would pass against a fixture that was never recorded.
        let runner = MapRunner::new("allium 3.5.3");
        let error = runner.run(Command::Plan, Path::new("specs/missing.allium")).unwrap_err();
        assert!(matches!(error, RunError::NoFixture { command: Command::Plan, .. }));
        assert!(error.to_string().contains("specs/missing.allium"));
    }

    #[test]
    fn map_runner_distinguishes_commands_for_the_same_path() {
        let runner = MapRunner::new("v")
            .with(Command::Model, "a.allium", json!({"which": "model"}))
            .with(Command::Plan, "a.allium", json!({"which": "plan"}));
        assert_eq!(
            runner.run(Command::Plan, Path::new("a.allium")).expect("recorded"),
            json!({"which": "plan"})
        );
    }

    #[test]
    fn map_runner_reports_its_version() {
        assert_eq!(MapRunner::new("allium 3.5.3").version().expect("set"), "allium 3.5.3");
    }

    #[test]
    fn map_runner_lists_its_paths() {
        let runner = MapRunner::new("v")
            .with(Command::Model, "b.allium", json!({}))
            .with(Command::Model, "a.allium", json!({}))
            .with(Command::Model, "a.allium", json!({}));
        assert_eq!(runner.paths(), [PathBuf::from("a.allium"), PathBuf::from("b.allium")]);
    }

    #[test]
    fn a_missing_binary_is_reported_as_not_found() {
        // The message has to name the fix. "No such file or directory (os
        // error 2)" is what the OS says and it tells the user nothing about
        // what to install.
        let runner = ProcessRunner::new("allium-does-not-exist-anywhere");
        let error = runner.run(Command::Model, Path::new("a.allium")).unwrap_err();
        assert!(matches!(error, RunError::NotFound));
        let message = error.to_string();
        assert!(message.contains("was not found on PATH"), "{message}");
        assert!(message.contains("allium-tools"), "the message must name where to get it");
    }

    #[test]
    fn a_missing_binary_is_not_found_when_asked_for_its_version_too() {
        let runner = ProcessRunner::new("allium-does-not-exist-anywhere");
        assert!(matches!(runner.version().unwrap_err(), RunError::NotFound));
    }

    #[test]
    fn non_json_output_from_a_successful_run_is_reported_as_such() {
        // `true` succeeds and prints nothing, which stands in for a CLI whose
        // output shape we no longer understand.
        let runner = ProcessRunner::new("true");
        let error = runner.run(Command::Model, Path::new("a.allium")).unwrap_err();
        assert!(matches!(error, RunError::NotJson { .. }), "{error}");
        assert!(error.to_string().contains("different major version"));
    }

    #[test]
    fn a_failing_run_with_no_json_is_reported_with_its_status() {
        let runner = ProcessRunner::new("false");
        let error = runner.run(Command::Model, Path::new("a.allium")).unwrap_err();
        assert!(matches!(error, RunError::Failed { command: Command::Model, .. }), "{error}");
    }

    /// Write an executable stand-in for the CLI that prints `stdout` and exits
    /// `code`, and return a runner pointed at it.
    ///
    /// A real process, because that is the only way to exercise the exit-status
    /// handling this runner exists for. `sh -c` cannot stand in: `run` appends
    /// the subcommand and the path as arguments, so the script has to be the
    /// binary rather than an argument to one.
    fn stub_cli(name: &str, stdout: &str, code: i32) -> (ProcessRunner, PathBuf) {
        use std::{fs, os::unix::fs::PermissionsExt};

        let dir = std::env::temp_dir().join(format!("allium-inspect-test-{name}"));
        fs::create_dir_all(&dir).expect("scratch dir");
        let script = dir.join("allium-stub");
        fs::write(&script, format!("#!/bin/sh\nprintf '%s' '{stdout}'\nexit {code}\n"))
            .expect("write stub");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod stub");
        (ProcessRunner::new(&script), dir)
    }

    #[test]
    fn json_on_stdout_is_accepted_even_when_the_process_fails() {
        // The behaviour the whole tool depends on. A spec carrying an
        // error-severity diagnostic makes the CLI exit non-zero *and* print the
        // document describing it; `analyse` exits 1 whenever it finds anything
        // at all. Checking the exit status before parsing would discard both,
        // blanking the UI on exactly the specs worth inspecting.
        let (runner, dir) = stub_cli("fails-with-json", r#"{"diagnostics":[1]}"#, 1);
        let value = runner
            .run(Command::Plan, Path::new("a.allium"))
            .expect("JSON on stdout is usable regardless of the exit status");
        assert_eq!(value["diagnostics"], json!([1]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_successful_run_returns_its_document() {
        let (runner, dir) = stub_cli("succeeds", r#"{"entities":[]}"#, 0);
        let value = runner.run(Command::Model, Path::new("a.allium")).expect("clean run");
        assert_eq!(value, json!({"entities": []}));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failing_run_reports_stderr_rather_than_the_raw_status_alone() {
        let (runner, dir) = stub_cli("fails-plain", "not json at all", 2);
        let error = runner.run(Command::Plan, Path::new("a.allium")).unwrap_err();
        match error {
            RunError::Failed { stderr, .. } => {
                assert!(stderr.contains("not json"), "the output must survive into the error");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn version_reports_what_the_binary_printed() {
        let runner = ProcessRunner::new("echo");
        let version = runner.version().expect("echo succeeds");
        assert_eq!(version, "--version");
    }

    #[test]
    fn version_that_prints_nothing_is_a_failure_not_an_empty_string() {
        let runner = ProcessRunner::new("true");
        assert!(matches!(runner.version().unwrap_err(), RunError::Failed { .. }));
    }

    #[test]
    fn the_default_runner_invokes_the_bare_name() {
        assert_eq!(ProcessRunner::default().binary, PathBuf::from("allium"));
    }
}
