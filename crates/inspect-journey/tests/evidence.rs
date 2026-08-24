//! Evidence, against the journey file this repository actually ships.
//!
//! The unit tests inside the crate run on a fixture somebody wrote to exercise
//! the code. This one runs on `specs/journeys/reading-a-spec.journey` — a real
//! journey, with continued clauses, `stipulate` lines, comments between steps
//! and three journeys in one file. Every one of those is a shape a hand-built
//! fixture is unlikely to contain and a slicer is likely to get wrong.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inspect_journey::{
    Standing, StepId, claims, evidence::manifest, parse, resolve, seal, step_texts,
};

fn journey_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/journeys/reading-a-spec.journey")
}

fn source() -> String {
    std::fs::read_to_string(journey_file()).expect("the repository's own journey file is readable")
}

fn texts() -> std::collections::BTreeMap<StepId, String> {
    let text = source();
    let journeys = parse(&text).expect("the repository's own journey file parses");
    step_texts(&text, &journeys)
}

#[test]
fn every_step_of_every_journey_in_the_file_is_sliced() {
    let texts = texts();

    // Three journeys, of five, five and three steps.
    assert_eq!(texts.len(), 13, "steps found: {:?}", texts.keys().collect::<Vec<_>>());
    assert!(texts.contains_key(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 5)));
    assert!(texts.contains_key(&StepId::new("AnEditReachesTheReaderWithoutBeingAskedAbout", 5)));
    assert!(texts.contains_key(&StepId::new("NarrowingToOneFileStillShowsWhatItNeeds", 3)));
}

#[test]
fn a_step_holds_its_own_clauses_and_none_of_the_next_step_s() {
    let texts = texts();
    let step =
        texts.get(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 1)).expect("step 1 was sliced");

    assert!(step.starts_with("1. she points at the directory"), "{step}");
    assert!(step.contains("SomebodyPointsAtASpecSet"), "{step}");
    assert!(step.contains("then set.status = reading"), "{step}");
    assert!(!step.contains("stipulate"), "step 1 ran on into step 2: {step}");
}

/// The step in that file with the most going on: two `stipulate` lines, an
/// `after`, a `sees`, and four lines of comment between them.
#[test]
fn the_comments_a_real_step_carries_are_not_part_of_what_it_says() {
    let texts = texts();
    let step =
        texts.get(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 2)).expect("step 2 was sliced");

    assert!(step.contains("stipulate set.module_count = 4"), "{step}");
    assert!(step.contains("after 1.second"), "{step}");
    assert!(step.contains("ines sees set.module_count on SetOfFiles"), "{step}");

    assert!(!step.contains("derived from"), "a comment survived: {step}");
    assert!(!step.contains("--"), "a comment marker survived: {step}");
}

/// The boundary a hand-built fixture will not have: the last step of a journey
/// followed by a page of comment and then the next `journey` block.
#[test]
fn the_last_step_of_a_journey_stops_before_the_prose_under_it() {
    let texts = texts();
    let step =
        texts.get(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 5)).expect("step 5 was sliced");

    assert!(step.starts_with("5. and reads what the author wrote"), "{step}");
    assert!(!step.contains("ends:"), "step 5 swallowed the `ends` line: {step}");
    assert!(!step.contains("journey"), "step 5 ran on into the next journey: {step}");
    assert!(!step.contains('}'), "step 5 kept the journey's closing brace: {step}");
}

#[test]
fn a_walk_of_the_real_file_seals_and_resolves() {
    let texts = texts();
    let step = StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 3);

    let shot = manifest::Shot {
        step: step.clone(),
        image: "03-browser.png".to_owned(),
        caption: Some("the domain view, first paint".to_owned()),
        passed: true,
        taken_at: "2026-08-24T09:00:00Z".to_owned(),
        source: Some("ui/e2e/reading-a-spec.walk.ts:42".to_owned()),
        tags: std::collections::BTreeMap::from([("theme".to_owned(), "dark".to_owned())]),
    };

    let sealed = seal(vec![shot], &texts, "2026-08-24T09:01:00Z", Some("reading".to_owned()))
        .expect("a frame naming a real step seals");

    let claimed = claims(&[(
        "walk.ts".to_owned(),
        "// journey: SomebodyMeetsASpecTheyDidNotWrite.4\n".to_owned(),
    )]);

    let resolution = resolve(&texts, Some(&sealed), &claimed);

    assert_eq!(resolution.at(&step).map(|e| e.standing), Some(Standing::Shown));
    assert_eq!(
        resolution.at(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 4)).map(|e| e.standing),
        Some(Standing::Claimed)
    );
    assert_eq!(
        resolution.at(&StepId::new("SomebodyMeetsASpecTheyDidNotWrite", 1)).map(|e| e.standing),
        Some(Standing::Unclaimed)
    );
    assert!(resolution.unknown.is_empty());
}

/// The staleness question, asked of the real file: reword a step and the
/// picture of it stops counting.
#[test]
fn rewording_a_real_step_makes_its_picture_stale() {
    let before = texts();
    let step = StepId::new("NarrowingToOneFileStillShowsWhatItNeeds", 2);

    let sealed = seal(
        vec![manifest::Shot {
            step: step.clone(),
            image: "02.png".to_owned(),
            caption: None,
            passed: true,
            taken_at: "2026-08-24T09:00:00Z".to_owned(),
            source: None,
            tags: std::collections::BTreeMap::new(),
        }],
        &before,
        "now",
        None,
    )
    .expect("the frame seals against the file as it stands");

    assert_eq!(
        resolve(&before, Some(&sealed), &[]).at(&step).map(|e| e.standing),
        Some(Standing::Shown)
    );

    // The same file with that step's heading reworded, and nothing else.
    let reworded = source().replace("he switches another file off", "he hides a second file");
    let journeys = parse(&reworded).expect("the reworded file still parses");
    let after = step_texts(&reworded, &journeys);

    let resolution = resolve(&after, Some(&sealed), &[]);
    let evidence = resolution.at(&step).expect("the step is still there");

    assert_eq!(evidence.standing, Standing::Stale);
    assert!(
        evidence.says_now.as_deref().is_some_and(|now| now.contains("hides a second file")),
        "a stale step must carry what it says now: {:?}",
        evidence.says_now
    );
    assert!(
        evidence
            .frames
            .first()
            .is_some_and(|frame| frame.said.contains("switches another file off")),
        "and the frame must keep what it said then"
    );

    // Only that step. The rest of the file did not move.
    assert_eq!(resolution.count(Standing::Stale), 1);
}
