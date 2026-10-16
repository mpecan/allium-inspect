//! Rules woken by one trigger read the world as the trigger found it
//! (issue #24).
//!
//! `CreateIfMissing` and `ReplaceIfPresent` both listen for `Put` and rule each
//! other out. Run in a sequence, the second saw the box the first had just
//! made and ran on it too. `Stamp` and `Tally` both react to `Touch`; the
//! second used to be refused by what the first wrote. `PaintRed` and
//! `PaintBlue` both write one field and disagree, which is undecided rather
//! than whichever happened to run last.
//!
//! Against the recorded `allium` output for the `siblings` fixture.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{
    Disposition, Effect, EntityId, RuleOutcome, StepOutcome, Value, seed::seed, step::Sources,
    step::step, world::Event, world::World,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The recorded `siblings` fixture, ingested exactly as the server does.
fn recorded() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let path = root.join("specs/siblings.allium");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/siblings.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    let text = read(&path);
    let sources: Sources = BTreeMap::from([("siblings".to_owned(), text.clone())]);
    let reader = MemoryReader::default().with(&path, text);
    let Ingestion { graph, program } = ingest(&runner, &reader, &[path]).expect("ingests");
    (graph, program, sources)
}

fn fired(world: &World, event: &Event) -> StepOutcome {
    let (graph, program, sources) = recorded();
    step(&graph, &program, &sources, world, event)
}

fn rule<'a>(outcome: &'a StepOutcome, name: &str) -> &'a RuleOutcome {
    outcome.rules.iter().find(|it| it.name == name).unwrap_or_else(|| panic!("no rule {name}"))
}

/// A world holding one box named `x`.
fn shelf() -> (World, EntityId) {
    let (graph, _, _) = recorded();
    let mut world = seed(&graph);
    let boxed = world.create("Box", "siblings");
    world.set_field(&boxed, "name", Value::Str("x".to_owned()));
    (world, boxed)
}

fn put(name: &str) -> Event {
    Event::new("Put", "siblings").with("name", Value::Str(name.to_owned()))
}

fn names(world: &World) -> Vec<Value> {
    world.instances_of("Box").map(|instance| instance.field("name")).collect()
}

/// The reproduction from the issue: a new name creates, and the replacement
/// does not see the box the creation made a moment earlier.
#[test]
fn a_new_name_is_created_and_not_also_replaced() {
    let (world, _) = shelf();
    let outcome = fired(&world, &put("y"));
    assert_eq!(rule(&outcome, "CreateIfMissing").disposition, Disposition::Fired);
    let replace = rule(&outcome, "ReplaceIfPresent");
    assert_eq!(replace.disposition, Disposition::Refused, "{replace:#?}");
    assert!(replace.effects.is_empty() && replace.unresolved.is_empty(), "{replace:#?}");
    assert_eq!(
        names(&outcome.world),
        vec![Value::Str("x".to_owned()), Value::Str("y".to_owned())],
        "one box made, none replaced: {outcome:#?}"
    );
}

/// The other half of the choice: a name already there is replaced, and the
/// creation is refused — exactly one runs either way.
#[test]
fn a_name_already_there_is_replaced_and_not_also_created() {
    let (world, boxed) = shelf();
    let outcome = fired(&world, &put("x"));
    assert_eq!(rule(&outcome, "CreateIfMissing").disposition, Disposition::Refused);
    assert_eq!(rule(&outcome, "ReplaceIfPresent").disposition, Disposition::Fired);
    assert!(outcome.world.instance(&boxed).is_none(), "{outcome:#?}");
    assert_eq!(names(&outcome.world), vec![Value::Str("x".to_owned())]);
}

fn lamp(fields: &[(&str, Value)]) -> (World, EntityId) {
    let (graph, _, _) = recorded();
    let mut world = seed(&graph);
    let lamp = world.create("Lamp", "siblings");
    for (field, value) in fields {
        world.set_field(&lamp, field, value.clone());
    }
    (world, lamp)
}

