//! What a step says, sliced from the file somebody wrote it in.
//!
//! This is the whole of the staleness question. A picture is of a step; if the
//! step has since been reworded, the picture may no longer show what it now
//! says, and a tool that kept showing it would be asserting something nobody
//! checked. So a sealed frame carries the step as it read at the time, and the
//! comparison is against the step as it reads now.
//!
//! Two normalisations, and both are the difference between a rewording and a
//! tidy-up:
//!
//! - **comments go**, following the same reasoning as the collapsed one-line
//!   form of a clause: a `--` note is the author talking to the next reader,
//!   not part of what the step demands;
//! - **layout goes** — each line is trimmed and blank lines dropped — so
//!   reindenting a journey file does not invalidate every photograph in it.
//!
//! What is left is the words. Changing those is a rewording, and should cast
//! doubt on the picture.

use std::collections::BTreeMap;

use crate::{
    evidence::StepId,
    journey::{Journey, Step},
    parse::indent_of,
};

/// Every step's text, keyed by id, sliced from the source it was parsed from.
///
/// `journeys` must be the parse of `source`; the span comes from the line
/// numbers that parse recorded, never from reading the file a second way. A
/// second implementation of "where does a step stop" is a second thing that can
/// disagree with the parser, and this crate already has the answer.
#[must_use]
pub fn step_texts(source: &str, journeys: &[Journey]) -> BTreeMap<StepId, String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut texts = BTreeMap::new();

    for journey in journeys {
        for step in &journey.steps {
            texts.insert(StepId::new(&journey.name, step.number), slice(&lines, step));
        }
    }

    texts
}

/// A step's own lines, as the words.
///
/// Bounded by its **last clause and that clause's continuations** rather than
/// by whatever comes next. Running to the next step looks equivalent and is
/// not: a journey's last step is followed by its `ends:` prose, and the first
/// version of this swallowed a paragraph of it — caught by
/// `tests/evidence.rs`, against the journey file this repository ships, which
/// is the shape a hand-built fixture did not have.
fn slice(lines: &[&str], step: &Step) -> String {
    let last = step.clauses.last().map_or(step.line, |clause| clause.line());
    let stops = continuations_end(lines, last);

    let mut words: Vec<&str> = Vec::new();
    for at in step.line..=stops {
        let Some(line) = lines.get(at.saturating_sub(1)) else { break };
        let trimmed = strip_comment(line).trim();
        if !trimmed.is_empty() {
            words.push(trimmed);
        }
    }

    words.join("\n")
}

/// The last line belonging to the clause that starts at `from`.
///
/// The parser's own rule, and deliberately its own function so it reads as the
/// borrowed thing it is: a line indented deeper than the clause above it
/// *continues* that clause. `ends:`, the next step and the closing brace all
/// sit at or below the clause's indent, so this stops at each of them without
/// having to know what any of them are.
fn continuations_end(lines: &[&str], from: usize) -> usize {
    let Some(opening) = lines.get(from.saturating_sub(1)) else { return from };
    let depth = indent_of(opening);

    let mut last = from;
    for (at, line) in lines.iter().enumerate().skip(from) {
        if strip_comment(line).trim().is_empty() {
            continue;
        }
        if indent_of(line) <= depth {
            break;
        }
        last = at + 1;
    }

    last
}

