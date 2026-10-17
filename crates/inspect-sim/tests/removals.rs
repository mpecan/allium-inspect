//! A rule that removes an instance and reads it in another `ensures`
//! (issue #33).
//!
//! `ensures` clauses are a set of postconditions about the state the rule
//! leaves, not a sequence. `not exists upload` written above
//! `Blob.created(name: upload.name)` left the read undecided, and the same two
//! lines swapped let it through, so the verdict was a fact about line order.
//! Now a value the rule leaves behind — a creation's parameter, an emission's
//! argument, an assigned value, a `let` inside `ensures` — reads a removed
//! instance as gone in either order, and says how to keep what it held. What
//! decides which postconditions apply — an `if`, a `for`, what a `not exists`
//! is about — reads it as the rule found it.
//!
//! Against the recorded `allium` output for the `uploads` fixture.

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

/// The recorded `uploads` fixture, ingested exactly as the server does.
fn recorded() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let path = root.join("specs/uploads.allium");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/uploads.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    let text = read(&path);
    let sources: Sources = BTreeMap::from([("uploads".to_owned(), text.clone())]);
    let reader = MemoryReader::default().with(&path, text);
    let Ingestion { graph, program } = ingest(&runner, &reader, &[path]).expect("ingests");
    (graph, program, sources)
}

/// A world holding one upload called "x", with `big` as given and a thumb.
fn world(graph: &SpecGraph, big: bool) -> (World, EntityId, EntityId) {
    let mut world = seed(graph);
    let thumb = world.create("Thumb", "uploads");
    world.set_field(&thumb, "name", Value::Str("t".to_owned()));
    let upload = world.create("Upload", "uploads");
    world.set_field(&upload, "name", Value::Str("x".to_owned()));
    world.set_field(&upload, "big", Value::Bool(big));
    world.set_field(&upload, "thumb", Value::Ref(thumb.clone()));
    (world, upload, thumb)
}

/// `trigger(upload)` on a world whose upload is big or not.
fn fired(trigger: &str, big: bool) -> (StepOutcome, EntityId, EntityId) {
    let (graph, program, sources) = recorded();
    let (world, upload, thumb) = world(&graph, big);
    let event = Event::new(trigger, "uploads").with("upload", Value::Ref(upload.clone()));
    (step(&graph, &program, &sources, &world, &event), upload, thumb)
}

fn rule<'a>(outcome: &'a StepOutcome, name: &str) -> &'a RuleOutcome {
    outcome.rules.iter().find(|it| it.name == name).unwrap_or_else(|| panic!("no rule {name}"))
}

/// The one blob the step made, and what its `name` was set to.
fn blob_name(outcome: &StepOutcome) -> Value {
    let blobs: Vec<_> = outcome.world.entities.values().filter(|it| it.entity == "Blob").collect();
    assert_eq!(blobs.len(), 1, "{outcome:#?}");
    blobs[0].field("name")
}

/// Every reason the rule gave.
fn reasons(rule: &RuleOutcome) -> Vec<String> {
    rule.unresolved.iter().map(|it| it.reason.clone()).collect()
}

/// The note a read of something the rule removes gives.
fn removal_note(rule: &RuleOutcome) -> &inspect_sim::Unresolved {
    rule.unresolved
        .iter()
        .find(|it| it.reason.contains("has nothing to read in the state it leaves"))
        .unwrap_or_else(|| panic!("no note about the removal: {:#?}", reasons(rule)))
}

/// The reason a read of the removed upload gives: which rule removed it, by
/// which clause, why that matters, and where a `let` keeps what it held — and
/// that the act says so, as something the rule ensures and did not apply.
fn says_it_was_removed_here(rule: &RuleOutcome, read: &str) {
    let note = removal_note(rule);
    let reason = &note.reason;
    assert!(
        reason.contains(&format!("rule `{}` removes `upload`", rule.name))
            && reason.contains("with `not exists upload`")
            && reason.contains(&format!("so `{read}` has nothing to read"))
            && reason.contains("whichever line comes first")
            && reason.contains(&format!("bind `let name = {read}` beside the `when`"))
            && reason.contains("a `let` inside `ensures` reads the state the rule leaves too"),
        "{reason}"
    );
    assert_eq!(note.expression.as_deref(), Some(read), "{note:#?}");
    assert!(rule.unapplied.contains(note), "the act says so too: {rule:#?}");
}

