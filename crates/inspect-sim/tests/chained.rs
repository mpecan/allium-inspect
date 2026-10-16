//! A trigger one rule emits, reaching the rules that listen for it (issue #12).
//!
//! A trigger is declared by nothing but its listeners' `when` clauses, so each
//! listener binds what a firing carries on its own terms: a positional argument
//! under the name *it* gives that place, a named argument under the name
//! written. A required parameter left unbound makes the listener undecided,
//! saying which name it waited for and what it was handed — never fired about
//! nothing, which is what reported `refused` for a reaction nobody decided.
//!
//! Against the recorded `allium` output for the `chains` and `relay` fixtures.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{
    Disposition, Effect, RuleOutcome, Truth, Value,
    step::{Sources, StepOutcome, step},
    value::EntityId,
    world::{Event, World},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// `chains` and `relay`, ingested exactly as the server does.
fn chains() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();

    for module in ["chains", "relay"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        let text = read(&path);
        sources.insert(module.to_owned(), text.clone());
        reader = reader.with(&path, text);
        paths.push(path);
    }

    let Ingestion { graph, program } = ingest(&runner, &reader, &paths).expect("ingests");
    (graph, program, sources)
}

/// A world holding one `Doc` owned by one `User`, every flag false.
fn one_doc() -> (World, EntityId) {
    let (world, doc, _) = doc_and_owner();
    (world, doc)
}

/// The same world, with the owner's id as well.
fn doc_and_owner() -> (World, EntityId, EntityId) {
    let mut world = World::new().at(1_000);
    let user = world.create("User", "chains");
    for flag in ["notified", "pinged"] {
        world.set_field(&user, flag, Value::Bool(false));
    }
    let doc = world.create("Doc", "chains");
    world.set_field(&doc, "owner", Value::Ref(user.clone()));
    for flag in ["seen", "told", "filed", "relayed", "echoed", "marked", "weighed"] {
        world.set_field(&doc, flag, Value::Bool(false));
    }
    (world, doc, user)
}

fn fire(event: &Event) -> (StepOutcome, EntityId) {
    let (graph, program, sources) = chains();
    let (world, doc) = one_doc();
    let event = event.clone();
    (step(&graph, &program, &sources, &world, &event), doc)
}

fn rule<'a>(outcome: &'a StepOutcome, name: &str) -> &'a RuleOutcome {
    outcome
        .rules
        .iter()
        .find(|rule| rule.name == name)
        .unwrap_or_else(|| panic!("no outcome for {name}: {:#?}", outcome.rules))
}

/// The emission `rule` made, as the event a walk would fire next.
fn emitted(outcome: &StepOutcome, name: &str) -> Event {
    rule(outcome, name)
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Emitted { trigger, module, arguments, positional } => Some(Event {
                trigger: trigger.clone(),
                module: module.clone(),
                arguments: arguments.clone(),
                positional: positional.clone(),
                emitted: true,
            }),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} emitted nothing"))
}

fn flag(outcome: &StepOutcome, doc: &EntityId, field: &str) -> Value {
    outcome.world.instance(doc).expect("the doc").field(field)
}

/// `Announced(doc)` by position reaches `Tell` as `thing`, `File` as `doc`, and
/// `Relay` — in another module — as `story`.
#[test]
fn a_positional_argument_reaches_every_listener_under_its_own_name() {
    let (world, doc) = one_doc();
    let (announce, _) = fire(&Event::new("Announce", "chains").passing(Value::Ref(doc.clone())));
    let next = emitted(&announce, "Announce");
    assert_eq!(next.positional, [Value::Ref(doc.clone())]);

    let (graph, program, sources) = chains();
    let outcome = step(&graph, &program, &sources, &world, &next);
    for listener in ["Tell", "File", "Relay"] {
        let ran = rule(&outcome, listener);
        assert_eq!(ran.disposition, Disposition::Fired, "{listener}: {:?}", ran.unresolved);
    }
    for field in ["told", "filed", "relayed"] {
        assert_eq!(flag(&outcome, &doc, field), Value::Bool(true), "{field}");
    }
}

