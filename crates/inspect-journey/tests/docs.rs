//! The documentation, held against the grammar it documents.
//!
//! `docs/journeys/reference.md` opens by saying every example in it is checked
//! against the fixture spec set and can be copied and run. Nothing checked
//! that. It is the shape this repository keeps meeting: a sentence nobody
//! re-reads, read as a promise nobody kept — and the reader it misleads is
//! exactly the one who trusted the document enough to copy from it.
//!
//! So every fenced block in the journey documentation is extracted, wrapped
//! into a whole journey if it is a fragment, parsed, and **checked against the
//! fixture spec** the document names. An example that stops parsing, or that
//! names a construct the fixture set does not declare, fails here.
//!
//! A block opts out by labelling its fence: `sh` and `text` for what is not
//! journey source at all, and **`no-check`** for source that is deliberately
//! elided — the design essay shows a journey with `…` in the middle of it,
//! which is a legitimate way to write a document and not a thing to parse.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inspect_journey::{Verdict, check, parse};
use inspect_model::{Command, Ingestion, MemoryReader, SpecGraph, ingest, runner::MapRunner};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()))
}

/// The library spec set the documentation tells a reader to run against.
fn library() -> SpecGraph {
    let root = repository().join("crates/inspect-model/tests/fixtures");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut paths = Vec::new();

    for module in ["catalogue", "lending"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        reader = reader.with(&path, read(&path));
        paths.push(path);
    }

    let Ingestion { graph, .. } = ingest(&runner, &reader, &paths).expect("the fixtures ingest");
    graph
}

/// One fenced block, with where it came from.
struct Block {
    doc: String,
    /// The line the fence opens on, so a failure is somewhere to go.
    line: usize,
    fence: String,
    body: String,
}

/// Every fenced block in `doc`.
fn blocks(doc: &str) -> Vec<Block> {
    let text = read(&repository().join("docs/journeys").join(doc));
    let mut found = Vec::new();
    let mut open: Option<(usize, String, Vec<String>)> = None;

    for (index, line) in text.lines().enumerate() {
        match (&mut open, line.trim_start().strip_prefix("```")) {
            (None, Some(fence)) => open = Some((index + 1, fence.trim().to_owned(), Vec::new())),
            (Some(_), Some(_)) => {
                if let Some((line, fence, body)) = open.take() {
                    found.push(Block { doc: doc.to_owned(), line, fence, body: body.join("\n") });
                }
            }
            (Some((_, _, body)), None) => body.push(line.to_owned()),
            (None, None) => {}
        }
    }

    assert!(open.is_none(), "{doc}: a fence is never closed");
    found
}

/// Whether this block is journey source meant to hold together.
///
/// An unlabelled fence is journey source: that is the default because it is the
/// common case, and because a new example somebody adds without thinking about
/// this test is one the test then checks. Opting out takes a word.
fn is_journey(block: &Block) -> bool {
    block.fence.is_empty() && !block.body.trim().is_empty()
}

/// The people the reference's fragments talk about.
///
/// A fragment showing one clause cannot also declare who `ada` is, so the
/// wrapper supplies the cast the document uses throughout. That is the one
/// thing invented here, and it is invented from the document rather than for
/// it: every name below appears in `reference.md` as its running example.
const STANDING_CAST: &str = "\
    cast:
        ada:  Member
        bob:  Member
        copy: catalogue/Copy
";

/// A step that puts a `loan` in scope, for the clauses that assert about one.
const A_LOAN: &str = "\
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
";