/// Two independent reactions: the second is read in the world the trigger
/// found, not refused by what the first wrote, and both writes stand.
#[test]
fn independent_reactions_both_run_on_the_world_the_trigger_found() {
    let (world, lamp) = lamp(&[("touched", Value::Bool(false)), ("touches", Value::Int(0))]);
    let event = Event::new("Touch", "siblings").with("lamp", Value::Ref(lamp.clone()));
    let outcome = fired(&world, &event);
    assert_eq!(rule(&outcome, "Stamp").disposition, Disposition::Fired);
    let tally = rule(&outcome, "Tally");
    assert_eq!(tally.disposition, Disposition::Fired, "{tally:#?}");
    assert!(tally.unapplied.is_empty(), "{tally:#?}");
    let after = outcome.world.instance(&lamp).unwrap();
    assert_eq!(after.field("touched"), Value::Bool(true));
    assert_eq!(after.field("touches"), Value::Int(1));
}

/// Two siblings that write one field differently: which stands is not the
/// spec's to say, so the field is left unknown and the clash names both.
#[test]
fn siblings_that_disagree_about_a_field_leave_it_undecided() {
    let (world, lamp) = lamp(&[("colour", Value::Str("white".to_owned()))]);
    let event = Event::new("Paint", "siblings").with("lamp", Value::Ref(lamp.clone()));
    let outcome = fired(&world, &event);
    let fired_both = ["PaintBlue", "PaintRed"]
        .iter()
        .all(|name| rule(&outcome, name).disposition == Disposition::Fired);
    assert!(fired_both, "{outcome:#?}");
    let said: Vec<_> = outcome.rules.iter().flat_map(|rule| &rule.unapplied).collect();
    assert_eq!(said.len(), 1, "{outcome:#?}");
    let reason = &said[0].reason;
    assert!(
        reason.contains("`PaintBlue` and `PaintRed` were both woken by `Paint`")
            && reason.contains("does not say which of the two runs first"),
        "{reason}"
    );
    assert_eq!(outcome.world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
    assert!(outcome.has_unknowns());
    // Each still says what it wrote: the rule's own effect is not the clash.
    assert!(rule(&outcome, "PaintRed").effects.iter().any(|effect| matches!(
        effect,
        Effect::Assigned { to: Value::Str(colour), .. } if colour == "red"
    )));
}

fn on(trigger: &str, binding: &str, id: &EntityId) -> Event {
    Event::new(trigger, "siblings").with(binding, Value::Ref(id.clone()))
}

/// What a sibling's postcondition reads is read in the world the trigger
/// found too: `Shade` copies the colour the lamp had, not the one `DyeRed`
/// wrote a moment earlier — which it would only see because it sorts later.
#[test]
fn a_postcondition_reads_the_world_the_trigger_found() {
    let (world, lamp) = lamp(&[
        ("colour", Value::Str("white".to_owned())),
        ("shade", Value::Str("white".to_owned())),
    ]);
    let outcome = fired(&world, &on("Dye", "lamp", &lamp));
    let shade = rule(&outcome, "Shade");
    assert_eq!(shade.disposition, Disposition::Fired, "{shade:#?}");
    assert!(shade.unapplied.is_empty(), "{shade:#?}");
    let after = outcome.world.instance(&lamp).unwrap();
    assert_eq!(after.field("colour"), Value::Str("red".to_owned()));
    assert_eq!(after.field("shade"), Value::Str("white".to_owned()));
}

/// Two siblings disagreed, and a third agreeing with the first does not
/// settle it: the field stays unknown, and the third says so too.
#[test]
fn a_third_sibling_does_not_settle_what_two_disagreed_about() {
    let (world, lamp) = lamp(&[("colour", Value::Str("white".to_owned()))]);
    let outcome = fired(&world, &on("Repaint", "lamp", &lamp));
    assert_eq!(outcome.world.instance(&lamp).unwrap().field("colour"), Value::Unknown);
    let third = rule(&outcome, "RepaintC");
    assert_eq!(third.unapplied.len(), 1, "{third:#?}");
    assert!(
        third.unapplied[0].reason.contains("`RepaintA` and `RepaintB` had already disagreed"),
        "{:#?}",
        third.unapplied
    );
    // And a later step reading it is undecided, saying why: the clash itself,
    // kept with the field, rather than only that something did not settle it
    // (issue #27).
    let admired = fired(&outcome.world, &on("Admire", "lamp", &lamp));
    let admire = rule(&admired, "Admire");
    assert_eq!(admire.disposition, Disposition::Undecided, "{admire:#?}");
    assert!(
        admire
            .unresolved
            .iter()
            .any(|it| it.reason.contains("`RepaintA` and `RepaintB` had already disagreed")),
        "{admire:#?}"
    );
}

/// A refusal from the value the trigger found is the rule's own, whichever
/// sibling went first: `KnockToLose` is refused from `draft` as it would be
/// from `open`, and `KnockOpen`'s write stands.
#[test]
fn a_refusal_from_the_trigger_time_value_is_the_rules_own() {
    let (graph, _, _) = recorded();
    let mut world = seed(&graph);
    let door = world.create("Door", "siblings");
    world.set_field(&door, "status", Value::Enum("draft".to_owned()));
    let outcome = fired(&world, &on("Knock", "door", &door));
    let lose = rule(&outcome, "KnockToLose");
    assert!(lose.unapplied.is_empty(), "{lose:#?}");
    assert!(
        lose.effects
            .iter()
            .any(|effect| matches!(effect, Effect::Refused { from, .. } if from == "draft")),
        "{lose:#?}"
    );
    assert_eq!(
        outcome.world.instance(&door).unwrap().field("status"),
        Value::Enum("open".to_owned())
    );
    // Two moves the lifecycle permits, to different states, do clash.
    let slammed = fired(&world, &on("Slam", "door", &door));
    assert_eq!(slammed.world.instance(&door).unwrap().field("status"), Value::Unknown);
    assert_eq!(rule(&slammed, "SlamShut").unapplied.len(), 1, "{slammed:#?}");
}

/// A removal and an assignment to what it removed come out the same in
/// either order: `Bin` sorts before `Label`, `MarkTossed` before `TossOut`.
#[test]
fn a_removal_and_an_assignment_to_it_agree_in_either_order() {
    for (trigger, rules) in [("Junk", ["Bin", "Label"]), ("Toss", ["MarkTossed", "TossOut"])] {
        let (world, lamp) = lamp(&[("tossed", Value::Bool(false))]);
        let outcome = fired(&world, &on(trigger, "lamp", &lamp));
        for name in rules {
            let it = rule(&outcome, name);
            assert_eq!(it.disposition, Disposition::Fired, "{it:#?}");
            assert!(it.unapplied.is_empty(), "{trigger}: {it:#?}");
        }
        assert!(outcome.world.instance(&lamp).is_none(), "{trigger}: {outcome:#?}");
    }
}

/// Two state rules woken by one firing: `BFlag`'s condition is read in the
/// world the trigger found, not the one `ALapse` left with the hold lapsed.
#[test]
fn state_rules_woken_together_read_the_world_the_trigger_found() {
    let (graph, _, _) = recorded();
    let mut world = seed(&graph);
    let hold = world.create("Hold", "siblings");
    world.set_field(&hold, "status", Value::Enum("active".to_owned()));
    world.set_field(&hold, "ends_at", Value::Timestamp(0));
    world.set_field(&hold, "flagged", Value::Bool(false));
    world.now = 3_600_000;
    let outcome = fired(&world, &on("Hold", "h", &hold));
    for name in ["ALapse", "BFlag"] {
        let it = rule(&outcome, name);
        assert_eq!(it.disposition, Disposition::Fired, "{it:#?}");
    }
    let after = outcome.world.instance(&hold).unwrap();
    assert_eq!(after.field("flagged"), Value::Bool(true));
    assert_eq!(after.field("status"), Value::Enum("lapsed".to_owned()));
}

/// Two reactions to one creation: `BLabel` is read about the crate the
/// creation made, though `AScrap` removed it a moment earlier.
#[test]
fn creation_reactions_woken_together_read_the_world_the_trigger_found() {
    let (graph, _, _) = recorded();
    let mut world = seed(&graph);
    let junk = world.create("Crate", "siblings");
    world.set_field(&junk, "name", Value::Str("junk".to_owned()));
    world.set_field(&junk, "labelled", Value::Bool(false));
    let outcome =
        fired(&world, &Event::new("Crate.created", "siblings").passing(Value::Ref(junk.clone())));
    for name in ["AScrap", "BLabel"] {
        let it = rule(&outcome, name);
        assert_eq!(it.disposition, Disposition::Fired, "{name}: {outcome:#?}");
        assert!(it.unapplied.is_empty() && it.unresolved.is_empty(), "{it:#?}");
    }
    assert!(outcome.world.instance(&junk).is_none(), "{outcome:#?}");
}