/// Issue #12: `Published(item: doc)` binds `item`, and `MarkSeen` waits for
/// `doc`. Undecided, about its `when` clause, naming both names.
#[test]
fn a_named_argument_the_listener_does_not_have_leaves_it_undecided_and_says_why() {
    let (_, doc) = one_doc();
    let (outcome, _) = fire(&Event::new("Published", "chains").with("item", Value::Ref(doc)));

    for listener in ["MarkSeen", "Echo"] {
        let ran = rule(&outcome, listener);
        assert_eq!(ran.disposition, Disposition::Undecided, "{listener}: {ran:#?}");
        assert!(ran.effects.is_empty(), "nothing applied: {:?}", ran.effects);
        let when = &ran.requires[0];
        assert_eq!(when.truth, Truth::Unknown);
        assert_eq!(when.text, ran.unresolved[0].expression.clone().unwrap_or_default());
        assert!(when.text.contains("Published(doc)"), "{}", when.text);
        assert!(ran.unresolved[0].span.is_some(), "anchored to the clause");
        assert_eq!(
            ran.unresolved[0].reason,
            "nothing this firing of `Published` carried binds `doc`: it was handed `item` by \
             name, and a named argument binds only the parameter it names"
        );
    }
}

/// The span is the `when` clause's own, so the source strip highlights the
/// clause that names the parameter.
#[test]
fn the_reason_is_anchored_to_the_listener_s_when_clause() {
    let (_, _, sources) = chains();
    let (_, doc) = one_doc();
    let (outcome, _) = fire(&Event::new("Published", "chains").with("item", Value::Ref(doc)));
    let span = rule(&outcome, "MarkSeen").unresolved[0].span.expect("a span");
    // An em-dash sits in a comment above `MarkSeen`, so a char offset taken
    // for a byte one lands two bytes short of the clause and this fails.
    let before = &sources["chains"][..span.start];
    assert!(!before.is_ascii(), "the fixture must put a multibyte character before the rule");
    let text = span.slice(&sources["chains"]).expect("in the source");
    assert_eq!(text, "when: Published(doc)");
}

/// Named the way the listener names it, it binds.
#[test]
fn a_named_argument_binds_the_parameter_it_names() {
    let (_, doc) = one_doc();
    let (outcome, _) =
        fire(&Event::new("Published", "chains").with("doc", Value::Ref(doc.clone())));
    for listener in ["MarkSeen", "Echo"] {
        assert_eq!(rule(&outcome, listener).disposition, Disposition::Fired, "{listener}");
    }
    assert_eq!(flag(&outcome, &doc, "seen"), Value::Bool(true));
    assert_eq!(flag(&outcome, &doc, "echoed"), Value::Bool(true));
}

/// Fired with nothing at all — a trigger picked with no arguments — it says so,
/// rather than running every clause about nothing.
#[test]
fn a_firing_that_carries_nothing_says_it_carried_nothing() {
    let (outcome, _) = fire(&Event::new("Announced", "chains"));
    let tell = rule(&outcome, "Tell");
    assert_eq!(tell.disposition, Disposition::Undecided);
    assert_eq!(
        tell.unresolved[0].reason,
        "nothing this firing of `Announced` carried binds `thing`: it carried no arguments"
    );
}

/// The event `rule` emitted, fired against the world `outcome` left.
fn follow(outcome: &StepOutcome, name: &str) -> StepOutcome {
    let (graph, program, sources) = chains();
    step(&graph, &program, &sources, &outcome.world, &emitted(outcome, name))
}

/// `Shared(doc.owner)`: an expression by position, not a bare name, reaches
/// `Notify` as `user`, evaluated to the owner.
#[test]
fn a_positional_expression_is_carried_as_its_value() {
    let (world, doc, user) = doc_and_owner();
    let (graph, program, sources) = chains();
    let share = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("Share", "chains").passing(Value::Ref(doc)),
    );
    assert_eq!(emitted(&share, "Share").positional, [Value::Ref(user.clone())]);
    let outcome = follow(&share, "Share");
    assert_eq!(rule(&outcome, "Notify").disposition, Disposition::Fired);
    assert_eq!(
        outcome.world.instance(&user).expect("the user").field("notified"),
        Value::Bool(true)
    );
}

/// `Paired(doc, doc.owner)` against `when: Paired(d, u)`: two by position, in
/// order, under names neither emitter nor any other listener uses. Swapped,
/// `requires: d.owner = u` would not hold.
#[test]
fn two_positional_arguments_bind_in_order_under_the_listener_s_names() {
    let (world, doc, user) = doc_and_owner();
    let (graph, program, sources) = chains();
    let pair = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("Pair", "chains").passing(Value::Ref(doc.clone())),
    );
    assert_eq!(emitted(&pair, "Pair").positional, [Value::Ref(doc), Value::Ref(user.clone())]);
    let outcome = follow(&pair, "Pair");
    let ping = rule(&outcome, "Ping");
    assert_eq!(ping.disposition, Disposition::Fired, "{ping:#?}");
    assert_eq!(outcome.world.instance(&user).expect("the user").field("pinged"), Value::Bool(true));
}

