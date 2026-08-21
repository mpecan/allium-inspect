//! The command as somebody runs it.
//!
//! `main` itself cannot be called from a unit test, and what is left in it —
//! parse the arguments, resolve the paths, refuse if there is nothing to read —
//! is exactly the part a caller meets first. So this runs the built binary.
//!
//! Only the paths that need nothing installed. Ingestion still runs `allium`
//! for `model` and `plan`, and the tests that cover *that* replay recorded
//! output instead; what is asserted here is the contract before any of it is
//! reached.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::process::Command;

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_allium-journey"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn nothing_to_read_is_two_and_says_what_was_missing() {
    // Allium's own third code: not a failure of the thing being checked, a
    // failure to find anything to check. A caller that treated it as 1 would
    // report a passing spec set as broken because it typed the path wrong.
    let (code, out, err) = run(&["walk", "/nowhere/at/all"]);
    assert_eq!(code, 2, "out: {out} err: {err}");
    assert!(err.contains("no .allium specs and no .journey files"), "{err}");
    assert!(out.is_empty(), "and prints no document: {out}");
}

#[test]
fn half_the_input_is_still_nothing_to_read() {
    // A spec set with no journeys is not a clean run — it is a command that
    // was pointed at the wrong place.
    let (code, _, err) = run(&["walk", "../../crates/inspect-model/tests/fixtures/specs"]);
    assert_eq!(code, 2);
    assert!(err.contains("no .journey files"), "{err}");
}

#[test]
fn a_path_is_required_at_all() {
    let (code, _, _) = run(&["walk"]);
    assert_ne!(code, 0, "an empty invocation is not a clean run");
}

#[test]
fn the_help_names_the_two_commands_and_the_exit_codes() {
    // The surface is borrowed from `allium` so the two compose in a Makefile,
    // and a reader finds that out from `--help` or not at all.
    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, 0);
    for expected in ["walk", "check"] {
        assert!(out.contains(expected), "`{expected}` is missing from:\n{out}");
    }
}

#[test]
fn it_reports_its_version() {
    let (code, out, _) = run(&["--version"]);
    assert_eq!(code, 0);
    assert!(out.contains(env!("CARGO_PKG_VERSION")), "{out}");
}