/// A fragment, wrapped into something whole enough to parse and check.
///
/// The documentation shows a `cast:` block on its own, a step on its own and a
/// clause on its own, because that is how a reference is read. Each needs a
/// different amount of journey built around it, and which one it needs is
/// decided the same way the parser decides it: by what the first line looks
/// like.
fn whole(body: &str) -> String {
    let first = body
        .lines()
        .map(|line| line.split("--").next().unwrap_or("").trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default();

    if first.starts_with("journey ") {
        return body.to_owned();
    }

    // A `world` block stands on its own, like a journey — it *is* a top-level
    // declaration — but a file of nothing but a world has no journey to check,
    // so one is put below it. That also exercises the thing the block is for.
    if first == "world {" {
        return format!(
            "{body}\njourney Documented {{\n    goal: an example\n\n    1. something \
             happens\n        then copy.status = available\n}}\n"
        );
    }

    let indent = |by: &str| {
        body.lines()
            .map(|line| if line.trim().is_empty() { String::new() } else { format!("{by}{line}") })
            .collect::<Vec<String>>()
            .join("\n")
    };

    // A `cast:` block is its own cast; anything else needs one supplied.
    let own_cast = first == "cast:";
    let cast = if own_cast { "" } else { STANDING_CAST };

    // A block keyword sits directly inside a journey, above the steps.
    if matches!(first, "cast:" | "given:" | "shows:") {
        return format!(
            "journey Documented {{\n    goal: an example\n\n{cast}\n{}\n\n    2. something happens\n        then copy.status = available\n}}\n",
            indent("    ")
        );
    }

    // A numbered step, which brings its own number.
    if step_number(first).is_some() {
        return format!(
            "journey Documented {{\n    goal: an example\n\n{cast}\n{}\n}}\n",
            indent("    ")
        );
    }

    // Anything else is a clause, and a clause belongs under a step — one that
    // has already created the `loan` the reference's clauses assert about.
    format!(
        "journey Documented {{\n    goal: an example\n\n{cast}\n{A_LOAN}\n    2. something happens\n{}\n}}\n",
        indent("        ")
    )
}

fn step_number(line: &str) -> Option<u32> {
    line.split_once('.').and_then(|(number, _)| number.trim().parse().ok())
}

#[test]
fn every_journey_example_in_the_documentation_parses() {
    let mut examined = 0;

    for doc in DOCUMENTED {
        for block in blocks(doc).into_iter().filter(is_journey) {
            let source = whole(&block.body);
            examined += 1;

            if let Err(error) = parse(&source) {
                panic!(
                    "docs/journeys/{}:{} does not parse — {error}\n\n{source}",
                    block.doc, block.line
                );
            }
        }
    }

    // A wrapper that quietly stopped finding anything would leave this test
    // green while checking nothing at all, which is the failure the gate
    // self-tests exist for, one file over.
    assert!(examined >= 12, "only {examined} examples found — has the extraction broken?");
}

/// Which documents promise their examples run against the fixture spec set.
///
/// `README.md` is the design essay and its worked example is deliberately
/// against `friend-mesh` — it says so in the line above it. So it is held to
/// the grammar and not to this spec's vocabulary, which is the honest bound:
/// checking it here would mean either weakening the check or rewriting an
/// example whose whole value is being real.
const RUNNABLE: [&str; 2] = ["reference.md", "evidence.md"];

const DOCUMENTED: [&str; 4] = ["reference.md", "evidence.md", "adopting.md", "README.md"];

#[test]
fn every_journey_example_names_only_what_the_fixture_spec_declares() {
    // The claim the reference opens with: copy any of them and run it. A name
    // the fixture set does not have makes that false for the reader who tried.
    let graph = library();

    for doc in RUNNABLE {
        for block in blocks(doc).into_iter().filter(is_journey) {
            let source = whole(&block.body);
            let journeys = parse(&source).expect("it parsed in the test above");

            for journey in &journeys {
                let missing: Vec<String> = check(journey, &journeys, &graph)
                    .into_iter()
                    .filter(|note| note.verdict == Verdict::Unspecified)
                    .map(|note| note.message)
                    .collect();

                assert!(
                    missing.is_empty(),
                    "docs/journeys/{}:{} names what the fixture spec does not have:\n  {}\n\n{source}",
                    block.doc,
                    block.line,
                    missing.join("\n  ")
                );
            }
        }
    }
}
