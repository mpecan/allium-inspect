//! `allium-journey` — walk user journeys against an Allium specification.
//!
//! A sibling to `allium`, not a subcommand of this repository's browser. The
//! two share their whole engine — the same parser, the same graph, the same
//! three-valued simulator — and differ in what they are for: the browser is
//! for looking at a specification, and this is for asking one whether it does
//! what somebody set out to do, from a Makefile.
//!
//! Everything about its surface is borrowed from `allium` so the two compose:
//! a bare `<path>...` searched recursively, one JSON document per file on
//! stdout, and 0 / 1 / 2 for nothing to say, something reported, nothing to
//! read. See [`args`] for what each of those is.
//!
//! The one deliberate divergence is `--report`, which exits 0 even when the
//! spec cannot support a step. Allium has no equivalent because it has nothing
//! to say it in: a journey is *written before* the thing it demands, so the
//! ordinary state of a new one is full of gaps, and a command that failed on
//! those would be turned off by the first person who wrote one.

#![forbid(unsafe_code)]

mod args;
mod emit;
mod resolve;

use std::{collections::BTreeMap, path::Path, process::ExitCode};

use args::{Args, Command, Run};
use clap::Parser;
use inspect_journey::{Verdict, Walk};
use inspect_model::{FileReader, Ingestion, ProcessRunner, SourceReader, ingest, module_name};
use inspect_sim::step::Sources;
use resolve::Found;

/// Nothing to say.
const CLEAN: u8 = 0;
/// Something was reported.
const REPORTED: u8 = 1;
/// Nothing to read: no paths, or none that resolved.
const UNUSABLE: u8 = 2;

fn main() -> ExitCode {
    let args = Args::parse();
    let run = args.command.options();

    let found = resolve::resolve(&run.paths);
    if !found.is_usable() {
        eprintln!(
            "allium-journey: {} among {}",
            found.missing(),
            run.paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(UNUSABLE);
    }

    match walk_all(&args.command, run, &found) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("allium-journey: {message}");
            ExitCode::from(UNUSABLE)
        }
    }
}

/// Ingest the specs, then answer for each journey file in turn.
fn walk_all(command: &Command, run: &Run, found: &Found) -> Result<u8, String> {
    let runner = ProcessRunner::new(&run.allium);
    let Ingestion { graph, program } =
        ingest(&runner, &FileReader, &found.specs).map_err(|error| error.to_string())?;
    let sources = read_sources(&found.specs);

    let mut worst = CLEAN;
    for file in &found.journeys {
        let name = file.to_string_lossy().into_owned();
        let (walks, error) = read_one(command, file, &graph, &program, &sources);

        if run.text {
            print_text(&name, &walks, error.as_deref());
        } else {
            let document = emit::document(command.as_str(), &name, &walks, error.as_deref());
            println!(
                "{}",
                serde_json::to_string_pretty(&document)
                    .map_err(|error| format!("could not serialise the report: {error}"))?
            );
        }
        worst = worst.max(code_for(&walks, error.is_some(), run.report));
    }
    Ok(worst)
}

/// One journey file: its walks, or why it could not be read.
fn read_one(
    command: &Command,
    file: &Path,
    graph: &inspect_model::SpecGraph,
    program: &inspect_model::Program,
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
fn statically(journey: &inspect_journey::Journey, graph: &inspect_model::SpecGraph) -> Walk {
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
fn code_for(walks: &[Walk], unreadable: bool, report: bool) -> u8 {
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
fn print_text(file: &str, walks: &[Walk], error: Option<&str>) {
    println!("{file}");
    match error {
        Some(message) => println!("  could not be read — {message}"),
        None => print!("{}", inspect_journey::render(walks)),
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
