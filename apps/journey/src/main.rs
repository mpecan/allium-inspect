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
mod run;

use std::process::ExitCode;

use args::Args;
use clap::Parser;
use inspect_model::ProcessRunner;

fn main() -> ExitCode {
    let args = Args::parse();
    let options = args.command.options();

    let found = resolve::resolve(&options.paths);
    if !found.is_usable() {
        eprintln!(
            "allium-journey: {} among {}",
            found.missing(),
            options.paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(run::UNUSABLE);
    }

    let runner = ProcessRunner::new(&options.allium);
    let mut out = std::io::stdout().lock();
    match run::all(&runner, &args.command, options, &found, &mut out) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("allium-journey: {message}");
            ExitCode::from(run::UNUSABLE)
        }
    }
}
