//! Answering for each journey file, behind the same seam as everything else.
//!
//! The runner is a parameter and the output is a writer, so what this does can
//! be driven from a test with recorded CLI output and no `allium` installed —
//! which is the arrangement the rest of this repository already uses, and the
//! reason its ingestion tests take milliseconds.
//!
//! Nothing here reaches the process or the terminal by itself. `main` supplies
//! both.

use std::{collections::BTreeMap, io::Write, path::Path};

use inspect_journey::{Verdict, Walk};
use inspect_model::{
    AlliumRunner, FileReader, Ingestion, Program, SourceReader, SpecGraph, ingest, module_name,
};
use inspect_sim::step::Sources;

use crate::{
    args::{Command, Run},
    emit,
    resolve::Found,
};

/// Nothing to say.
pub const CLEAN: u8 = 0;
/// Something was reported.
pub const REPORTED: u8 = 1;
/// Nothing to read: no paths, or none that resolved.
pub const UNUSABLE: u8 = 2;

/// Ingest the specs, then answer for each journey file in turn.
///
/// # Errors
///
/// Returns a message when the spec set cannot be ingested or the report cannot
/// be serialised.
pub fn all<R: AlliumRunner, W: Write>(
    runner: &R,
    command: &Command,
    run: &Run,
    found: &Found,
    out: &mut W,
) -> Result<u8, String> {
    let Ingestion { graph, program } =
        ingest(runner, &FileReader, &found.specs).map_err(|error| error.to_string())?;
    let sources = read_sources(&found.specs);

    let mut worst = CLEAN;
    for file in &found.journeys {
        let name = file.to_string_lossy().into_owned();
        let (walks, error) = read_one(command, file, &graph, &program, &sources);

        let written = if run.text {
            text_report(&name, &walks, error.as_deref())
        } else {
            let document = emit::document(command.as_str(), &name, &walks, error.as_deref());
            format!(
                "{}\n",
                serde_json::to_string_pretty(&document)
                    .map_err(|error| format!("could not serialise the report: {error}"))?
            )
        };
        out.write_all(written.as_bytes()).map_err(|error| format!("could not write: {error}"))?;
        worst = worst.max(code_for(&walks, error.is_some(), run.report));
    }
    Ok(worst)
}

/// One journey file: its walks, or why it could not be read.
fn read_one(
    command: &Command,
    file: &Path,
    graph: &SpecGraph,
    program: &Program,
    sources: &Sources,
) -> (Vec<Walk>, Option<String>) {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => return (Vec::new(), Some(format!("could not be read: {error}"))),
    };
    let journeys = match inspect_journey::parse(&text) {
        Ok(journeys) => journeys,
        Err(error) => return (Vec::new(), Some(error.to_string())),
    };

    let walks = journeys
        .iter()
        .map(|journey| match command {
            Command::Walk(_) => inspect_journey::walk(journey, graph, program, sources),
            // Static only: what can be answered from the graph, without a
            // world. The checker's notes become the outcomes, so the document
            // has the same shape either way and a caller reads one parser.
            Command::Check(_) => statically(journey, graph),
        })
        .collect();
    (walks, None)
}

/// A journey checked against the graph and not run.
fn statically(journey: &inspect_journey::Journey, graph: &SpecGraph) -> Walk {
    let notes = inspect_journey::check(journey, graph);
    let steps = journey
        .steps
        .iter()
        .map(|step| {
            let lines: Vec<usize> =
                step.clauses.iter().map(inspect_journey::Clause::line).collect();
            inspect_journey::Walked {
                number: step.number,
                title: step.title.clone(),
                world: inspect_sim::World::new(),
                outcomes: notes
                    .iter()
                    .filter(|note| lines.contains(&note.line))
                    .map(|note| inspect_journey::Outcome {
                        line: note.line,
                        verdict: note.verdict,
                        about: note.message.clone(),
                        detail: None,
                    })
                    .collect(),
            }
        })
        .collect();
    Walk {
        name: journey.name.clone(),
        cast: Vec::new(),
        goal: journey.goal.clone(),
        ends: journey.ends.clone(),
        line: journey.line,
        steps,
        stipulated: Vec::new(),
    }
}

