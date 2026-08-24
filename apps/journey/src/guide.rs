//! The documentation, carried by the binary that implements it.
//!
//! An agent working in somebody else's repository has this command on `PATH`
//! and nothing else: no checkout of this project, no network, and no way to
//! know which version of the grammar the binary it is holding actually
//! implements. So the binary carries its own instructions.
//!
//! Embedded with `include_str!` rather than read from disk, which buys the one
//! property that matters here: **the guide cannot disagree with the binary**.
//! A vendored copy in another repository goes stale silently; a link to a
//! branch describes whatever that branch says today. This describes the thing
//! you are running, because it was compiled into it.
//!
//! One copy, too. These are the same files `crates/inspect-journey/tests/docs.rs`
//! parses every example out of and checks against the fixture spec, so the
//! guide is held to the grammar it documents rather than merely shipped beside
//! it.

use std::io::Write;

use crate::args::Topic;

const ADOPTING: &str = include_str!("../../../docs/journeys/adopting.md");
const REFERENCE: &str = include_str!("../../../docs/journeys/reference.md");
const EVIDENCE: &str = include_str!("../../../docs/journeys/evidence.md");
const DESIGN: &str = include_str!("../../../docs/journeys/README.md");

/// What an agent needs before it knows which topic it wants.
///
/// Deliberately not a fifth document. It is about *using the guide* rather than
/// about journeys, so there is nothing here for the other four to duplicate —
/// and a default that printed three hundred lines of prose to somebody who
/// wanted a list is a worse answer than a list.
const ORIENTATION: &str = "\
allium-journey — user journeys over an Allium specification

A journey is an executable claim about what an actor can do, written beside the
spec rather than in it, and written *first*. A step naming a surface the spec
does not have is a requirement nobody has met — reported, not an error. That is
the whole point of the tool: the report is a ledger of what the specification
still owes.

    allium-journey check specs/ journeys/ --text    the static half, no world
    allium-journey walk  specs/ journeys/ --text    run them
    allium-journey evidence check EVIDENCE --journeys journeys/ --code .

TOPICS

    adopting    adding journeys to a repository that has none — start here
    reference   the grammar: every form, one runnable example each
    evidence    pictures of a journey happening: markers, the log, the seal
    design      why it is shaped this way, and what is left out on purpose
    skill       a ready-made Claude Code skill, to paste into a repository

    allium-journey guide adopting

Printed as Markdown, which is what it is. Everything below `TOPICS` is compiled
into this binary from the documentation it was built with, so it describes the
version you are holding rather than whatever a branch says today.
";

/// A skill file, for a repository whose agents should find this on their own.
///
/// The gap this closes is discovery. Everything else here assumes somebody
/// already knows the command exists; a skill is how an agent finds out without
/// being told, in the one harness this project's author uses. It is a pointer
/// rather than a copy — the instructions stay in the binary, where they cannot
/// drift from it.
const SKILL: &str = r#"---
name: journeys
description: Write, walk and evidence Allium user journeys. Use when the user
  wants to write a journey, add journeys to a spec set, find out what a
  specification still owes, check whether a journey holds, or attach
  screenshots from a test run to the steps they demonstrate.
---

# Journeys over an Allium spec

`allium-journey` carries its own documentation. Read it before writing anything:

```sh
allium-journey guide adopting     # a repository that has none — start here
allium-journey guide reference    # the grammar
allium-journey guide evidence     # pictures of a journey actually happening
```

Then:

```sh
allium-journey check specs/ journeys/ --text
allium-journey walk  specs/ journeys/ --text
```

Three things that are easy to get wrong and worth reading the guide for:

- an `unspecified` step is **the output**, not an error. Never weaken a step to
  make it hold — a journey bent into the shape the spec already supports has
  stopped being a demand;
- `refused` is different: the spec actively forbids this, which is a
  disagreement to raise rather than an assertion to delete;
- never renumber a step. Numbers are citations, and code carries them.
"#;

