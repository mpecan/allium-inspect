//! Walking journeys against configuration the spec does not default to
//! (issue #11).
//!
//! A journey used to start from the spec's config defaults and have no way to
//! change one: `config.lfs_enabled = true` in `given` or in a `stipulate` came
//! back "`config` is not a name this journey bound", so a rule guarded by a
//! switch that defaults to off could only ever be walked refused. Much of a
//! real system is off by default, and the workarounds — moving the check onto
//! a surface, wrapping config in a black box — each changed the spec to suit
//! the tool.
//!
//! `config.<name> = …` now writes the parameter, wherever a journey can write.
//! It is a departure from what the spec says its world is, so it is reported
//! as a stipulation, first and always, like any other.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::SWITCHES;
use inspect_journey::{Verdict, Walk, parse, render, walk};

fn walked_nth(source: &str, at: usize) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(SWITCHES);
    walk(&journeys[at], &journeys, &graph, &program, &sources)
}

fn walked(source: &str) -> Walk {
    walked_nth(source, 0)
}

/// The outcome of the line that reads `about`, in a step or outside one.
fn line(walk: &Walk, about: &str) -> (Verdict, String) {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .chain(walk.notes.iter())
        .find(|outcome| outcome.about == about)
        .map(|outcome| (outcome.verdict, outcome.detail.clone().unwrap_or_default()))
        .unwrap_or_else(|| panic!("no line `{about}` in {walk:#?}"))
}

fn stipulated(walk: &Walk) -> Vec<(String, Option<String>)> {
    walk.stipulated.iter().map(|told| (told.said.clone(), told.through.clone())).collect()
}

/// One journey over one repo, with `given` and the clauses of one step.
fn journey(given: &str, clauses: &str) -> String {
    format!(
        "journey J {{
    cast:
        r: Repo
    given:
{given}
    1. the step
{clauses}
}}"
    )
}

const FILED: &str = include_str!("fixtures/switches.journey");

#[test]
fn the_issue_as_filed_walks_a_switch_turned_on_in_given() {
    let result = walked_nth(FILED, 0);
    assert!(result.notes.is_empty(), "the `given` line landed: {:#?}", result.notes);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    // Not invisibly: a world the spec does not default to is something the
    // journey was told, and the ledger says so before any step.
    assert_eq!(stipulated(&result), vec![("config.lfs_enabled = true".to_owned(), None)]);
}

#[test]
fn the_issue_as_filed_walks_a_switch_turned_on_by_a_stipulation() {
    let result = walked_nth(FILED, 1);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert_eq!(stipulated(&result), vec![("config.lfs_enabled = true".to_owned(), None)]);
}

#[test]
fn a_switch_nobody_turned_on_is_still_off() {
    // The default is still in force wherever a journey says nothing, so the
    // refusal the issue could already show is still what it shows.
    let result = walked_nth(FILED, 2);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert!(result.stipulated.is_empty(), "nothing was told: {:#?}", result.stipulated);
}

#[test]
fn the_text_report_lists_the_switch_before_the_steps() {
    let text = render(&[walked_nth(FILED, 0)]);
    let told = text.find("stipulated  config.lfs_enabled = true").expect(&text);
    let first_step = text.find(" 1. ").expect(&text);
    assert!(told < first_step, "the switch is reported first:\n{text}");
    assert!(!text.contains("not a name this journey bound"), "{text}");
}

#[test]
fn a_number_and_a_parameter_with_no_default_can_be_set_too() {
    let result = walked(&journey(
        "        config.max_mirrors = 2
        config.sync_mode = \"push\"",
        "        r does Mirror(r) on Settings
        r does Sync(r) on Settings
        then r.mirrored = true
        then r.synced = true",
    ));
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert_eq!(result.stipulated.len(), 2, "{:#?}", result.stipulated);
}

#[test]
fn a_parameter_with_no_default_is_undecided_until_somebody_says() {
    // The other half of the same honesty: with nothing said, `Sync` is neither
    // refused nor allowed, and the reason names the parameter.
    let result = walked(&journey("        r.synced = false", "        r does Sync(r) on Settings"));
    let (verdict, why) = line(&result, "r does Sync(r) on Settings");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("sync_mode"), "{why}");
}

#[test]
fn the_module_may_be_written_in_front_of_config() {
    let result = walked(&journey(
        "        switches/config.max_mirrors = 1",
        "        r does Mirror(r) on Settings
        then r.mirrored = true",
    ));
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert_eq!(
        stipulated(&result),
        vec![("switches/config.max_mirrors = 1".to_owned(), None)],
        "said as it was written"
    );
}

