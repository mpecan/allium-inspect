//! `allium-inspect` — explore an allium specification in a browser.
//!
//! The binary's whole job is to find the spec files, run the CLI over them,
//! bind a port and open a browser. Every decision worth testing is in
//! `inspect-model`; what is here is the plumbing, plus two things that need
//! saying out loud: the port is chosen by the operating system unless asked
//! otherwise, and a spec that fails to ingest is reported at the terminal
//! rather than served as an empty graph.

#![forbid(unsafe_code)]

mod args;
mod watch;

use std::{net::SocketAddr, path::PathBuf, process::ExitCode};

use args::Args;
use clap::Parser;
use inspect_model::ProcessRunner;
use inspect_server::{AppState, Inspection, serve};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("INSPECT_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let args = Args::parse();
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("allium-inspect: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), String> {
    let paths = args::resolve(&args.paths);
    if paths.is_empty() {
        return Err(format!(
            "no .allium files found in {}.\n\
             Point allium-inspect at a spec file, or at a directory holding some.",
            args.paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        ));
    }

    // A journey path that names nothing is a typo, and a browser opening on an
    // empty Journeys view is a slower way to find that out than being told.
    let journeys = match &args.journeys {
        Some(path) => {
            let files = args::journeys(path);
            if files.is_empty() {
                return Err(format!("no .journey files found in {}", path.display()));
            }
            files
        }
        None => Vec::new(),
    };

    let runner = ProcessRunner::new(&args.allium);
    let inspection =
        Inspection::build(&runner, &paths, &journeys).map_err(|error| error.to_string())?;

    // `--strict` and `--json` are terminal answers, so asking for either is
    // asking for the terminal.
    if args.check || args.strict || args.json {
        return check_journeys(&journeys, &inspection, args.strict, args.json);
    }

    if args.print_graph {
        let json = serde_json::to_string_pretty(&inspection.graph)
            .map_err(|error| format!("could not serialise the graph: {error}"))?;
        println!("{json}");
        return Ok(());
    }

    report(&inspection, &paths);

    let state = AppState::new(inspection);

    // Watching is on by default: the normal way to use this is with the spec
    // open in an editor beside it, and a reload that needs a restart turns a
    // two-second loop into a ten-second one.
    if !args.no_watch {
        match watch::watch(paths.clone(), journeys.clone(), args.allium.clone(), state.clone()) {
            Ok(_handle) => println!("watching for changes"),
            // Not fatal. A machine whose watcher limit is exhausted, or a
            // filesystem that does not support notifications, is a reason to
            // reload by hand rather than a reason not to serve.
            Err(error) => eprintln!("not watching ({error}); restart to pick up changes"),
        }
    }
    // Port 0 asks the operating system for a free one, which is the point: a
    // tool people run in several checkouts at once should never fail to start
    // because another copy of it is already up.
    let address = SocketAddr::from(([127, 0, 0, 1], args.port.unwrap_or(0)));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not bind {address}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("could not read the bound address: {error}"))?;

    let url = format!("http://{bound}");
    println!("allium-inspect  {url}");
    if !args.no_open
        && let Err(error) = open::that_detached(&url)
    {
        // Not fatal. The address is printed above, and a headless machine
        // having no browser is not a reason to refuse to serve.
        eprintln!("could not open a browser ({error}); open {url} yourself");
    }

    serve(listener, state).await.map_err(|error| format!("server stopped: {error}"))
}

/// Walk every journey under `path` against the spec, and say what happened.
///
/// A journey is the demand written first, so this reports by default: the steps
/// the spec cannot support are the backlog rather than an error. `--strict` is
/// the mode a finished journey is defended in.
fn check_journeys(
    files: &[PathBuf],
    inspection: &Inspection,
    strict: bool,
    json: bool,
) -> Result<(), String> {
    let mut walks = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(file)
            .map_err(|error| format!("could not read {}: {error}", file.display()))?;
        // A file that does not parse names its own line, and the path is what
        // turns that into somewhere to go.
        let journeys = inspect_journey::parse(&source)
            .map_err(|error| format!("{}:{error}", file.display()))?;
        for journey in &journeys {
            walks.push(inspect_journey::walk(
                journey,
                &inspection.graph,
                &inspection.program,
                inspection.sources_by_module(),
            ));
        }
    }

    if json {
        let document = serde_json::to_string_pretty(&inspect_journey::as_json(&walks))
            .map_err(|error| format!("could not serialise the report: {error}"))?;
        println!("{document}");
    } else {
        print!("{}", inspect_journey::render(&walks));
    }

    let strictness = if strict {
        inspect_journey::Strictness::Strict
    } else {
        inspect_journey::Strictness::Report
    };
    if inspect_journey::passes(&walks, strictness) {
        return Ok(());
    }
    Err("a journey names something this specification does not have".to_owned())
}

/// Say what was read and what is wrong with it, before the browser opens.
///
/// A spec with errors still produces a graph worth looking at, so this reports
/// rather than refuses — but it reports at the terminal too, because somebody
/// running this in a loop should not have to look at a browser to find out the
/// spec stopped parsing.
fn report(inspection: &Inspection, paths: &[PathBuf]) {
    let graph = &inspection.graph;
    println!(
        "{} module{}, {} construct{}, {} edge{}  ({})",
        graph.modules.len(),
        plural(graph.modules.len()),
        graph.nodes.len(),
        plural(graph.nodes.len()),
        graph.edges.len(),
        plural(graph.edges.len()),
        graph.allium_version,
    );

    let errors =
        graph.diagnostics.iter().filter(|d| d.severity == inspect_model::Severity::Error).count();
    let warnings =
        graph.diagnostics.iter().filter(|d| d.severity == inspect_model::Severity::Warning).count();
    if errors > 0 || warnings > 0 || !graph.findings.is_empty() {
        println!(
            "{errors} error{}, {warnings} warning{}, {} finding{}",
            plural(errors),
            plural(warnings),
            graph.findings.len(),
            plural(graph.findings.len()),
        );
    }

    if graph.nodes.is_empty() {
        eprintln!(
            "warning: nothing was found in {}. The files parsed, but declared no constructs.",
            paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        );
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
