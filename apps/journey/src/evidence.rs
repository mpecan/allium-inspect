//! Reading a run off the disk, and saying where every step stands.
//!
//! The impure half of `inspect_journey::evidence`: files in, a manifest or a
//! report out. Everything with a judgement in it is in the crate, where it is
//! tested without a filesystem; this is the part that opens things.
//!
//! Two commands, and they are deliberately separate. `seal` runs once at the
//! end of a walk, is allowed to refuse, and writes a file people commit.
//! `check` reads that file back beside the markers in the code and reports —
//! which is a thing to run in a build, on a machine that never took a picture.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::Write,
    path::{Path, PathBuf},
};

use inspect_journey::{
    Manifest, Resolution, Shot, Standing, StepId, claims, evidence::manifest::VERSION, parse,
    resolve as stand, seal as seal_frames, step_texts,
};

use crate::{
    args::{Checking, Sealing},
    resolve::resolve,
    run::{CLEAN, REPORTED},
};

/// The log a harness appends to, one JSON object a line.
const LOG: &str = "frames.jsonl";
/// What `seal` writes and `check` reads.
const MANIFEST: &str = "manifest.json";

/// Turn a run's log into a manifest.
///
/// # Errors
///
/// Returns a message when the log, the journeys or the pictures cannot be read,
/// or when a frame does not resolve.
pub fn seal<W: Write>(options: &Sealing, out: &mut W) -> Result<u8, String> {
    let steps = steps_under(&options.paths)?;

    let log = options.evidence.join(LOG);
    let text =
        std::fs::read_to_string(&log).map_err(|error| format!("{}: {error}", log.display()))?;
    let shots = shots(&text, &log)?;

    // A picture the manifest points at and nobody can open is worse than one it
    // never mentioned: the panel would show a gap where it promised evidence,
    // and the run that wrote the line is long over.
    let missing: Vec<String> = shots
        .iter()
        .filter(|shot| !options.evidence.join(&shot.image).is_file())
        .map(|shot| format!("  {} — named by {}, and not in the directory", shot.image, shot.step))
        .collect();
    if !missing.is_empty() {
        return Err(format!("pictures that are not there:\n{}", missing.join("\n")));
    }

    let sealed =
        seal_frames(shots, &steps, options.at.clone().unwrap_or_else(now), options.walk.clone())
            .map_err(|faults| {
                let listed: Vec<String> = faults.iter().map(|fault| format!("  {fault}")).collect();
                format!("frames that name no step:\n{}", listed.join("\n"))
            })?;

    let path = options.evidence.join(MANIFEST);
    let json = serde_json::to_string_pretty(&sealed)
        .map_err(|error| format!("the manifest will not serialise: {error}"))?;
    std::fs::write(&path, format!("{json}\n"))
        .map_err(|error| format!("{}: {error}", path.display()))?;

    writeln!(out, "sealed {} frames into {}", sealed.frames.len(), path.display())
        .map_err(|error| error.to_string())?;
    Ok(CLEAN)
}

/// Say where every step stands.
///
/// # Errors
///
/// Returns a message when the journeys, the manifest or the source cannot be
/// read.
pub fn check<W: Write>(options: &Checking, out: &mut W) -> Result<u8, String> {
    let steps = steps_under(&options.journeys)?;
    let manifest = match &options.evidence {
        Some(directory) => read_manifest(&directory.join(MANIFEST))?,
        None => None,
    };
    let claimed = claims(&sources_under(&options.code));
    let resolution = stand(&steps, manifest.as_ref(), &claimed);

    write!(out, "{}", render(&resolution)).map_err(|error| error.to_string())?;

    Ok(if resolution.is_failure() && !options.report { REPORTED } else { CLEAN })
}

