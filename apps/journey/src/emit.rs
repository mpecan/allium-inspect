//! The documents this command prints.
//!
//! `allium analyse specs/` prints one `{command, spec_file, diagnostics,
//! findings}` object per file, streamed rather than wrapped in an array. This
//! prints the same shape, one per `.journey` file, so a caller that already
//! reads allium's output reads this with the same code.
//!
//! Which half a thing goes in follows allium's own split. A **diagnostic** is
//! about the file — where it is wrong, with a line to point at. A **finding**
//! is about the system — free-form, tagged with a `type` and a `summary`. A
//! journey that will not parse is a diagnostic; a journey that walks is a
//! finding, and the steps it could not complete are diagnostics against the
//! lines that named them.

use inspect_journey::{Verdict, Walk};
use serde_json::{Value, json};

/// How loud a diagnostic is, in the words allium prints.
///
/// A step the spec cannot support is a **warning**, not an error: a journey is
/// written before the thing it demands, so a gap is the ordinary state of one
/// and calling it an error would make the ordinary state look broken. An
/// undecided step is **info** — it is a limit of this tool rather than
/// anything about the spec. Only a journey that could not be read is an error,
/// because that is a fault in the journey itself.
#[must_use]
pub fn severity_of(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Refused | Verdict::Unspecified | Verdict::Unexposed => "warning",
        Verdict::Undecided | Verdict::Remark => "info",
        Verdict::Specified => "info",
    }
}

/// The document for one journey file.
#[must_use]
pub fn document(command: &str, file: &str, walks: &[Walk], error: Option<&str>) -> Value {
    let mut diagnostics = Vec::new();
    if let Some(message) = error {
        diagnostics.push(json!({
            "code": "allium.journey.unreadable",
            "location": { "file": file, "line": line_of(message), "col": 1 },
            "message": message,
            "severity": "error",
        }));
    }
    for walk in walks {
        for step in &walk.steps {
            for outcome in &step.outcomes {
                if outcome.verdict == Verdict::Specified {
                    continue;
                }
                diagnostics.push(json!({
                    "code": code_of(outcome.verdict),
                    "location": { "file": file, "line": outcome.line, "col": 1 },
                    "message": message_of(&walk.name, outcome),
                    "severity": severity_of(outcome.verdict),
                }));
            }
        }
    }

    json!({
        "command": command,
        "spec_file": file,
        "diagnostics": diagnostics,
        "findings": walks.iter().map(finding).collect::<Vec<_>>(),
    })
}