#[test]
fn a_parameter_the_spec_does_not_declare_is_unspecified_and_changes_nothing() {
    let result = walked(&journey(
        "        config.lfs_enabld = true",
        "        r cannot do UseLfs(r) on Settings",
    ));
    let (verdict, why) = line(&result, "given config.lfs_enabld");
    assert_eq!(verdict, Verdict::Unspecified, "{why}");
    assert!(why.contains("`lfs_enabld`"), "names the parameter: {why}");
    // A typo is not a switch: `lfs_enabled` is still off, and the ledger does
    // not list a write that never reached the world.
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert!(result.stipulated.is_empty(), "{:#?}", result.stipulated);
}

#[test]
fn a_module_that_declares_no_such_parameter_is_unspecified() {
    let result = walked(&journey("        lending/config.lfs_enabled = true", ""));
    let (verdict, why) = line(&result, "given lending/config.lfs_enabled");
    assert_eq!(verdict, Verdict::Unspecified, "{why}");
    assert!(why.contains("`lending`"), "{why}");
}

#[test]
fn a_value_of_the_wrong_type_is_not_written() {
    // `5` for a Boolean would reach `requires: config.lfs_enabled` as a number,
    // and a rule refused over that would be refused over a world nobody meant.
    let result = walked(&journey("", "        stipulate config.lfs_enabled = 5"));
    let (verdict, why) = line(&result, "stipulate config.lfs_enabled = 5");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("Boolean"), "names the declared type: {why}");
    assert!(result.stipulated.is_empty(), "{:#?}", result.stipulated);
}

#[test]
fn a_path_past_the_parameter_names_nothing_to_set() {
    let result = walked(&journey("", "        stipulate config.lfs_enabled.deep = true"));
    let (verdict, why) = line(&result, "stipulate config.lfs_enabled.deep = true");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("config.lfs_enabled.deep"), "{why}");
}

#[test]
fn a_switch_can_be_read_back() {
    let result = walked(&journey(
        "        config.lfs_enabled = true",
        "        then config.lfs_enabled = true
        then config.max_mirrors = 0",
    ));
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
}

#[test]
fn a_switch_in_the_file_world_is_reported_from_the_file() {
    let result = walked(
        "world {
    cast:
        r: Repo
    given:
        config.lfs_enabled = true
}

journey J {
    1. the step
        r does UseLfs(r) on Settings
        then r.lfs_used = true
}",
    );
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    let inherited: Vec<&str> = result.inherited.iter().map(|line| line.said.as_str()).collect();
    assert!(inherited.contains(&"config.lfs_enabled = true"), "{inherited:?}");
    // Said once, where it was written, rather than twice.
    assert!(result.stipulated.is_empty(), "{:#?}", result.stipulated);
}

#[test]
fn a_switch_turned_on_earlier_in_a_chain_says_whose_it_was() {
    let source = "journey TurnedOn {
    cast:
        r: Repo
    given:
        config.lfs_enabled = true
    1. it is used
        r does UseLfs(r) on Settings
}

journey AndStaysUsed {
    after: TurnedOn
    1. it was used
        then r.lfs_used = true
}";
    let result = walked_nth(source, 1);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert_eq!(
        stipulated(&result),
        vec![("config.lfs_enabled = true".to_owned(), Some("TurnedOn".to_owned()))]
    );
}

#[test]
fn a_file_world_switch_under_a_chain_is_listed_once() {
    // The later journey stands on the earlier, and both on the file's world.
    // The line is the file's, so it is listed `from the file` and not again
    // `through` a journey that never wrote it.
    let source = "world {
    cast:
        r: Repo
    given:
        config.lfs_enabled = true
}

journey TurnedOn {
    1. it is used
        r does UseLfs(r) on Settings
}

journey AndStaysUsed {
    after: TurnedOn
    1. it was used
        then r.lfs_used = true
}";
    let result = walked_nth(source, 1);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert!(result.stipulated.is_empty(), "{:#?}", result.stipulated);
    let inherited: Vec<&str> = result.inherited.iter().map(|line| line.said.as_str()).collect();
    assert!(inherited.contains(&"config.lfs_enabled = true"), "{inherited:?}");
}