/// The exit code one file earns.
#[must_use]
pub fn code_for(walks: &[Walk], unreadable: bool, report: bool) -> u8 {
    if unreadable {
        // A fault in the journey rather than a gap in the spec, so `--report`
        // does not excuse it: there is nothing to report *from*.
        return REPORTED;
    }
    if report {
        return CLEAN;
    }
    let reported = walks.iter().any(|walk| walk.verdict() != Verdict::Specified);
    if reported { REPORTED } else { CLEAN }
}

/// The text report, for a person rather than for a pipe.
fn text_report(file: &str, walks: &[Walk], error: Option<&str>) -> String {
    match error {
        Some(message) => format!("{file}\n  could not be read — {message}\n"),
        None => format!("{file}\n{}", inspect_journey::render(walks)),
    }
}

/// Each spec's text, which the simulator quotes from when it cannot decide.
fn read_sources(paths: &[std::path::PathBuf]) -> Sources {
    let mut sources: Sources = BTreeMap::new();
    for path in paths {
        if let Ok(text) = FileReader.read(path) {
            sources.insert(module_name(path), text);
        }
    }
    sources
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use inspect_model::{Command as CliCommand, runner::MapRunner};

    use super::*;
    use crate::args::Run as Options;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/inspect-model/tests/fixtures")
    }

    fn journeys() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/inspect-journey/tests/fixtures")
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
    }

    /// The two commands allium still has to be run for, replayed.
    ///
    /// The whole point of the seam: this drives the command end to end with no
    /// `allium` on PATH, in milliseconds, against the same recordings every
    /// other test in the repository uses.
    fn recorded() -> (MapRunner, Vec<PathBuf>) {
        let root = fixtures();
        let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
        let mut specs = Vec::new();
        for module in ["catalogue", "lending"] {
            let path = root.join(format!("specs/{module}.allium"));
            for command in CliCommand::ALL {
                let document = read(&root.join(format!("cli/{module}.{command}.json")));
                runner =
                    runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
            }
            specs.push(path);
        }
        (runner, specs)
    }

    fn options(report: bool, text: bool) -> Options {
        Options { paths: Vec::new(), report, text, allium: PathBuf::from("not-on-path") }
    }

    /// Run `command` over one journey file, returning what it printed and the
    /// code it earned.
    fn run_over(command: &Command, journey: &str, report: bool, text: bool) -> (String, u8) {
        let (runner, specs) = recorded();
        let found = Found { specs, journeys: vec![journeys().join(journey)] };
        let mut out = Vec::new();
        let code = all(&runner, command, &options(report, text), &found, &mut out)
            .unwrap_or_else(|error| panic!("{error}"));
        (String::from_utf8(out).expect("UTF-8"), code)
    }

    fn walk_command() -> Command {
        Command::Walk(options(false, false))
    }

    fn check_command() -> Command {
        Command::Check(options(false, false))
    }

    #[test]
    fn a_journey_the_spec_supports_end_to_end_says_nothing_and_exits_clean() {
        // The whole contract in one case: exit 0 means there is nothing to do.
        let (printed, code) = run_over(&walk_command(), "reservations.journey", false, false);
        assert_eq!(code, CLEAN, "{printed}");
        let document: serde_json::Value = serde_json::from_str(&printed).expect("JSON");
        assert_eq!(document["diagnostics"].as_array().expect("an array").len(), 0);
        assert_eq!(document["findings"][0]["verdict"], "specified");
    }

    #[test]
    fn a_journey_with_a_gap_is_reported_and_exits_one() {
        let (printed, code) = run_over(&walk_command(), "forms.journey", false, false);
        assert_eq!(code, REPORTED);
        let document: serde_json::Value = serde_json::from_str(&printed).expect("JSON");
        assert!(!document["diagnostics"].as_array().expect("an array").is_empty());
    }

    #[test]
    fn report_mode_prints_the_same_thing_and_exits_clean() {
        // The mode a journey is *written* in. What it changes is the exit
        // code and nothing else — a gate that hid the gaps as well would be
        // useless for the loop it exists to serve.
        let (loud, _) = run_over(&walk_command(), "forms.journey", false, false);
        let (quiet, code) = run_over(&walk_command(), "forms.journey", true, false);
        assert_eq!(code, CLEAN);
        assert_eq!(loud, quiet);
    }

    #[test]
    fn one_document_is_printed_for_each_journey_file() {
        // Streamed rather than wrapped in an array, which is what
        // `allium analyse specs/` prints.
        let (runner, specs) = recorded();
        let found = Found {
            specs,
            journeys: vec![
                journeys().join("loss.journey"),
                journeys().join("reservations.journey"),
            ],
        };
        let mut out = Vec::new();
        all(&runner, &walk_command(), &options(false, false), &found, &mut out).expect("runs");
        let printed = String::from_utf8(out).expect("UTF-8");
        let documents: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&printed)
            .into_iter::<serde_json::Value>()
            .map(|document| document.expect("JSON"))
            .collect();
        assert_eq!(documents.len(), 2);
        assert!(documents[0]["spec_file"].as_str().is_some_and(|f| f.ends_with("loss.journey")));
        assert_eq!(documents[0]["command"], "walk");
    }

    #[test]
    fn check_answers_without_running_anything() {
        // The static half. It reaches a different verdict from a walk on the
        // same file — the walk finds what the world could not settle, and this
        // finds only what the graph does not have.
        let (printed, code) = run_over(&check_command(), "undecided.journey", false, false);
        let document: serde_json::Value = serde_json::from_str(&printed).expect("JSON");
        assert_eq!(document["command"], "check");
        assert_eq!(code, CLEAN, "the graph has everything this journey names");

        let (walked, walk_code) = run_over(&walk_command(), "undecided.journey", false, false);
        assert_eq!(walk_code, REPORTED, "but the world could not settle it");
        assert_ne!(printed, walked);
    }

    #[test]
    fn a_journey_that_cannot_be_read_is_an_error_that_report_mode_does_not_excuse() {
        // A fault in the journey rather than a gap in the spec: there is
        // nothing to report *from*.
        let (runner, specs) = recorded();
        let missing = journeys().join("no-such-file.journey");
        let found = Found { specs, journeys: vec![missing] };
        for report in [false, true] {
            let mut out = Vec::new();
            let code = all(&runner, &walk_command(), &options(report, false), &found, &mut out)
                .expect("runs");
            assert_eq!(code, REPORTED, "report={report}");
            let document: serde_json::Value =
                serde_json::from_str(&String::from_utf8(out).expect("UTF-8")).expect("JSON");
            assert_eq!(document["diagnostics"][0]["severity"], "error");
        }
    }

    #[test]
    fn the_text_report_is_for_a_person_and_names_the_file() {
        let (printed, _) = run_over(&walk_command(), "forms.journey", false, true);
        assert!(printed.contains("forms.journey"), "{printed}");
        assert!(printed.contains("steps hold"), "{printed}");
        // Not JSON — checked by parsing rather than by looking for a brace,
        // because a report can legitimately contain one: `Reservations is {}`.
        assert!(
            serde_json::from_str::<serde_json::Value>(&printed).is_err(),
            "and is not JSON: {printed}"
        );
    }

    #[test]
    fn the_worst_file_decides_the_exit_code() {
        // One holding journey beside one with a gap is not a clean run: a gate
        // that reported the last file rather than the worst would pass
        // whenever the gaps happened to come first.
        let (runner, specs) = recorded();
        let found = Found {
            specs,
            journeys: vec![
                journeys().join("forms.journey"),
                journeys().join("reservations.journey"),
            ],
        };
        let mut out = Vec::new();
        let code =
            all(&runner, &walk_command(), &options(false, false), &found, &mut out).expect("runs");
        assert_eq!(code, REPORTED);
    }

    #[test]
    fn the_code_for_one_file_is_decided_by_what_it_reported() {
        let held = run_over(&walk_command(), "loss.journey", false, false).1;
        assert_eq!(held, CLEAN);
        assert_eq!(code_for(&[], false, false), CLEAN, "nothing walked is nothing reported");
        assert_eq!(code_for(&[], true, true), REPORTED, "except a file that would not read");
    }
}
