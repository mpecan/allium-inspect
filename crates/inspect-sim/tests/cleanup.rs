//! A state rule woken about an instance a sibling in the same step removes
//! (issue #22).
//!
//! `Cleanup` and `TimesOut` both watch a `Delivery`, so the one event that
//! fires the first reaches the second as well — after the delivery is gone.
//! A delivery that no longer exists has no status to be overdue on, so
//! `TimesOut` does not apply: refused, decided, and saying which rule removed
//! it. A reference to something that was never in the world is a different
//! matter and stays undecided, with its reason.
//!
//! Against the recorded `allium` output for the `cleanup` fixture.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{
    Disposition, EntityId, RuleOutcome, StepOutcome, Truth, Value, seed::seed, step::Sources,
    step::step, world::Event, world::World,
};

const DAY: i64 = 24 * 3_600_000;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The recorded `cleanup` fixture's graph and program, for `sources` as
/// edited — read by the library, as `parse` and `analyse` are.
fn graph_with(sources: &Sources) -> (SpecGraph, Program) {
    let root = fixtures();
    let path = root.join("specs/cleanup.allium");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/cleanup.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    let reader = MemoryReader::default().with(&path, sources["cleanup"].clone());
    let Ingestion { graph, program } = ingest(&runner, &reader, &[path]).expect("ingests");
    (graph, program)
}

/// The recorded `cleanup` fixture, ingested exactly as the server does.
fn recorded() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let path = root.join("specs/cleanup.allium");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/cleanup.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    let text = read(&path);
    let sources: Sources = BTreeMap::from([("cleanup".to_owned(), text.clone())]);
    let reader = MemoryReader::default().with(&path, text);
    let Ingestion { graph, program } = ingest(&runner, &reader, &[path]).expect("ingests");
    (graph, program, sources)
}

/// A delivery that finished at zero and was never stopped, with `status` as
/// given, eight days on.
fn world(graph: &SpecGraph, status: &str) -> (World, EntityId) {
    let mut world = seed(graph);
    let delivery = world.create("Delivery", "cleanup");
    world.set_field(&delivery, "status", Value::Enum(status.to_owned()));
    world.set_field(&delivery, "finished_at", Value::Timestamp(0));
    world.set_field(&delivery, "stopped_at", Value::Null);
    world.now = 8 * DAY;
    (world, delivery)
}

fn fired(world: &World, about: &EntityId) -> StepOutcome {
    let (graph, program, sources) = recorded();
    let event = Event::new("Delivery", "cleanup").with("d", Value::Ref(about.clone()));
    step(&graph, &program, &sources, world, &event)
}

fn rule<'a>(outcome: &'a StepOutcome, name: &str) -> &'a RuleOutcome {
    outcome.rules.iter().find(|it| it.name == name).unwrap_or_else(|| panic!("no rule {name}"))
}

/// The reproduction from the issue, one step down: the delivery is gone by the
/// time `TimesOut` reads its condition, and that is a decided *no*.
#[test]
fn a_rule_about_an_instance_removed_earlier_in_the_step_is_refused() {
    let (graph, _, _) = recorded();
    let (world, delivery) = world(&graph, "done");
    let outcome = fired(&world, &delivery);
    assert_eq!(rule(&outcome, "Cleanup").disposition, Disposition::Fired, "{outcome:#?}");
    assert!(outcome.world.instance(&delivery).is_none(), "{outcome:#?}");

    let times_out = rule(&outcome, "TimesOut");
    assert_eq!(times_out.disposition, Disposition::Refused, "{times_out:#?}");
    assert!(times_out.unresolved.is_empty(), "a decided no carries no unknown: {times_out:#?}");
    assert!(times_out.effects.is_empty(), "{times_out:#?}");
    // Its `when` and nothing after it: `requires: d.finished_at < now` is not
    // read about a delivery that is not there.
    assert_eq!(times_out.requires.len(), 1, "{times_out:#?}");
    assert!(!outcome.has_unknowns(), "{outcome:#?}");
}