/// Print one topic, or the orientation when none was asked for.
///
/// `notes` is stderr. Only the skill uses it, and it has to: a skill file
/// begins with YAML frontmatter, which must be the very first thing in the
/// file, so a line of advice on stdout would break the one use the topic has —
///
/// ```sh
/// allium-journey guide skill > .claude/skills/journeys/SKILL.md
/// ```
///
/// # Errors
///
/// Returns a message when the output cannot be written.
pub fn print<W: Write, E: Write>(
    topic: Option<Topic>,
    out: &mut W,
    notes: &mut E,
) -> Result<u8, String> {
    let text = match topic {
        None => ORIENTATION,
        Some(Topic::Adopting) => ADOPTING,
        Some(Topic::Reference) => REFERENCE,
        Some(Topic::Evidence) => EVIDENCE,
        Some(Topic::Design) => DESIGN,
        Some(Topic::Skill) => SKILL,
    };

    write!(out, "{text}").map_err(|error| error.to_string())?;

    if topic == Some(Topic::Skill) {
        writeln!(
            notes,
            "allium-journey: save this as `.claude/skills/journeys/SKILL.md` in the \
             repository that holds the specs."
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(crate::run::CLEAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn both(topic: Option<Topic>) -> (String, String) {
        let (mut out, mut notes) = (Vec::new(), Vec::new());
        assert_eq!(print(topic, &mut out, &mut notes), Ok(crate::run::CLEAN));
        (
            String::from_utf8(out).expect("the guide is text"),
            String::from_utf8(notes).expect("the notes are text"),
        )
    }

    fn printed(topic: Option<Topic>) -> String {
        both(topic).0
    }

    #[test]
    fn every_topic_prints_its_own_document() {
        // Each is checked by something it alone says, so a mapping that sent
        // two topics to one document would fail here rather than looking fine.
        assert!(printed(Some(Topic::Adopting)).contains("repository that has none"));
        assert!(printed(Some(Topic::Reference)).contains("## `cast` — who is in it"));
        assert!(printed(Some(Topic::Evidence)).contains("frames.jsonl"));
        assert!(printed(Some(Topic::Design)).contains("What a journey is"));
        assert!(printed(Some(Topic::Skill)).contains("name: journeys"));
    }

    /// The documentation is embedded, so an empty one is a build that silently
    /// shipped nothing — which reads exactly like a topic nobody wrote.
    #[test]
    fn no_topic_is_empty() {
        for topic in [
            None,
            Some(Topic::Adopting),
            Some(Topic::Reference),
            Some(Topic::Evidence),
            Some(Topic::Design),
            Some(Topic::Skill),
        ] {
            assert!(printed(topic).len() > 400, "{topic:?} printed almost nothing");
        }
    }

    /// A reader who asked for nothing is told what there is to ask for.
    #[test]
    fn the_orientation_names_every_topic() {
        let said = printed(None);
        for topic in ["adopting", "reference", "evidence", "design", "skill"] {
            assert!(said.contains(topic), "the orientation does not mention `{topic}`");
        }
    }

    #[test]
    fn the_orientation_shows_how_to_ask_for_one() {
        assert!(printed(None).contains("allium-journey guide adopting"));
    }

    /// The skill is a pointer, not a copy: it tells an agent to read the guide
    /// rather than restating a grammar that would then have two homes.
    /// The one use the topic has, and it only works if stdout is the file.
    #[test]
    fn the_skill_is_pipeable_into_the_file_it_is_for() {
        let (file, notes) = both(Some(Topic::Skill));

        assert!(file.starts_with("---\nname: journeys"), "frontmatter must open the file");
        assert!(notes.contains(".claude/skills/journeys/SKILL.md"), "and stderr says where");
    }

    #[test]
    fn nothing_else_writes_to_stderr() {
        for topic in [None, Some(Topic::Adopting), Some(Topic::Reference), Some(Topic::Design)] {
            assert!(both(topic).1.is_empty(), "{topic:?} wrote to stderr");
        }
    }

    #[test]
    fn the_skill_points_at_the_guide_rather_than_repeating_it() {
        let skill = printed(Some(Topic::Skill));
        assert!(skill.contains("allium-journey guide adopting"));
        assert!(!skill.contains("## `cast`"), "the skill has started copying the reference");
    }
}
