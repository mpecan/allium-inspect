//! What the code claims to demonstrate.
//!
//! A marker is a comment naming a step:
//!
//! ```text
//! // journey: SomebodyMeetsASpecTheyDidNotWrite.3
//! ```
//!
//! Shaped after the `/// allium: rule-success.MintIdentity` tags this project's
//! sister repository already carries, so the two read as siblings rather than as
//! two people's conventions.
//!
//! It exists for one state and would not be worth having for any other: **a
//! test that says it shows a step and produced no picture**. Without the scan,
//! a harness that quietly stopped photographing and a step nobody ever covered
//! leave exactly the same trace — nothing — and the tool would report the first
//! as the second. That is the failure shape the repository's gate self-tests
//! exist for, one layer out.
//!
//! Deliberately no regular expression. The rule is small enough to read in
//! full, and a dependency whose whole job is six lines of `find` and `parse` is
//! rent with no tenant.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::evidence::StepId;

/// The word a marker opens with, after its comment punctuation.
const MARKER: &str = "journey:";

/// One place in the code that says it demonstrates a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Claim {
    pub step: StepId,
    /// The file, as the caller named it — a path a reader can act on.
    pub file: String,
    pub line: usize,
}

/// Every marker in every file, in the order they were given.
///
/// Takes text rather than paths: reading files is the app layer's job, which is
/// what keeps this crate testable without one and free of a filesystem.
#[must_use]
pub fn claims(files: &[(String, String)]) -> Vec<Claim> {
    let mut found = Vec::new();

    for (file, text) in files {
        for (index, line) in text.lines().enumerate() {
            if let Some(step) = claim_in(line) {
                found.push(Claim { step, file: file.clone(), line: index + 1 });
            }
        }
    }

    found
}

/// The step a line claims, if it claims one.
///
/// Only from a comment. Requiring the run-up to be comment punctuation is what
/// stops a string literal or a stretch of prose that happens to contain the
/// word from filing a claim nobody made — and every comment syntax worth
/// supporting is made of these five characters.
fn claim_in(line: &str) -> Option<StepId> {
    let at = line.find(MARKER)?;
    let before = line.get(..at)?.trim();
    if !before.chars().all(|c| matches!(c, '/' | '*' | '#' | '-' | '!' | ';' | '%')) {
        return None;
    }

    // The id runs to the first whitespace, so a marker may carry a note after
    // it: `// journey: Name.3 — the empty list`.
    let rest = line.get(at + MARKER.len()..)?.trim_start();
    rest.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Option<StepId> {
        claim_in(line)
    }

    #[test]
    fn reads_the_shapes_the_languages_here_write() {
        for line in [
            "// journey: Reading.3",
            "/// journey: Reading.3",
            "  // journey: Reading.3",
            "# journey: Reading.3",
            "-- journey: Reading.3",
            "/* journey: Reading.3 */",
            " * journey: Reading.3",
        ] {
            assert_eq!(one(line), Some(StepId::new("Reading", 3)), "did not read `{line}`");
        }
    }

    #[test]
    fn a_marker_may_carry_a_note_after_it() {
        assert_eq!(
            one("// journey: Reading.3 — her empty chat list"),
            Some(StepId::new("Reading", 3))
        );
    }

    /// The reason the run-up is checked at all.
    #[test]
    fn code_that_merely_contains_the_word_claims_nothing() {
        for line in [
            r#"const help = "journey: Reading.3";"#,
            "let x = journey: Reading.3",
            "print(journey: Reading.3)",
        ] {
            assert_eq!(one(line), None, "`{line}` should not file a claim");
        }
    }

    #[test]
    fn a_marker_naming_something_that_is_not_a_step_claims_nothing() {
        assert_eq!(one("// journey: Reading"), None);
        assert_eq!(one("// journey:"), None);
        assert_eq!(one("// journey: Reading.x"), None);
    }

    #[test]
    fn every_marker_in_a_file_is_found_with_its_line() {
        let text = "fn a() {}\n// journey: Reading.1\nfn b() {}\n\n// journey: Reading.2\n";
        let found = claims(&[("walk.rs".to_owned(), text.to_owned())]);

        assert_eq!(
            found,
            vec![
                Claim { step: StepId::new("Reading", 1), file: "walk.rs".to_owned(), line: 2 },
                Claim { step: StepId::new("Reading", 2), file: "walk.rs".to_owned(), line: 5 },
            ]
        );
    }

    #[test]
    fn two_files_both_contribute() {
        let found = claims(&[
            ("a.ts".to_owned(), "// journey: Reading.1\n".to_owned()),
            ("b.rs".to_owned(), "// journey: Reading.2\n".to_owned()),
        ]);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found.iter().map(|claim| claim.file.as_str()).collect::<Vec<_>>(),
            ["a.ts", "b.rs"]
        );
    }

    #[test]
    fn a_file_with_nothing_to_say_contributes_nothing() {
        assert!(claims(&[("empty.rs".to_owned(), String::new())]).is_empty());
        assert!(claims(&[]).is_empty());
    }

    /// Two tests may honestly demonstrate one step from different angles.
    #[test]
    fn one_step_may_be_claimed_twice() {
        let found = claims(&[(
            "walk.ts".to_owned(),
            "// journey: Reading.1\n// journey: Reading.1\n".to_owned(),
        )]);
        assert_eq!(found.len(), 2);
    }
}