/// Every step there is, and what it currently says.
///
/// Refuses two journeys of one name. They would silently overwrite one another
/// here, and a picture filed under the loser would resolve against the winner —
/// evidence quietly attributed to a journey it is not of.
fn steps_under(paths: &[PathBuf]) -> Result<BTreeMap<StepId, String>, String> {
    let found = resolve(paths);
    if found.journeys.is_empty() {
        return Err(format!(
            "no .journey files among {}",
            paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        ));
    }

    let mut steps = BTreeMap::new();
    let mut whose: BTreeMap<String, PathBuf> = BTreeMap::new();

    for file in &found.journeys {
        let text = std::fs::read_to_string(file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let journeys = parse(&text).map_err(|error| format!("{}:{error}", file.display()))?;

        for journey in &journeys {
            if let Some(first) = whose.get(&journey.name) {
                return Err(format!(
                    "two journeys are called `{}`: {} and {}",
                    journey.name,
                    first.display(),
                    file.display()
                ));
            }
            whose.insert(journey.name.clone(), file.clone());
        }

        steps.extend(step_texts(&text, &journeys));
    }

    Ok(steps)
}

/// The log, one shot a line, saying which line could not be read.
fn shots(text: &str, log: &Path) -> Result<Vec<Shot>, String> {
    let mut shots = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let shot = serde_json::from_str(line)
            .map_err(|error| format!("{}:{}: {error}", log.display(), index + 1))?;
        shots.push(shot);
    }
    Ok(shots)
}

/// The manifest, when there is one.
fn read_manifest(path: &Path) -> Result<Option<Manifest>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;

    // Written by a harness that does not ship with this binary, so the two
    // versions move apart on their own. Saying so beats reading the fields of a
    // shape this does not know.
    if manifest.version != VERSION {
        return Err(format!(
            "{}: manifest version {} — this reads version {VERSION}",
            path.display(),
            manifest.version
        ));
    }

    Ok(Some(manifest))
}

/// Every readable file under `paths`, with its path, for the marker scan.
///
/// Skips the directories that are always generated and always enormous.
/// Anything that is not UTF-8 is skipped rather than reported: a marker lives
/// in a comment, and a file with no text in it has none.
fn sources_under(paths: &[PathBuf]) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for path in paths {
        gather(path, &mut files, 0);
    }
    files.sort();
    files.dedup();
    files
}

/// Directories whose contents nobody wrote.
const GENERATED: [&str; 4] = ["target", "node_modules", ".git", "dist"];

/// The same backstop as the journey walk, for the same reason: a link that
/// points at one of its own parents.
const DEEPEST: usize = 64;

fn gather(path: &Path, into: &mut Vec<(String, String)>, depth: usize) {
    if path.is_dir() {
        if depth >= DEEPEST {
            return;
        }
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if GENERATED.contains(&name) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            gather(&entry.path(), into, depth + 1);
        }
        return;
    }
    if let Ok(text) = std::fs::read_to_string(path) {
        into.push((path.display().to_string(), text));
    }
}

/// Now, as a manifest spells it.
///
/// The one clock in any of this, and it is here rather than in the crate for
/// the reason stipulation 2 gives: a pure crate that read the time could not be
/// replayed. `seal` is a command somebody runs once, so it may.
fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    format!("{seconds}")
}

/// The report a person reads.
fn render(resolution: &Resolution) -> String {
    let mut out = String::new();

    let shown = resolution.count(Standing::Shown);
    let _ = writeln!(out, "{shown} of {} steps have been shown", resolution.steps.len());

    for (id, evidence) in &resolution.steps {
        if evidence.standing == Standing::Unclaimed {
            continue;
        }
        let _ = writeln!(out, "  {:<10} {id}", word(evidence.standing));

        for frame in &evidence.frames {
            let _ = writeln!(out, "               {} — {}", frame.image, frame.taken_at);
        }
        if let Some(now) = &evidence.says_now {
            let _ = writeln!(out, "               it now says: {}", first_line(now));
        }
        for claim in &evidence.claims {
            if evidence.frames.is_empty() {
                let _ = writeln!(out, "               claimed by {}:{}", claim.file, claim.line);
            }
        }
    }

    // Markers naming nothing, last and unmissable: the test still passes while
    // covering something that no longer exists, which is the case worth having
    // a command for.
    for claim in &resolution.unknown {
        let _ = writeln!(
            out,
            "  no such step  {} — claimed by {}:{}",
            claim.step, claim.file, claim.line
        );
    }

    out
}