/// `Noted(doc: doc, reason: doc)` against `when: Noted(doc, cause?)`:
/// `cause` is not quietly `null` — `reason` may have been meant for it — so
/// `Mark` is undecided and names `reason`, rather than firing on `cause =
/// null`.
#[test]
fn an_optional_parameter_beside_a_misnamed_argument_is_undecided() {
    let (_, doc) = one_doc();
    let (note, _) = fire(&Event::new("Note", "chains").passing(Value::Ref(doc.clone())));
    let outcome = follow(&note, "Note");
    let mark = rule(&outcome, "Mark");
    assert_eq!(mark.disposition, Disposition::Undecided, "{mark:#?}");
    assert_eq!(
        mark.unresolved[0].reason,
        "nothing this firing of `Noted` carried binds `cause`: it was handed `reason` by name, \
         and a named argument binds only the parameter it names"
    );
    assert_eq!(flag(&outcome, &doc, "marked"), Value::Bool(false));
}

/// A parameter short, against a precondition false whatever it would have
/// been: refused, with the `when` still undecided and saying why.
#[test]
fn a_definitely_false_precondition_refuses_despite_an_unbound_parameter() {
    let (_, doc) = one_doc();
    let (outcome, _) = fire(&Event::new("Weighed", "chains").passing(Value::Ref(doc.clone())));
    let weigh = rule(&outcome, "Weigh");
    assert_eq!(weigh.disposition, Disposition::Refused, "{weigh:#?}");
    let when = &weigh.requires[0];
    assert_eq!(when.text, "Weighed(doc, scale)");
    assert_eq!(when.truth, Truth::Unknown);
    assert!(when.unresolved[0].reason.contains("binds `scale`"), "{:?}", when.unresolved);
    assert!(weigh.requires.iter().any(|clause| clause.truth == Truth::False), "{weigh:#?}");
    assert_eq!(flag(&outcome, &doc, "weighed"), Value::Bool(false));
}

/// Two modules that do not import each other, each closing its own thing with
/// its own `Closed`: a trigger name they share by coincidence.
fn unrelated() -> (SpecGraph, Program, Sources) {
    let modules = [
        (
            "issues",
            "-- allium: 3\nentity Issue { closed: Boolean }\n\
             rule CloseIssue {\n    when: CloseIssue(issue)\n    ensures: Closed(issue: issue)\n}\n\
             rule MarkIssueClosed {\n    when: Closed(issue)\n    ensures: issue.closed = true\n}\n",
        ),
        (
            "pulls",
            "-- allium: 3\nentity Pull { closed: Boolean }\n\
             rule ClosePull {\n    when: ClosePull(pull)\n    ensures: Closed(pull: pull)\n}\n\
             rule MarkPullClosed {\n    when: Closed(pull)\n    ensures: pull.closed = true\n}\n",
        ),
    ];
    from_source(&modules)
}

/// `modules`, ingested from their source alone: `model` and `plan` replayed
/// empty, because nothing here reads what they carry.
fn from_source(modules: &[(&str, &str)]) -> (SpecGraph, Program, Sources) {
    let mut runner = MapRunner::new("test");
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();
    for &(module, text) in modules {
        let path = PathBuf::from(format!("{module}.allium"));
        for command in [Command::Model, Command::Plan] {
            runner = runner.with(command, &path, serde_json::json!({}));
        }
        reader = reader.with(&path, text);
        sources.insert(module.to_owned(), text.to_owned());
        paths.push(path);
    }
    let Ingestion { graph, program } = ingest(&runner, &reader, &paths).expect("ingests");
    (graph, program, sources)
}

/// An emission wakes the rules that hear the module it was emitted in, and
/// not an unrelated module's rule waiting on the same word — which reported a
/// correct rule undecided, handed `issue` when it waits for `pull`.
#[test]
fn an_emission_is_not_heard_by_a_module_that_does_not_import_the_emitter() {
    let (graph, program, sources) = unrelated();
    let mut world = World::new();
    let issue = world.create("Issue", "issues");
    world.set_field(&issue, "closed", Value::Bool(false));
    let event = Event::new("Closed", "issues").with("issue", Value::Ref(issue)).as_emitted();
    let outcome = step(&graph, &program, &sources, &world, &event);
    let woken: Vec<&str> = outcome.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(woken, ["MarkIssueClosed"]);
    assert_eq!(outcome.rules[0].disposition, Disposition::Fired);
}