/// The issue's reproduction: removed, then read. The read is undecided and
/// says why — not "is not in this world", which named no clause and no fix.
#[test]
fn a_creation_reading_what_the_rule_removed_above_it_is_undecided_and_says_how_to_keep_it() {
    let (outcome, upload, _) = fired("CompleteRemovingFirst", false);
    let complete = rule(&outcome, "CompleteRemovingFirst");
    assert_eq!(complete.disposition, Disposition::Fired, "{complete:#?}");
    assert!(outcome.world.instance(&upload).is_none(), "{outcome:#?}");
    assert_eq!(blob_name(&outcome), Value::Unknown);
    says_it_was_removed_here(complete, "upload.name");
    assert!(
        !reasons(complete).iter().any(|it| it.contains("is not in this world")),
        "{:#?}",
        reasons(complete)
    );
}

/// The same two lines swapped: the same answer. This one used to hold.
#[test]
fn the_same_read_written_above_the_removal_gets_the_same_answer() {
    let (first, ..) = fired("CompleteRemovingFirst", false);
    let (second, upload, _) = fired("CompleteReadingFirst", false);
    let reading = rule(&second, "CompleteReadingFirst");
    assert_eq!(reading.disposition, Disposition::Fired, "{reading:#?}");
    assert!(second.world.instance(&upload).is_none(), "{second:#?}");
    assert_eq!(blob_name(&second), Value::Unknown, "{second:#?}");
    says_it_was_removed_here(reading, "upload.name");
    let named = |outcome: &RuleOutcome| {
        reasons(outcome).into_iter().map(|it| it.replace(&outcome.name, "_")).collect::<Vec<_>>()
    };
    assert_eq!(
        named(reading),
        named(rule(&first, "CompleteRemovingFirst")),
        "the two orders say the same thing"
    );
    let fields = |outcome: &StepOutcome| {
        outcome
            .world
            .entities
            .values()
            .map(|it| (it.id.clone(), it.fields.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(fields(&second), fields(&first), "and leave the same world");
}

/// The way to keep the name: a `let` beside the `when`, read before the rule
/// changes anything.
#[test]
fn a_name_bound_before_the_rule_is_kept() {
    let (outcome, upload, _) = fired("CompleteKeepingTheName", false);
    let keeping = rule(&outcome, "CompleteKeepingTheName");
    assert_eq!(keeping.disposition, Disposition::Fired, "{keeping:#?}");
    assert!(keeping.unresolved.is_empty(), "{keeping:#?}");
    assert!(outcome.world.instance(&upload).is_none());
    assert_eq!(blob_name(&outcome), Value::Str("x".to_owned()));
}

/// An assigned value is a value the rule leaves behind as well, read above
/// the removal.
#[test]
fn an_assigned_value_read_from_what_the_rule_removes_is_undecided() {
    let (graph, program, sources) = recorded();
    let (mut world, upload, _) = world(&graph, false);
    let shelf = world.create("Shelf", "uploads");
    world.set_field(&shelf, "last", Value::Str("none".to_owned()));
    let event = Event::new("Archive", "uploads")
        .with("upload", Value::Ref(upload.clone()))
        .with("shelf", Value::Ref(shelf.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let archive = rule(&outcome, "Archive");
    assert_eq!(archive.disposition, Disposition::Fired, "{archive:#?}");
    assert!(outcome.world.instance(&upload).is_none());
    let last = outcome.world.instance(&shelf).expect("the shelf stays").field("last");
    assert_eq!(last, Value::Unknown, "{outcome:#?}");
    says_it_was_removed_here(archive, "upload.name");
}

/// An emission's argument: the listener is handed nothing settled, and the
/// emitting rule says why.
#[test]
fn an_emission_argument_read_from_what_the_rule_removes_is_undecided() {
    let (outcome, ..) = fired("Discard", false);
    let discard = rule(&outcome, "Discard");
    assert_eq!(discard.disposition, Disposition::Fired, "{discard:#?}");
    let carried = discard.effects.iter().find_map(|effect| match effect {
        Effect::Emitted { trigger, arguments, .. } if trigger == "UploadDiscarded" => {
            arguments.get("name").cloned()
        }
        _ => None,
    });
    assert_eq!(carried, Some(Value::Unknown), "{discard:#?}");
    says_it_was_removed_here(discard, "upload.name");
}

/// A `let` written inside `ensures` reads the state the rule leaves too.
#[test]
fn a_let_inside_ensures_reading_what_the_rule_removes_is_undecided() {
    let (outcome, ..) = fired("Shred", false);
    let shred = rule(&outcome, "Shred");
    assert_eq!(blob_name(&outcome), Value::Unknown, "{outcome:#?}");
    says_it_was_removed_here(shred, "upload.name");
}

/// What decides whether to remove it reads it as the rule found it: a small
/// upload stays and its name is read; a big one goes and its name is not.
#[test]
fn a_condition_deciding_the_removal_reads_the_instance_as_the_rule_found_it() {
    let (small, upload, _) = fired("Tidy", false);
    let tidy = rule(&small, "Tidy");
    assert_eq!(tidy.disposition, Disposition::Fired, "{tidy:#?}");
    assert!(tidy.unresolved.is_empty(), "{tidy:#?}");
    assert!(tidy.unapplied.is_empty(), "{tidy:#?}");
    assert!(small.world.instance(&upload).is_some());
    assert_eq!(blob_name(&small), Value::Str("x".to_owned()));

    let (big, upload, _) = fired("Tidy", true);
    let tidy = rule(&big, "Tidy");
    assert!(!skipped(tidy), "the `if` was decided: {tidy:#?}");
    assert!(big.world.instance(&upload).is_none(), "{big:#?}");
    assert_eq!(blob_name(&big), Value::Unknown);
    says_it_was_removed_here(tidy, "upload.name");
}

/// Whether the rule left a conditional undecided.
fn skipped(rule: &RuleOutcome) -> bool {
    rule.unapplied.iter().any(|it| it.reason.contains("was skipped"))
}

/// What a `not exists` is about is read as the rule found it: the thumb goes
/// whichever of the two lines comes first.
#[test]
fn what_a_removal_is_about_is_read_as_the_rule_found_it_in_either_order() {
    for trigger in ["ClearRemovingFirst", "ClearReadingFirst"] {
        let (outcome, upload, thumb) = fired(trigger, false);
        let clear = rule(&outcome, trigger);
        assert_eq!(clear.disposition, Disposition::Fired, "{trigger}: {clear:#?}");
        assert!(clear.unapplied.is_empty(), "{trigger}: {clear:#?}");
        assert!(clear.unresolved.is_empty(), "{trigger}: {clear:#?}");
        assert!(outcome.world.instance(&upload).is_none(), "{trigger}: {outcome:#?}");
        assert!(outcome.world.instance(&thumb).is_none(), "{trigger}: {outcome:#?}");
    }
}

/// A removal one pass cannot see: `if upload.big` sits below `not exists
/// upload`, so read in line order the upload is gone and the thumb stays. Read
/// as the rule found it the upload is big and the thumb goes — and then the
/// thumb's name, read on the first line of all, is gone with it. Told only
/// what one pass removed, that read went through.
#[test]
fn a_removal_decided_from_what_another_line_removes_is_found_before_anything_is_read() {
    let (graph, program, sources) = recorded();
    let (world, upload, thumb) = world(&graph, true);
    let event = Event::new("Sweep", "uploads")
        .with("upload", Value::Ref(upload.clone()))
        .with("thumb", Value::Ref(thumb.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let sweep = rule(&outcome, "Sweep");
    assert_eq!(sweep.disposition, Disposition::Fired, "{sweep:#?}");
    assert!(!skipped(sweep), "the `if` was decided: {sweep:#?}");
    assert!(outcome.world.instance(&upload).is_none(), "{outcome:#?}");
    assert!(outcome.world.instance(&thumb).is_none(), "{outcome:#?}");
    assert_eq!(blob_name(&outcome), Value::Unknown, "{outcome:#?}");
    let note = removal_note(sweep);
    assert!(
        note.reason.contains("removes `thumb` (`Thumb#1`) with `not exists thumb`"),
        "{}",
        note.reason
    );
    assert!(note.reason.contains("`let name = thumb.name`"), "{}", note.reason);
}

/// `trigger(upload)` over a world whose upload has a thumb for each of
/// `names`, each one's `upload` the upload — the relationship `thumbs`.
fn fired_with_thumbs(trigger: &str, names: &[&str]) -> (StepOutcome, EntityId, Vec<EntityId>) {
    let (graph, program, sources) = recorded();
    let (mut world, upload, other) = world(&graph, false);
    // The upload's `thumb` is no member of its `thumbs`.
    world.set_field(&other, "upload", Value::Null);
    let thumbs: Vec<EntityId> = names
        .iter()
        .map(|name| {
            let thumb = world.create("Thumb", "uploads");
            world.set_field(&thumb, "name", Value::Str((*name).to_owned()));
            world.set_field(&thumb, "upload", Value::Ref(upload.clone()));
            thumb
        })
        .collect();
    let event = Event::new(trigger, "uploads").with("upload", Value::Ref(upload.clone()));
    (step(&graph, &program, &sources, &world, &event), upload, thumbs)
}

/// The one flag the step made, and what its `had` was set to.
fn flag(outcome: &StepOutcome) -> Value {
    let flags: Vec<_> = outcome.world.entities.values().filter(|it| it.entity == "Flag").collect();
    assert_eq!(flags.len(), 1, "{outcome:#?}");
    flags[0].field("had")
}

/// A removal decided through a `let` inside `ensures` is decided as `if
/// upload.big:` decides it, in either order: the `let` names a read, and what
/// decides reads the world the rule found. Read only in the state the rule
/// leaves, the removal undid the answer that made it, and which reading of
/// the rule came last decided whether the upload went — the issue's
/// line-order dependence again, one level down.
#[test]
fn a_removal_decided_through_a_let_inside_ensures_is_the_same_in_either_order() {
    for big in [true, false] {
        let mut said = Vec::new();
        for trigger in ["TidyByLetReadingFirst", "TidyByLetReadingLast"] {
            let (outcome, upload, _) = fired(trigger, big);
            let tidy = rule(&outcome, trigger);
            assert_eq!(tidy.disposition, Disposition::Fired, "{trigger}: {tidy:#?}");
            assert!(!skipped(tidy), "{trigger}: {tidy:#?}");
            assert!(
                !reasons(tidy).iter().any(|it| it.contains("is undecided: what decides it")),
                "{trigger}: it settled: {tidy:#?}"
            );
            assert_eq!(outcome.world.instance(&upload).is_none(), big, "{trigger}: {outcome:#?}");
            if big {
                assert_eq!(blob_name(&outcome), Value::Unknown, "{trigger}");
                says_it_was_removed_here(tidy, "upload.name");
            } else {
                assert_eq!(blob_name(&outcome), Value::Str("x".to_owned()), "{trigger}");
                assert!(tidy.unresolved.is_empty(), "{trigger}: {tidy:#?}");
            }
            said.push(
                reasons(tidy).into_iter().map(|it| it.replace(trigger, "_")).collect::<Vec<_>>(),
            );
        }
        assert_eq!(said[0], said[1], "big {big}: the two orders say the same thing");
    }
}

/// What decides reads the upload as the rule found it — before any of its
/// clauses ran — not as a line above the removal left it. `upload.big =
/// false` written above `not exists upload` once made the `if` above both
/// read `false`, in that order only.
#[test]
fn what_decides_reads_the_instance_as_the_rule_found_it_not_as_its_own_lines_left_it() {
    for trigger in ["FlagThenRemove", "RemoveThenFlag"] {
        let (outcome, upload, _) = fired(trigger, true);
        let flagged = rule(&outcome, trigger);
        assert_eq!(flagged.disposition, Disposition::Fired, "{trigger}: {flagged:#?}");
        assert!(flagged.unapplied.is_empty(), "{trigger}: {flagged:#?}");
        assert!(outcome.world.instance(&upload).is_none(), "{trigger}: {outcome:#?}");
        assert_eq!(blob_name(&outcome), Value::Str("big".to_owned()), "{trigger}: {outcome:#?}");
    }
}

/// An assignment through what the rule removes, to something it does not:
/// what it is about is read as the rule found it, so the thumb is renamed.
#[test]
fn an_assignment_through_a_removed_instance_reaches_what_it_does_not_remove() {
    let (outcome, upload, thumb) = fired("Retag", false);
    let retag = rule(&outcome, "Retag");
    assert_eq!(retag.disposition, Disposition::Fired, "{retag:#?}");
    assert!(retag.unapplied.is_empty() && retag.unresolved.is_empty(), "{retag:#?}");
    assert!(outcome.world.instance(&upload).is_none());
    let renamed = outcome.world.instance(&thumb).expect("the thumb stays").field("name");
    assert_eq!(renamed, Value::Str("z".to_owned()));
}

/// A `for` over a relationship of what the rule removes ranges over it as the
/// rule found it: every thumb goes, and nothing is left unapplied.
#[test]
fn a_for_over_a_removed_instances_relationship_ranges_over_it_as_the_rule_found_it() {
    let (outcome, upload, thumbs) = fired_with_thumbs("Purge", &["a", "b"]);
    let purge = rule(&outcome, "Purge");
    assert_eq!(purge.disposition, Disposition::Fired, "{purge:#?}");
    assert!(purge.unapplied.is_empty() && purge.unresolved.is_empty(), "{purge:#?}");
    assert!(outcome.world.instance(&upload).is_none());
    for thumb in &thumbs {
        assert!(outcome.world.instance(thumb).is_none(), "{thumb}: {outcome:#?}");
    }
}

/// And its filter, through the loop's name or a bare field, reads each thumb
/// as the rule found it: only the one named "a" goes.
#[test]
fn a_filter_over_a_removed_instances_relationship_reads_each_member_as_the_rule_found_it() {
    for trigger in ["PurgeA", "PurgeBareA"] {
        let (outcome, upload, thumbs) = fired_with_thumbs(trigger, &["a", "b"]);
        let purge = rule(&outcome, trigger);
        assert_eq!(purge.disposition, Disposition::Fired, "{trigger}: {purge:#?}");
        assert!(purge.unapplied.is_empty(), "{trigger}: {purge:#?}");
        assert!(outcome.world.instance(&upload).is_none());
        assert!(outcome.world.instance(&thumbs[0]).is_none(), "{trigger}: {outcome:#?}");
        assert!(outcome.world.instance(&thumbs[1]).is_some(), "{trigger}: {outcome:#?}");
    }
}

/// A relationship read in the state the rule leaves has none of what the rule
/// removes: the upload has no thumbs left, so their names are none — the same
/// answer, and the same world, whichever line comes first.
#[test]
fn a_relationship_read_in_the_state_the_rule_leaves_is_the_same_in_either_order() {
    let mut seen = Vec::new();
    for trigger in ["NamesAfter", "NamesBefore"] {
        let (outcome, _, thumbs) = fired_with_thumbs(trigger, &["a"]);
        let names = rule(&outcome, trigger);
        assert_eq!(names.disposition, Disposition::Fired, "{trigger}: {names:#?}");
        assert!(names.unapplied.is_empty() && names.unresolved.is_empty(), "{trigger}: {names:#?}");
        assert!(outcome.world.instance(&thumbs[0]).is_none(), "{trigger}: {outcome:#?}");
        seen.push(blob_name(&outcome));
    }
    assert_eq!(seen, [Value::Set(Vec::new()), Value::Set(Vec::new())]);
}

/// The same thumbs held from before the rule: a projection over instances the
/// rule removes is undecided, and says which clause removed them.
#[test]
fn a_projection_over_instances_the_rule_removes_is_undecided_and_says_why() {
    let (outcome, _, thumbs) = fired_with_thumbs("NamesHeld", &["a"]);
    let held = rule(&outcome, "NamesHeld");
    assert_eq!(blob_name(&outcome), Value::Unknown, "{outcome:#?}");
    assert!(outcome.world.instance(&thumbs[0]).is_none());
    let note = removal_note(held);
    assert!(
        note.reason.contains("rule `NamesHeld` removes `t`")
            && note.reason.contains("with `not exists t`")
            && note.reason.contains("so `thumbs.name` has nothing to read")
            && note.reason.contains("`let name = thumbs.name`"),
        "{}",
        note.reason
    );
    assert_eq!(note.expression.as_deref(), Some("thumbs.name"));
    assert!(held.unapplied.contains(note), "{held:#?}");
}

/// A function the removed upload defines, asked in the state the rule leaves:
/// undecided, saying why, and — not a field read — pointing at a `let`
/// without spelling one.
#[test]
fn a_function_asked_of_what_the_rule_removes_is_undecided_and_says_why() {
    let (outcome, ..) = fired("Ask", false);
    let ask = rule(&outcome, "Ask");
    assert_eq!(flag(&outcome), Value::Unknown, "{outcome:#?}");
    let note = removal_note(ask);
    assert!(
        note.reason.contains("rule `Ask` removes `upload`")
            && note.reason.contains("so `upload.is_named(\"x\")` has nothing to read")
            && note.reason.contains("bind it in a `let` beside the `when`"),
        "{}",
        note.reason
    );
    assert!(ask.unapplied.contains(note), "{ask:#?}");
}

/// In the state the rule leaves, what it removes does not exist — written
/// above the removal or not.
#[test]
fn what_the_rule_removes_does_not_exist_in_the_state_it_leaves() {
    let (outcome, ..) = fired("Check", false);
    let check = rule(&outcome, "Check");
    assert_eq!(flag(&outcome), Value::Bool(false), "{outcome:#?}");
    assert!(check.unresolved.is_empty(), "{check:#?}");
}

/// A value that reads what the rule removes but is settled anyway — `or
/// true` decides it — is not something the rule failed to apply. The read
/// still says why it had nothing to read, but the act is not marked.
#[test]
fn a_value_settled_whatever_the_removed_instance_held_is_not_left_unapplied() {
    let (outcome, ..) = fired("Settle", false);
    let settle = rule(&outcome, "Settle");
    assert_eq!(flag(&outcome), Value::Bool(true), "{outcome:#?}");
    assert!(settle.unapplied.is_empty(), "{settle:#?}");
    removal_note(settle);
}

/// The reason a removed read gives is said once on the rule, though it is
/// both why the value is undecided and what the act did not apply.
#[test]
fn the_removal_note_is_said_once_on_the_rule() {
    for (trigger, name) in
        [("CompleteRemovingFirst", "CompleteRemovingFirst"), ("Discard", "Discard")]
    {
        let (outcome, ..) = fired(trigger, false);
        let fired = rule(&outcome, name);
        let note = removal_note(fired);
        let said = fired.unresolved.iter().filter(|it| *it == note).count();
        assert_eq!(said, 1, "{trigger}: {fired:#?}");
        assert_eq!(fired.unapplied.iter().filter(|it| *it == note).count(), 1, "{fired:#?}");
    }
}

/// A removal decided by a value read from what it removes never settles:
/// removed, the value is undecided and nothing removes it; kept, the value
/// says to remove it. No pass's answer is taken for the rule's — whichever
/// came last decided it before — and the rule says the question is open.
#[test]
fn a_removal_that_unsettles_what_decides_it_is_undecided_and_says_so() {
    let (graph, program, sources) = recorded();
    let (mut world, upload, _) = world(&graph, false);
    let shelf = world.create("Shelf", "uploads");
    let event = Event::new("Paradox", "uploads")
        .with("upload", Value::Ref(upload.clone()))
        .with("shelf", Value::Ref(shelf.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let paradox = rule(&outcome, "Paradox");
    assert_eq!(paradox.disposition, Disposition::Fired, "{paradox:#?}");
    let open = paradox
        .unapplied
        .iter()
        .find(|it| it.reason.contains("is undecided: what decides it"))
        .unwrap_or_else(|| panic!("{paradox:#?}"));
    assert!(
        open.reason
            .contains(&format!("whether rule `Paradox` removes `{upload}` (`not exists upload`)"))
            && open.reason.contains("beside the `when`"),
        "{}",
        open.reason
    );
    assert_eq!(paradox.unresolved.first(), Some(open), "said first: {paradox:#?}");
    // What it reads of the upload is undecided, and says why.
    says_it_was_removed_here(paradox, "upload.name");
}

/// Not about removal: an emitted argument nothing settled says why on the
/// rule that emitted it — which sub-expression, where — since the listener
/// is handed only the value; a state word handed on says nothing; and neither
/// is something the rule left unapplied.
#[test]
fn an_emitted_argument_nothing_settled_says_why_on_the_emitting_rule() {
    let (graph, program, sources) = recorded();
    let mut world = seed(&graph);
    let upload = world.create("Upload", "uploads");
    let event = Event::new("Announce", "uploads").with("upload", Value::Ref(upload.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let announce = rule(&outcome, "Announce");
    assert_eq!(announce.disposition, Disposition::Fired, "{announce:#?}");
    assert!(announce.unapplied.is_empty(), "{announce:#?}");
    assert_eq!(announce.unresolved.len(), 1, "{announce:#?}");
    let note = &announce.unresolved[0];
    assert!(note.reason.contains(&format!("`{upload}` has no `name` set")), "{note:#?}");
    assert_eq!(note.expression.as_deref(), Some("upload.name"));
    let span = note.span.expect("where it is");
    assert_eq!(span.slice(&sources["uploads"]), Some("upload.name"), "by byte offset");
    let carried = announce.effects.iter().find_map(|effect| match effect {
        Effect::Emitted { arguments, .. } => Some(arguments.clone()),
        _ => None,
    });
    let carried = carried.expect("emitted");
    assert_eq!(carried.get("name"), Some(&Value::Unknown));
    assert_eq!(carried.get("state"), Some(&Value::Enum("done".to_owned())));
}

/// A loop's name is the member in what decides, even where a `let` inside
/// `ensures` named something else the same — what the `let` read as the rule
/// found it does not outlive the loop rebinding the name.
#[test]
fn a_loop_name_is_the_member_where_a_let_inside_ensures_named_another() {
    let (outcome, upload, thumbs) = fired_with_thumbs("ShadowedByLoop", &["a", "b"]);
    let shadowed = rule(&outcome, "ShadowedByLoop");
    assert_eq!(shadowed.disposition, Disposition::Fired, "{shadowed:#?}");
    assert!(!skipped(shadowed), "{shadowed:#?}");
    assert!(outcome.world.instance(&upload).is_none());
    assert!(outcome.world.instance(&thumbs[0]).is_none(), "{outcome:#?}");
    assert!(outcome.world.instance(&thumbs[1]).is_some(), "{outcome:#?}");
}

/// An assignment to what the rule removes is checked against what it held:
/// `done` is a state of `status`, and the field moved from `pending` — not
/// from nothing to a word nobody could read.
#[test]
fn an_assignment_to_what_the_rule_removes_is_read_against_what_it_held() {
    let (graph, program, sources) = recorded();
    let (mut world, upload, _) = world(&graph, false);
    world.set_field(&upload, "status", Value::Enum("pending".to_owned()));
    let event =
        Event::new("FinishRemovingFirst", "uploads").with("upload", Value::Ref(upload.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let finish = rule(&outcome, "FinishRemovingFirst");
    assert!(finish.unresolved.is_empty(), "{finish:#?}");
    let assigned = finish.effects.iter().find_map(|effect| match effect {
        Effect::Assigned { from, to, .. } => Some((from.clone(), to.clone())),
        _ => None,
    });
    assert_eq!(
        assigned,
        Some((Value::Enum("pending".to_owned()), Value::Enum("done".to_owned()))),
        "{finish:#?}"
    );
    assert!(outcome.world.instance(&upload).is_none());
}

/// What a live instance computes from what the rule removes is read in the
/// state the rule leaves as well, and the definition says which rule removed
/// it — not that the upload was never in this world.
#[test]
fn a_definition_reading_what_the_rule_removes_says_which_rule_removed_it() {
    let (graph, program, sources) = recorded();
    let (mut world, upload, _) = world(&graph, false);
    let shelf = world.create("Shelf", "uploads");
    world.set_field(&shelf, "latest", Value::Ref(upload.clone()));
    let event = Event::new("Recall", "uploads")
        .with("upload", Value::Ref(upload.clone()))
        .with("shelf", Value::Ref(shelf));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let recall = rule(&outcome, "Recall");
    assert_eq!(blob_name(&outcome), Value::Unknown, "{outcome:#?}");
    let note = removal_note(recall);
    assert!(
        note.reason.contains("rule `Recall` removes `upload`")
            && note.reason.contains("so `latest.name` has nothing to read"),
        "{}",
        note.reason
    );
    assert!(!reasons(recall).iter().any(|it| it.contains("is not in this world")), "{recall:#?}");
}

/// A chain of four links, each the `next` of the one before it.
fn chain(world: &mut World) -> Vec<EntityId> {
    let links: Vec<EntityId> = (0..4).map(|_| world.create("Link", "uploads")).collect();
    for pair in links.windows(2) {
        world.set_field(&pair[0], "next", Value::Ref(pair[1].clone()));
    }
    world.set_field(&links[3], "next", Value::Null);
    links
}

/// A chain four deep goes whole, top first or bottom first, and neither
/// order is said to unsettle anything. A line reaching what it removes
/// through what a line above it removes reads that as the rule found it in
/// the same reading of the rule, so the whole chain is found at once: found
/// a level per reading, a chain deeper than the readings allowed was
/// reported as a removal that never settles, and the verdict turned on line
/// order again.
#[test]
fn a_chain_removed_through_itself_goes_whole_in_either_order() {
    for trigger in ["Fell", "FellFromTheBottom"] {
        let (graph, program, sources) = recorded();
        let mut world = seed(&graph);
        let links = chain(&mut world);
        let event = Event::new(trigger, "uploads").with("link", Value::Ref(links[0].clone()));
        let outcome = step(&graph, &program, &sources, &world, &event);
        let fell = rule(&outcome, trigger);
        assert_eq!(fell.disposition, Disposition::Fired, "{fell:#?}");
        assert!(
            !reasons(fell).iter().any(|it| it.contains("unsettles")),
            "{trigger}: {:#?}",
            reasons(fell)
        );
        assert!(fell.unapplied.is_empty(), "{trigger}: {fell:#?}");
        for link in &links {
            assert!(outcome.world.instance(link).is_none(), "{trigger} kept {link}: {outcome:#?}");
        }
    }
}

/// What decides reads what the rule's own lines wrote to what it removes,
/// as it reads what they wrote to anything else: `upload.big = false` above
/// `if upload.big:` decides the `if` from `false` whether or not a third
/// line removes the upload. Read from the upload as the trigger found it,
/// the removal alone turned the same `if` true and made a blob.
#[test]
fn what_decides_reads_the_rules_own_writes_to_what_it_removes() {
    for trigger in ["Shrink", "ShrinkRemoving"] {
        let (outcome, upload, _) = fired(trigger, true);
        let shrink = rule(&outcome, trigger);
        assert_eq!(shrink.disposition, Disposition::Fired, "{shrink:#?}");
        assert!(!skipped(shrink), "{trigger}: {shrink:#?}");
        let blobs = outcome.world.entities.values().filter(|it| it.entity == "Blob").count();
        assert_eq!(blobs, 0, "{trigger}: {outcome:#?}");
        assert_eq!(
            outcome.world.instance(&upload).is_none(),
            trigger == "ShrinkRemoving",
            "{outcome:#?}"
        );
    }
}

/// A loop's member removed in the loop that reads it has no name beside
/// the `when`, so the reason does not tell the author to bind one there:
/// it says which name a `let` there would have to start from, and that
/// `t` is not one.
#[test]
fn a_read_of_a_removed_loop_member_does_not_advise_a_let_nobody_can_write() {
    let (outcome, _, _) = fired_with_thumbs("PurgeRecording", &["a"]);
    let purge = rule(&outcome, "PurgeRecording");
    let note = removal_note(purge);
    assert!(
        note.reason.contains("rule `PurgeRecording` removes `t`")
            && note.reason.contains("so `t.name` has nothing to read")
            && note.reason.contains("`t` is not a name bound there")
            && !note.reason.contains("bind `let"),
        "{}",
        note.reason
    );
    assert!(purge.unapplied.contains(note), "{purge:#?}");
}