/// The word a report prints for a standing.
fn word(standing: Standing) -> &'static str {
    match standing {
        Standing::Shown => "shown",
        Standing::Failing => "failing",
        Standing::Stale => "stale",
        Standing::Claimed => "claimed",
        Standing::Unclaimed => "—",
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journey_text() -> &'static str {
        "\
journey Reading {
    goal: somebody reads a spec

    1. she points at it
        then set.status = reading

    2. it answers
        then set.status = read
}
"
    }

    /// A scratch evidence directory with a journey beside it.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("allium-journey-evidence-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch directory");
        std::fs::write(root.join("reading.journey"), journey_text()).expect("a journey");
        root
    }

    fn log_line(step: &str, image: &str) -> String {
        format!(
            r#"{{"step":"{step}","image":"{image}","caption":null,"passed":true,"taken_at":"2026-08-24T09:00:00Z","source":null}}"#
        )
    }

    fn sealing(root: &Path) -> Sealing {
        Sealing {
            evidence: root.to_owned(),
            paths: vec![root.to_owned()],
            walk: Some("reading".to_owned()),
            at: Some("2026-08-24T09:01:00Z".to_owned()),
        }
    }

    #[test]
    fn a_clean_log_seals() {
        let root = scratch("clean");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.1", "01.png")))
            .expect("a log");
        std::fs::write(root.join("01.png"), "not really a png").expect("a picture");

        let mut out = Vec::new();
        assert_eq!(seal(&sealing(&root), &mut out), Ok(CLEAN));

        let manifest = std::fs::read_to_string(root.join(MANIFEST)).expect("a manifest");
        assert!(manifest.contains("\"Reading.1\""), "{manifest}");
        assert!(manifest.contains("1. she points at it"), "{manifest}");
    }

    #[test]
    fn a_frame_naming_no_step_refuses() {
        let root = scratch("orphan");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.9", "09.png")))
            .expect("a log");
        std::fs::write(root.join("09.png"), "not really a png").expect("a picture");

        let error = seal(&sealing(&root), &mut Vec::new()).expect_err("it must refuse");
        assert!(error.contains("Reading.9"), "{error}");
        assert!(!root.join(MANIFEST).exists(), "a refusal must not leave a manifest");
    }

    #[test]
    fn a_frame_whose_picture_is_not_there_refuses() {
        let root = scratch("missing");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.1", "01.png")))
            .expect("a log");

        let error = seal(&sealing(&root), &mut Vec::new()).expect_err("it must refuse");
        assert!(error.contains("01.png"), "{error}");
    }

    #[test]
    fn a_malformed_log_line_says_which_line() {
        let root = scratch("malformed");
        std::fs::write(root.join(LOG), format!("{}\nnot json\n", log_line("Reading.1", "01.png")))
            .expect("a log");
        std::fs::write(root.join("01.png"), "not really a png").expect("a picture");

        let error = seal(&sealing(&root), &mut Vec::new()).expect_err("it must refuse");
        assert!(error.contains(":2:"), "the line number is what makes it findable: {error}");
    }

    #[test]
    fn two_journeys_of_one_name_refuse() {
        let root = scratch("clash");
        std::fs::write(root.join("again.journey"), journey_text()).expect("a second journey");
        std::fs::write(root.join(LOG), String::new()).expect("a log");

        let error = seal(&sealing(&root), &mut Vec::new()).expect_err("it must refuse");
        assert!(error.contains("two journeys are called `Reading`"), "{error}");
    }

    #[test]
    fn check_reports_a_marker_with_no_picture() {
        let root = scratch("claimed");
        std::fs::write(root.join("walk.ts"), "// journey: Reading.2\n").expect("a marked file");

        let options = Checking {
            evidence: None,
            journeys: vec![root.join("reading.journey")],
            code: vec![root.clone()],
            report: false,
        };

        let mut out = Vec::new();
        assert_eq!(check(&options, &mut out), Ok(REPORTED));

        let said = String::from_utf8(out).expect("the report is text");
        assert!(said.contains("claimed    Reading.2"), "{said}");
        assert!(said.contains("walk.ts"), "{said}");
    }

    #[test]
    fn check_reports_a_marker_naming_no_step() {
        let root = scratch("unknown");
        std::fs::write(root.join("walk.ts"), "// journey: Gone.1\n").expect("a marked file");

        let options = Checking {
            evidence: None,
            journeys: vec![root.join("reading.journey")],
            code: vec![root.clone()],
            report: false,
        };

        let mut out = Vec::new();
        assert_eq!(check(&options, &mut out), Ok(REPORTED));
        assert!(
            String::from_utf8(out).expect("text").contains("no such step  Gone.1"),
            "an unknown marker must be unmissable"
        );
    }

    /// A journey nobody has photographed is the ordinary starting state.
    #[test]
    fn nothing_at_all_is_not_a_failure() {
        let root = scratch("nothing");
        let options = Checking {
            evidence: None,
            journeys: vec![root.join("reading.journey")],
            code: Vec::new(),
            report: false,
        };

        let mut out = Vec::new();
        assert_eq!(check(&options, &mut out), Ok(CLEAN));
        assert!(String::from_utf8(out).expect("text").starts_with("0 of 2 steps have been shown"));
    }

    #[test]
    fn report_turns_a_finding_into_an_exit_zero() {
        let root = scratch("report");
        std::fs::write(root.join("walk.ts"), "// journey: Reading.2\n").expect("a marked file");

        let options = Checking {
            evidence: None,
            journeys: vec![root.join("reading.journey")],
            code: vec![root.clone()],
            report: true,
        };

        assert_eq!(check(&options, &mut Vec::new()), Ok(CLEAN));
    }

    #[test]
    fn check_reads_back_what_seal_wrote() {
        let root = scratch("roundtrip");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.1", "01.png")))
            .expect("a log");
        std::fs::write(root.join("01.png"), "not really a png").expect("a picture");
        seal(&sealing(&root), &mut Vec::new()).expect("it seals");

        let options = Checking {
            evidence: Some(root.clone()),
            journeys: vec![root.join("reading.journey")],
            code: Vec::new(),
            report: false,
        };

        let mut out = Vec::new();
        assert_eq!(check(&options, &mut out), Ok(CLEAN));
        let said = String::from_utf8(out).expect("text");
        assert!(said.starts_with("1 of 2 steps have been shown"), "{said}");
        assert!(said.contains("shown      Reading.1"), "{said}");
    }

    #[test]
    fn a_manifest_from_a_version_this_does_not_read_says_so() {
        let root = scratch("version");
        std::fs::write(
            root.join(MANIFEST),
            r#"{"version":99,"sealed_at":"now","walk":null,"frames":[]}"#,
        )
        .expect("a manifest from the future");

        let options = Checking {
            evidence: Some(root.clone()),
            journeys: vec![root.join("reading.journey")],
            code: Vec::new(),
            report: false,
        };

        let error = check(&options, &mut Vec::new()).expect_err("it must refuse");
        assert!(error.contains("version 99"), "{error}");
    }

    #[test]
    fn a_generated_directory_is_not_scanned() {
        let root = scratch("generated");
        let generated = root.join("node_modules");
        std::fs::create_dir_all(&generated).expect("a generated directory");
        std::fs::write(generated.join("dep.js"), "// journey: Reading.1\n").expect("a file");

        assert!(
            !sources_under(std::slice::from_ref(&root))
                .iter()
                .any(|(path, _)| path.contains("node_modules")),
            "a marker in a dependency is not this repository's claim"
        );
    }

    /// `now()` is the one clock in any of this. Untested, a seal without
    /// `--at` would stamp a manifest with whatever it liked.
    #[test]
    fn a_seal_with_no_time_given_stamps_one_of_its_own() {
        let root = scratch("clock");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.1", "01.png")))
            .expect("a log");
        std::fs::write(root.join("01.png"), "not really a png").expect("a picture");

        let mut options = sealing(&root);
        options.at = None;
        seal(&options, &mut Vec::new()).expect("it seals");

        let written = std::fs::read_to_string(root.join(MANIFEST)).expect("a manifest");
        let manifest: Manifest = serde_json::from_str(&written).expect("it reads back");

        let seconds: u64 = manifest.sealed_at.parse().expect("a stamp, in seconds");
        // Later than this file was written, and not some distant future.
        assert!(seconds > 1_750_000_000, "not a plausible time: {seconds}");
        assert!(seconds < 4_000_000_000, "not a plausible time: {seconds}");
    }

    /// A step is several lines. A report that printed all of them under every
    /// stale entry would bury the list it exists to be.
    #[test]
    fn a_stale_step_reports_only_its_first_line() {
        let root = scratch("firstline");
        std::fs::write(root.join(LOG), format!("{}\n", log_line("Reading.1", "01.png")))
            .expect("a log");
        std::fs::write(root.join("01.png"), "not really a png").expect("a picture");
        seal(&sealing(&root), &mut Vec::new()).expect("it seals");

        // The same journey with that step reworded under it.
        std::fs::write(
            root.join("reading.journey"),
            journey_text().replace("she points at it", "she points at the directory"),
        )
        .expect("a reworded journey");

        let options = Checking {
            evidence: Some(root.clone()),
            journeys: vec![root.join("reading.journey")],
            code: Vec::new(),
            report: false,
        };

        let mut out = Vec::new();
        assert_eq!(check(&options, &mut out), Ok(REPORTED));
        let said = String::from_utf8(out).expect("text");

        assert!(said.contains("it now says: 1. she points at the directory"), "{said}");
        assert!(
            !said.contains("then set.status = reading"),
            "the whole step was printed where one line was wanted: {said}"
        );
    }

    /// The backstop the walk has for the same reason: a link pointing at one of
    /// its own parents. Only a real guard if something has been down there.
    #[test]
    fn a_tree_deeper_than_the_cap_stops_rather_than_running_out_of_stack() {
        let root =
            std::env::temp_dir().join(format!("allium-journey-deep-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let mut deep = root.clone();
        for level in 0..(DEEPEST + 5) {
            deep = deep.join(format!("level{level}"));
        }
        std::fs::create_dir_all(&deep).expect("a deep temp tree");
        std::fs::write(deep.join("buried.rs"), "// journey: Reading.1\n").expect("a marked file");
        std::fs::write(root.join("shallow.rs"), "// journey: Reading.2\n").expect("a marked file");

        let found = sources_under(std::slice::from_ref(&root));
        assert!(
            found.iter().any(|(path, _)| path.ends_with("shallow.rs")),
            "the shallow file must still be scanned"
        );
        assert!(
            !found.iter().any(|(path, _)| path.ends_with("buried.rs")),
            "the cap must stop before the buried one"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_directory_with_no_journeys_says_so() {
        let empty =
            std::env::temp_dir().join(format!("allium-journey-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("an empty directory");

        let error = steps_under(&[empty]).expect_err("nothing to read");
        assert!(error.contains("no .journey files"), "{error}");
    }
}