/// A firing nobody emitted is not narrowed: which module's `Closed` a person
/// meant is not something to guess, so both listeners are woken as before.
#[test]
fn a_firing_nobody_emitted_still_wakes_every_listener_of_that_name() {
    let (graph, program, sources) = unrelated();
    let event = Event::new("Closed", "issues");
    let outcome = step(&graph, &program, &sources, &World::new(), &event);
    let woken: Vec<&str> = outcome.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(woken, ["MarkIssueClosed", "MarkPullClosed"]);
}

/// `failures` and `migrations`, ingested exactly as the server does.
fn failures() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();
    for module in ["failures", "migrations"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
        }
        let text = read(&path);
        sources.insert(module.to_owned(), text.clone());
        reader = reader.with(&path, text);
        paths.push(path);
    }
    let Ingestion { graph, program } = ingest(&runner, &reader, &paths).expect("ingests");
    (graph, program, sources)
}

/// `admin/FailureReported(…)` is `failures`' trigger, emitted from
/// `migrations` through the alias it imported it under (issue #17). It is
/// emitted as that module's, and the rule there waiting on `FailureReported`
/// is woken by it and makes its notice — where it was read as neither a
/// creation nor a trigger, and `Fail` left part of what it ensures undone.
#[test]
fn an_emission_qualified_with_another_module_is_that_module_s_trigger() {
    let (graph, program, sources) = failures();
    let mut world = World::new();
    let migration = world.create("Migration", "migrations");
    world.set_field(&migration, "failed", Value::Bool(false));
    let event = Event::new("Fail", "migrations").passing(Value::Ref(migration.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let fail = rule(&outcome, "Fail");
    assert_eq!(fail.disposition, Disposition::Fired, "{fail:#?}");
    assert!(fail.unapplied.is_empty(), "{:#?}", fail.unapplied);
    let next = emitted(&outcome, "Fail");
    assert_eq!(next.trigger, "FailureReported");
    assert_eq!(next.module, "failures", "the alias's module, not the emitter's");
    assert_eq!(next.arguments["description"], Value::Str("boom".to_owned()));

    let after = step(&graph, &program, &sources, &outcome.world, &next);
    let record = rule(&after, "Record");
    assert_eq!(record.disposition, Disposition::Fired, "{record:#?}");
    let Some(Effect::Created { id, entity }) = record.effects.first() else {
        panic!("Record creates a notice: {:?}", record.effects)
    };
    assert_eq!(entity, "Notice");
    let notice = after.world.instance(id).expect("the notice exists");
    assert_eq!(notice.module, "failures");
    assert_eq!(notice.field("kind"), Value::Str("migration".to_owned()));
    assert_eq!(notice.field("text"), Value::Str("boom".to_owned()));
}

/// Heard as `failures` emitting it: by a module that imports `failures`, and
/// not by one that imports neither side and happens to wait on the same word.
#[test]
fn a_qualified_emission_is_heard_where_the_aliased_module_s_own_would_be() {
    let listener = |name: &str| {
        format!(
            "rule {name} {{\n    when: Reported(what)\n    ensures: Seen.created(what: what)\n}}\n"
        )
    };
    let owner = format!("-- allium: 3\nentity Seen {{ what: String }}\n{}", listener("Own"));
    let user = format!(
        "-- allium: 3\nuse \"./owner.allium\" as o\nentity Seen {{ what: String }}\n{}",
        listener("Imports")
    );
    let stranger =
        format!("-- allium: 3\nentity Seen {{ what: String }}\n{}", listener("Stranger"));
    let emitter = "-- allium: 3\nuse \"./owner.allium\" as o\n\
                   rule Go {\n    when: Go(what)\n    ensures: o/Reported(what: what)\n}\n";
    let (graph, program, sources) = from_source(&[
        ("emitter", emitter),
        ("owner", &owner),
        ("stranger", &stranger),
        ("user", &user),
    ]);
    let event = Event::new("Go", "emitter").with("what", Value::Str("x".to_owned()));
    let outcome = step(&graph, &program, &sources, &World::new(), &event);
    let next = emitted(&outcome, "Go");
    assert_eq!(next.module, "owner");
    let after = step(&graph, &program, &sources, &outcome.world, &next);
    let woken: Vec<&str> = after.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(woken, ["Own", "Imports"]);
}

/// A rule written `when: o/Reported(…)` hears `owner` emitting when its `o`
/// is `owner`, and not when its `o` is some other module — the same alias,
/// leading elsewhere.
#[test]
fn a_qualified_emission_wakes_a_qualified_listener_whose_alias_leads_to_the_same_module() {
    let listener = |name: &str| {
        format!(
            "entity Seen {{ what: String }}\n\
             rule {name} {{\n    when: o/Reported(what)\n    ensures: Seen.created(what: what)\n}}\n"
        )
    };
    let owner = "-- allium: 3\nentity Seen { what: String }\n";
    let qualified = format!("-- allium: 3\nuse \"./owner.allium\" as o\n{}", listener("Qualified"));
    let elsewhere =
        format!("-- allium: 3\nuse \"./stranger.allium\" as o\n{}", listener("Elsewhere"));
    let stranger = "-- allium: 3\nentity Seen { what: String }\n";
    let emitter = "-- allium: 3\nuse \"./owner.allium\" as o\n\
                   rule Go {\n    when: Go(what)\n    ensures: o/Reported(what: what)\n}\n";
    let (graph, program, sources) = from_source(&[
        ("elsewhere", &elsewhere),
        ("emitter", emitter),
        ("owner", owner),
        ("qualified", &qualified),
        ("stranger", stranger),
    ]);
    let event = Event::new("Go", "emitter").with("what", Value::Str("x".to_owned()));
    let outcome = step(&graph, &program, &sources, &World::new(), &event);
    let next = emitted(&outcome, "Go");
    let after = step(&graph, &program, &sources, &outcome.world, &next);
    let woken: Vec<&str> = after.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(woken, ["Qualified"]);
}

/// The step `Go` takes in `emitter`, and the one reason it left undone.
fn left_undone(emitter: &str) -> inspect_sim::Unresolved {
    let (graph, program, sources) = from_source(&[("emitter", emitter)]);
    let event = Event::new("Go", "emitter").with("what", Value::Str("x".to_owned()));
    let outcome = step(&graph, &program, &sources, &World::new(), &event);
    let go = rule(&outcome, "Go");
    assert!(
        !go.effects.iter().any(|effect| matches!(effect, Effect::Emitted { .. })),
        "{:#?}",
        go.effects
    );
    assert_eq!(go.unapplied.len(), 1, "{:#?}", go.unapplied);
    go.unapplied[0].clone()
}

/// An alias that leads to no module in the set names nobody's trigger. Nothing
/// is emitted — waking every rule of that name would be a guess — and the rule
/// says what it left undone and which of the two mistakes it is: a `use` whose
/// file is not in the set, or an alias never imported. The emission is quoted
/// and located by byte span, past a comment where bytes and characters differ.
#[test]
fn an_emission_through_an_alias_that_leads_nowhere_is_left_undone_and_says_why() {
    let emitter = "-- allium: 3\nuse \"./missing.allium\" as gone\n\
                   rule Go {\n    when: Go(what)\n    -- a failure — handed off\n    \
                   ensures: gone/Reported(what: what)\n}\n\
                   rule Hears {\n    when: Reported(what)\n    ensures: Seen.created(what: what)\n}\n\
                   entity Seen { what: String }\n";
    let why = left_undone(emitter);
    assert!(
        why.reason.starts_with(
            "`emitter` imports `gone` from \"./missing.allium\", which is not in this spec \
             set, so `gone/Reported` is no known module's trigger and was not emitted"
        ),
        "{why:?}"
    );
    assert_eq!(why.expression.as_deref(), Some("gone/Reported"));
    let span = why.span.unwrap_or_else(|| panic!("located: {why:?}"));
    assert_eq!(Some(span.start), emitter.find("gone/Reported"));
    assert_eq!(&emitter[span.start..span.end], "gone/Reported");

    let never = "-- allium: 3\n\
                 rule Go {\n    when: Go(what)\n    ensures: ghost/Reported(what: what)\n}\n";
    let why = left_undone(never);
    assert!(
        why.reason.starts_with(
            "`emitter` imports nothing as `ghost`, so `ghost/Reported` is no known \
             module's trigger and was not emitted"
        ),
        "{why:?}"
    );
}