/// A line without its trailing `--` comment.
///
/// Its own copy rather than the parser's, which is private and takes the whole
/// line as a clause. The rule is the same and small: `--` starts a comment, and
/// nothing in a journey quotes one.
fn strip_comment(line: &str) -> &str {
    match line.find("--") {
        Some(at) => &line[..at],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const SOURCE: &str = "\
-- a note about the file
journey First {
    goal: something

    1. she points at it
        ada does Look(ada) on Shelf
        then Looked fires

    2. and it answers
        -- this comment is not part of the step
        then thing.status = read
}

journey Second {
    goal: another

    1. only one step here
        then other.status = open
}
";

    fn texts() -> BTreeMap<StepId, String> {
        let journeys = parse(SOURCE).expect("the fixture parses");
        step_texts(SOURCE, &journeys)
    }

    #[test]
    fn a_step_stops_where_the_next_one_starts() {
        assert_eq!(
            texts().get(&StepId::new("First", 1)).map(String::as_str),
            Some("1. she points at it\nada does Look(ada) on Shelf\nthen Looked fires")
        );
    }

    #[test]
    fn comments_are_not_part_of_what_a_step_says() {
        let text = texts().get(&StepId::new("First", 2)).cloned().unwrap_or_default();
        assert!(!text.contains("comment"), "a comment survived into the step text: {text}");
        assert_eq!(text, "2. and it answers\nthen thing.status = read");
    }

    #[test]
    fn the_last_step_of_a_journey_stops_before_the_next_journey() {
        let text = texts().get(&StepId::new("First", 2)).cloned().unwrap_or_default();
        assert!(!text.contains("Second"), "step 2 ran on into the next journey: {text}");
        assert!(!text.contains("goal"), "step 2 ran on into the next journey: {text}");
    }

    #[test]
    fn the_last_step_of_the_last_journey_stops_at_the_end_of_the_file() {
        assert_eq!(
            texts().get(&StepId::new("Second", 1)).map(String::as_str),
            Some("1. only one step here\nthen other.status = open")
        );
    }

    #[test]
    fn every_step_of_every_journey_is_there() {
        let texts = texts();
        assert_eq!(texts.len(), 3);
        assert!(texts.contains_key(&StepId::new("Second", 1)));
    }

    /// The point of the normalisation: a tidy-up is not a rewording.
    #[test]
    fn reindenting_does_not_change_what_a_step_says() {
        let reindented = SOURCE.replace("        ", "  ").replace("    ", " ");
        let journeys = parse(&reindented).expect("the reindented fixture parses");
        let after = step_texts(&reindented, &journeys);
        assert_eq!(after.get(&StepId::new("First", 1)), texts().get(&StepId::new("First", 1)));
    }

    /// And the point of comparing at all: a rewording is.
    #[test]
    fn rewording_a_step_changes_what_it_says() {
        let reworded = SOURCE.replace("she points at it", "she points at the directory");
        let journeys = parse(&reworded).expect("the reworded fixture parses");
        let after = step_texts(&reworded, &journeys);
        assert_ne!(after.get(&StepId::new("First", 1)), texts().get(&StepId::new("First", 1)));
    }

    /// A clause wrapped over two lines: the parser records only where it
    /// *starts*, so a step bounded by its last clause's line alone would stop
    /// one line short and lose the rest of the sentence.
    #[test]
    fn a_clause_continued_on_the_next_line_is_part_of_the_step() {
        let source = "\
journey Wrapped {
    goal: something

    1. she borrows one
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
}
";
        let journeys = parse(source).expect("a wrapped clause parses");
        let text = step_texts(source, &journeys)
            .get(&StepId::new("Wrapped", 1))
            .cloned()
            .unwrap_or_default();

        assert!(text.contains("creating loan: Loan"), "the continuation was cut off: {text}");
        assert!(!text.contains('}'), "and it did not run on past the journey: {text}");
    }

    /// The other direction: a continuation must not swallow what follows it.
    #[test]
    fn a_continuation_stops_at_the_next_thing_at_its_own_depth() {
        let source = "\
journey Wrapped {
    goal: something

    1. she borrows one
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
        then loan.status = open

    2. and returns it
        then loan.status = returned
}
";
        let journeys = parse(source).expect("the fixture parses");
        let texts = step_texts(source, &journeys);
        let first = texts.get(&StepId::new("Wrapped", 1)).cloned().unwrap_or_default();

        assert!(first.contains("creating loan: Loan"), "{first}");
        assert!(first.contains("then loan.status = open"), "{first}");
        assert!(!first.contains("returned"), "step 1 ran on into step 2: {first}");
    }

    #[test]
    fn a_journey_with_no_steps_contributes_nothing() {
        let source = "journey Empty {\n    goal: nothing happens\n}\n";
        let journeys = parse(source).expect("an empty journey parses");
        assert!(step_texts(source, &journeys).is_empty());
    }
}