/// One journey, as a finding.
///
/// `type` and `summary` beside type-specific evidence, which is the shape
/// every finding allium emits already has.
fn finding(walk: &Walk) -> Value {
    let holds = walk.steps.iter().filter(|step| step.verdict() == Verdict::Specified).count();
    json!({
        "type": "journey",
        "summary": format!(
            "{}: {holds} of {} steps hold",
            walk.name,
            walk.steps.len(),
        ),
        "journey": walk.name,
        "line": walk.line,
        "verdict": verdict_name(walk.verdict()),
        "goal": walk.goal.join(" "),
        "ends": walk.ends.join(" "),
        "stipulated": walk.stipulated,
        "steps": walk.steps.iter().map(|step| json!({
            "number": step.number,
            "title": step.title,
            "verdict": verdict_name(step.verdict()),
            "outcomes": step.outcomes.iter().map(|outcome| json!({
                "line": outcome.line,
                "verdict": verdict_name(outcome.verdict),
                "about": outcome.about,
                "detail": outcome.detail,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// A dotted code, as allium spells its own.
fn code_of(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Specified => "allium.journey.holds",
        Verdict::Undecided => "allium.journey.undecided",
        Verdict::Refused => "allium.journey.refused",
        Verdict::Unspecified => "allium.journey.unspecified",
        Verdict::Unexposed => "allium.journey.unexposed",
        Verdict::Remark => "allium.journey.remark",
    }
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Specified => "specified",
        Verdict::Undecided => "undecided",
        Verdict::Refused => "refused",
        Verdict::Unspecified => "unspecified",
        Verdict::Unexposed => "unexposed",
        Verdict::Remark => "remark",
    }
}

/// What a reader is told about one line, in one sentence.
fn message_of(journey: &str, outcome: &inspect_journey::Outcome) -> String {
    let head = format!("{journey}: `{}`", outcome.about);
    match &outcome.detail {
        Some(detail) => format!("{head} — {detail}"),
        None => head,
    }
}

/// The line a parse error names, so the diagnostic can point at it.
///
/// The parser reports `line 12: expected …`, which is the only place the number
/// exists — it is prose by the time it reaches here.
fn line_of(message: &str) -> usize {
    message
        .split_once("line ")
        .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk_of(name: &str, outcomes: Vec<(Verdict, &str)>) -> Walk {
        Walk {
            name: name.to_owned(),
            title: inspect_journey::title::readable(name),
            cast: Vec::new(),
            goal: vec!["she does the thing".to_owned()],
            ends: vec!["it is done".to_owned()],
            line: 4,
            stipulated: Vec::new(),
            inherited: Vec::new(),
            after: None,
            notes: Vec::new(),
            steps: vec![inspect_journey::Walked {
                number: 1,
                title: "she does it".to_owned(),
                line: 8,
                world: inspect_sim::World::new(),
                outcomes: outcomes
                    .into_iter()
                    .map(|(verdict, about)| inspect_journey::Outcome {
                        line: 9,
                        verdict,
                        about: about.to_owned(),
                        detail: Some("because".to_owned()),
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn the_envelope_is_the_one_allium_prints() {
        // A caller that already reads `allium analyse` reads this with the same
        // code, which is the whole reason for the shape.
        let document = document("walk", "j.journey", &[], None);
        // The set, not the order: a JSON object has no order a reader can rely
        // on, and `jq` sorts them anyway.
        let mut keys: Vec<&str> =
            document.as_object().expect("an object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["command", "diagnostics", "findings", "spec_file"]);
        assert_eq!(document["command"], "walk");
        assert_eq!(document["spec_file"], "j.journey");
    }

    #[test]
    fn a_step_the_spec_cannot_support_is_a_warning_rather_than_an_error() {
        // A journey is written before the thing it demands, so a gap is the
        // ordinary state of one. Calling it an error makes ordinary progress
        // look like a broken build.
        assert_eq!(severity_of(Verdict::Unspecified), "warning");
        assert_eq!(severity_of(Verdict::Refused), "warning");
        assert_eq!(severity_of(Verdict::Unexposed), "warning");
    }

    #[test]
    fn a_step_nobody_could_decide_is_quieter_still() {
        // A limit of this tool rather than anything about the spec.
        assert_eq!(severity_of(Verdict::Undecided), "info");
    }

    #[test]
    fn only_an_unreadable_journey_is_an_error() {
        let document =
            document("walk", "j.journey", &[], Some("line 12: expected `<name>: <Type>`"));
        assert_eq!(document["diagnostics"][0]["severity"], "error");
        assert_eq!(document["diagnostics"][0]["location"]["line"], 12);
        assert_eq!(document["diagnostics"][0]["location"]["file"], "j.journey");
    }

    #[test]
    fn a_parse_error_with_no_line_still_points_somewhere() {
        let document = document("walk", "j.journey", &[], Some("something went wrong"));
        assert_eq!(document["diagnostics"][0]["location"]["line"], 1);
    }

    #[test]
    fn a_line_that_holds_produces_no_diagnostic() {
        // The report is read by somebody looking for what to do next. Every
        // passing line as a diagnostic buries the four that are not.
        let walks = [walk_of("J", vec![(Verdict::Specified, "then x = 1")])];
        assert_eq!(document("walk", "j.journey", &walks, None)["diagnostics"], json!([]));
    }

    #[test]
    fn a_line_that_does_not_hold_names_its_journey_and_quotes_itself() {
        let walks =
            [walk_of("ACopyGoesOut", vec![(Verdict::Unspecified, "ada does Nope on Desk")])];
        let document = document("walk", "j.journey", &walks, None);
        let message = document["diagnostics"][0]["message"].as_str().expect("a message");
        assert!(message.contains("ACopyGoesOut"), "{message}");
        assert!(message.contains("ada does Nope on Desk"), "{message}");
        assert!(message.contains("because"), "and why: {message}");
        assert_eq!(document["diagnostics"][0]["code"], "allium.journey.unspecified");
        assert_eq!(document["diagnostics"][0]["location"]["line"], 9);
    }

    #[test]
    fn every_journey_is_a_finding_whether_it_held_or_not() {
        // The diagnostics are what went wrong; the findings are what happened.
        // A caller checking off what it has achieved needs the whole walk.
        let walks = [
            walk_of("Held", vec![(Verdict::Specified, "then x = 1")]),
            walk_of("Gapped", vec![(Verdict::Unspecified, "ada does Nope on Desk")]),
        ];
        let document = document("walk", "j.journey", &walks, None);
        let findings = document["findings"].as_array().expect("an array");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0]["type"], "journey");
        assert_eq!(findings[0]["verdict"], "specified");
        assert_eq!(findings[1]["verdict"], "unspecified");
        assert!(
            findings[0]["summary"].as_str().is_some_and(|s| s.contains("1 of 1 steps hold")),
            "{}",
            findings[0]["summary"]
        );
    }

    #[test]
    fn a_finding_carries_what_the_journey_was_for() {
        // A list of verdicts with the intent stripped off cannot be read
        // against anything.
        let walks = [walk_of("J", vec![(Verdict::Specified, "then x = 1")])];
        let finding = &document("walk", "j.journey", &walks, None)["findings"][0];
        assert_eq!(finding["goal"], "she does the thing");
        assert_eq!(finding["ends"], "it is done");
        assert_eq!(finding["line"], 4);
    }
}