#[test]
fn a_chain_does_not_lay_the_file_world_over_what_the_earlier_journey_changed() {
    // The earlier journey turned the switch on and set a field; laying the
    // file's world out again under the later one turned the switch back off
    // and cast a second `r`, so the later step was refused by a world its own
    // ledger contradicted.
    let source = "world {
    cast:
        r: Repo
    given:
        config.max_mirrors = 0
}

journey TurnOn {
    1. turn on
        stipulate config.max_mirrors = 2
        stipulate r.synced = true
}

journey Then {
    after: TurnOn
    1. still on
        r does Mirror(r) on Settings
        then r.mirrored = true
        then r.synced = true
}";
    let result = walked_nth(source, 1);
    assert_eq!(result.steps[0].verdict(), Verdict::Specified, "{result:#?}");
    assert_eq!(result.cast.iter().filter(|member| member.name == "r").count(), 1, "one `r`");
    assert_eq!(
        stipulated(&result),
        vec![
            ("config.max_mirrors = 2".to_owned(), Some("TurnOn".to_owned())),
            ("r.synced = true".to_owned(), Some("TurnOn".to_owned())),
        ]
    );
}

#[test]
fn a_stipulated_parameter_the_spec_does_not_declare_is_unspecified() {
    let result = walked(&journey("", "        stipulate config.lfs_enabld = true"));
    let (verdict, why) = line(&result, "stipulate config.lfs_enabld = true");
    assert_eq!(verdict, Verdict::Unspecified, "{why}");
    assert!(why.contains("`lfs_enabld`"), "{why}");
    assert!(result.stipulated.is_empty(), "{:#?}", result.stipulated);
}

#[test]
fn a_name_that_only_ends_another_parameter_reads_nothing() {
    // `enabled` is the tail of `lfs_enabled`, and is not a parameter. Reading
    // it off the longer name held `then config.enabled = false` against a
    // switch the line never named.
    let result = walked(&journey("", "        then config.enabled = false"));
    let (verdict, why) = line(&result, "then config.enabled = false");
    assert_eq!(verdict, Verdict::Unspecified, "{why}");
    assert!(why.contains("`enabled`"), "says which: {why}");
}

#[test]
fn reading_and_writing_an_undeclared_parameter_give_the_same_answer() {
    let read = walked(&journey("", "        then config.nope = 1"));
    let written = walked(&journey("        config.nope = 1", ""));
    assert_eq!(line(&read, "then config.nope = 1"), line(&written, "given config.nope"));
    // On the right of a comparison too.
    let right = walked(&journey("", "        then r.mirrored = config.nope"));
    assert_eq!(line(&right, "then r.mirrored = config.nope").0, Verdict::Unspecified);
}

#[test]
fn a_given_switch_is_listed_as_written() {
    let result = walked(&journey(
        "        config.lfs_enabled = r.lfs_used",
        "        r does UseLfs(r) on Settings",
    ));
    // What the author wrote, not the `unknown` it evaluated to.
    assert_eq!(stipulated(&result), vec![("config.lfs_enabled = r.lfs_used".to_owned(), None)]);
    // And the reason the rule is undecided says what happened: it was set, to
    // something undecided. "No default, and nothing set it" would be false on
    // both counts.
    let (verdict, why) = line(&result, "r does UseLfs(r) on Settings");
    assert_eq!(verdict, Verdict::Undecided, "{why}");
    assert!(why.contains("itself undecided"), "{why}");
    assert!(!why.contains("nothing set it"), "{why}");
}

#[test]
fn a_switch_written_twice_is_listed_each_time_it_was_said() {
    // Each line is something the journey was told, so each is listed — the
    // same as two `stipulate` lines would be. A reader seeing one line for two
    // would not know the second was there.
    let result = walked(&journey(
        "        config.max_mirrors = 1
        config.max_mirrors = 1",
        "",
    ));
    assert_eq!(result.stipulated.len(), 2, "{:#?}", result.stipulated);
}

#[test]
fn a_switch_overridden_under_another_spelling_is_still_an_override() {
    let result = walked(
        "world {
    given:
        config.lfs_enabled = true
}

journey J {
    cast:
        r: Repo
    given:
        switches/config.lfs_enabled = false
    1. the step
        r cannot do UseLfs(r) on Settings
}",
    );
    let line = result
        .inherited
        .iter()
        .find(|line| line.said == "config.lfs_enabled = true")
        .expect("the world's line is listed");
    assert!(line.overridden, "one switch, two spellings: {:#?}", result.inherited);
}