/// The refusal says what refused it, against the `when` as the author wrote
/// it, and names the rule that removed the instance.
#[test]
fn the_refusal_names_the_removed_instance_and_the_rule_that_removed_it() {
    let (graph, _, _) = recorded();
    let (world, delivery) = world(&graph, "done");
    let outcome = fired(&world, &delivery);
    let times_out = rule(&outcome, "TimesOut");
    let when = times_out.requires.first().expect("the when");
    assert_eq!(when.truth, Truth::False, "{times_out:#?}");
    assert_eq!(when.text, "d: Delivery.is_overdue");
    let note = when.unresolved.first().expect("the refusal says why");
    assert_eq!(note.expression.as_deref(), Some(when.text.as_str()));
    assert!(note.reason.contains("`Delivery#1`"), "{}", note.reason);
    assert!(note.reason.contains("`Cleanup`"), "{}", note.reason);

    // Located by byte, in a source with an em-dash above the rule: the span
    // covers exactly the clause the note quotes.
    let (_, _, sources) = recorded();
    let source = &sources["cleanup"];
    assert!(!source.is_ascii(), "the fixture must not be all ASCII");
    let span = note.span.expect("the refusal is located");
    assert_eq!(span.start, source.find("d: Delivery.is_overdue").expect("in the source"));
    assert_eq!(&source[span.start..span.end], "d: Delivery.is_overdue");
}

/// Both due at once: the delivery was overdue when the event woke both rules,
/// and `Cleanup` happens to run first. The spec does not say which does, so
/// whether `TimesOut` runs is undecided — and says which two rules raced —
/// rather than a refusal that renaming either rule would flip.
#[test]
fn a_timeout_due_beside_the_cleanup_that_removed_it_is_undecided() {
    let (graph, _, _) = recorded();
    let (world, delivery) = world(&graph, "sending");
    let outcome = fired(&world, &delivery);
    assert_eq!(rule(&outcome, "Cleanup").disposition, Disposition::Fired);
    assert!(outcome.world.instance(&delivery).is_none(), "{outcome:#?}");

    let times_out = rule(&outcome, "TimesOut");
    assert_eq!(times_out.disposition, Disposition::Undecided, "{times_out:#?}");
    assert!(times_out.effects.is_empty(), "{times_out:#?}");
    assert_eq!(times_out.requires.len(), 1, "{times_out:#?}");
    let reason = &times_out.unresolved.first().expect("it says why").reason;
    assert!(
        reason.contains("`TimesOut` was due on `Delivery#1`")
            && reason.contains("`Cleanup` removed it")
            && reason.contains("does not say which of the two runs first"),
        "{reason}"
    );
}

/// `Alert` is due exactly when `TimesOut` is, and sorts before `Cleanup`, so
/// the step reads it while the delivery is still in the world it is building.
/// The same race, so the same answer: undecided, nothing applied — and not
/// fired, which is what reading the answer off the order gave.
#[test]
fn a_rule_read_before_the_cleanup_that_removes_its_instance_races_it_all_the_same() {
    let (graph, program, mut sources) = recorded();
    let (world, delivery) = world(&graph, "sending");
    let event = Event::new("Delivery", "cleanup").with("d", Value::Ref(delivery.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let alert = rule(&outcome, "Alert");
    assert_eq!(alert.disposition, Disposition::Undecided, "{alert:#?}");
    assert!(alert.effects.is_empty(), "{alert:#?}");
    let reason = &alert.unresolved.first().expect("it says why").reason;
    assert!(
        reason.contains("`Alert` was due on `Delivery#1`")
            && reason.contains("`Cleanup` removed it"),
        "{reason}"
    );
    assert_eq!(
        rule(&outcome, "TimesOut").unresolved.first().map(|it| it.reason.replace("TimesOut", "")),
        Some(reason.replace("Alert", "")),
        "both sides of the cleanup are told the same thing"
    );

    // Rename `Alert` so it sorts after `Cleanup`, as `TimesOut` does: the
    // same verdict and the same words.
    let text = sources["cleanup"].replace("rule Alert {", "rule Zalert {");
    sources.insert("cleanup".to_owned(), text);
    let renamed = graph_with(&sources);
    let outcome = step(&renamed.0, &renamed.1, &sources, &world, &event);
    let zalert = rule(&outcome, "Zalert");
    assert_eq!(zalert.disposition, Disposition::Undecided, "{zalert:#?}");
    assert_eq!(
        zalert.unresolved.first().map(|it| it.reason.replace("Zalert", "Alert")),
        Some(reason.clone())
    );
}

/// The rule that removes the instance itself is not racing anybody for it:
/// `Cleanup` fires whatever else watched the delivery.
#[test]
fn the_rule_that_removes_it_is_read_as_usual() {
    let (graph, _, _) = recorded();
    let (world, delivery) = world(&graph, "sending");
    let outcome = fired(&world, &delivery);
    assert_eq!(rule(&outcome, "Cleanup").disposition, Disposition::Fired, "{outcome:#?}");
    assert!(outcome.world.instance(&delivery).is_none());
}

/// `Zombie` reads a deadline that is not there: never due, so refused beside
/// the cleanup, decided, rather than raced. Read with the ordinary reading of
/// a condition, `null + config.keep <= now` is undecided and it would race.
#[test]
fn a_null_deadline_beside_the_cleanup_was_never_due() {
    let (graph, _, _) = recorded();
    let (world, delivery) = world(&graph, "done");
    let outcome = fired(&world, &delivery);
    let zombie = rule(&outcome, "Zombie");
    assert_eq!(zombie.disposition, Disposition::Refused, "{zombie:#?}");
    assert!(zombie.unresolved.is_empty(), "{zombie:#?}");
    let note = &zombie.requires[0].unresolved.first().expect("it says why").reason;
    assert!(note.contains("removed in this step, by `Cleanup`"), "{note}");
}

/// The event that woke `Expire` carries the lease as `e`; `Lapse` calls it
/// `l`. It is still the lease the event is about, so `Lapse` is refused with
/// the removal, not left reading its condition about nothing.
#[test]
fn a_rule_naming_the_removed_instance_differently_is_refused() {
    let (graph, program, sources) = recorded();
    let mut world = seed(&graph);
    let lease = world.create("Lease", "cleanup");
    world.set_field(&lease, "ends_at", Value::Timestamp(0));
    world.set_field(&lease, "renewed", Value::Bool(true));
    world.set_field(&lease, "lapsed", Value::Bool(false));
    world.now = DAY;
    let event = Event::new("Lease", "cleanup").with("e", Value::Ref(lease.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    assert_eq!(rule(&outcome, "Expire").disposition, Disposition::Fired, "{outcome:#?}");
    let lapse = rule(&outcome, "Lapse");
    assert_eq!(lapse.disposition, Disposition::Refused, "{lapse:#?}");
    let note = lapse.requires[0].unresolved.first().expect("it says why");
    assert!(note.reason.contains("by `Expire`"), "{}", note.reason);
    assert!(!outcome.has_unknowns(), "{outcome:#?}");
}

/// The control: an event about a delivery that was never in this world is not
/// one anything removed, and whether a rule applies to it stays undecided,
/// with the reason naming what is missing.
#[test]
fn a_reference_to_something_never_in_the_world_stays_undecided() {
    let (graph, _, _) = recorded();
    let (world, _) = world(&graph, "done");
    let stranger = EntityId::new("Delivery", 9);
    let outcome = fired(&world, &stranger);
    let times_out = rule(&outcome, "TimesOut");
    assert_eq!(times_out.disposition, Disposition::Undecided, "{times_out:#?}");
    assert!(
        times_out
            .unresolved
            .iter()
            .any(|it| it.reason.contains("`Delivery#9` is not in this world")),
        "{times_out:#?}"
    );
}
