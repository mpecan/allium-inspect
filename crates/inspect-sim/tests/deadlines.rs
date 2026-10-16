//! A temporal trigger over a `null` deadline, in the listing the browser shows
//! (issue #20).
//!
//! `enabled` is what the Trace panel offers as the steps a person may take
//! next, and what `newly_enabled` is computed from. A task that never stopped
//! has no deadline, so `Expire` is not due for it — decided, and so absent
//! from the listing — while an offer with no opening still lists `Withdraw` as
//! undecided, with its reason, because whether a `null` is still ahead is not
//! a question it answers.
//!
//! Against the recorded `allium` output for the `deadlines` fixture.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{Enabled, Value, enabled, seed::seed, step::Sources, world::World};

const HOUR: i64 = 3_600_000;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The recorded `deadlines` fixture, ingested exactly as the server does.
fn recorded() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let path = root.join("specs/deadlines.allium");
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    for command in Command::ALL {
        let document = read(&root.join(format!("cli/deadlines.{command}.json")));
        runner = runner.with(command, &path, serde_json::from_str(&document).expect("JSON"));
    }
    let text = read(&path);
    let sources: Sources = BTreeMap::from([("deadlines".to_owned(), text.clone())]);
    let reader = MemoryReader::default().with(&path, text);
    let Ingestion { graph, program } = ingest(&runner, &reader, &[path]).expect("ingests");
    (graph, program, sources)
}

/// A task that started at zero and never stopped, already timed out by hand
/// so `Zombie` refuses, and an offer with no opening — both two hours on.
fn world(graph: &SpecGraph) -> World {
    let mut world = seed(graph);
    let task = world.create("Task", "deadlines");
    world.set_field(&task, "started_at", Value::Timestamp(0));
    world.set_field(&task, "stopped_at", Value::Null);
    world.set_field(&task, "status", Value::Enum("failure".to_owned()));
    world.set_field(&task, "expired", Value::Bool(false));
    let offer = world.create("Offer", "deadlines");
    world.set_field(&offer, "opens_at", Value::Null);
    world.set_field(&offer, "withdrawn", Value::Bool(false));
    world.now = 2 * HOUR;
    world
}

fn listed<'a>(listing: &'a [Enabled], rule: &str) -> Option<&'a Enabled> {
    listing.iter().find(|it| it.name == rule)
}

#[test]
fn a_task_that_never_stopped_is_not_listed_as_due_or_undecided() {
    let (graph, program, sources) = recorded();
    let listing = enabled(&graph, &program, &sources, &world(&graph));
    assert!(listed(&listing, "Expire").is_none(), "{listing:#?}");
}

/// The control: the opposite question stays undecided, naming the comparison
/// and the offer it could not settle.
#[test]
fn an_offer_with_no_opening_is_still_listed_as_undecided_with_its_reason() {
    let (graph, program, sources) = recorded();
    let world = world(&graph);
    let listing = enabled(&graph, &program, &sources, &world);
    let withdraw = listed(&listing, "Withdraw").expect("listed");
    assert!(withdraw.over.is_empty(), "{withdraw:#?}");
    assert!(!withdraw.undecided.is_empty(), "{withdraw:#?}");
    assert!(
        withdraw
            .undecided
            .iter()
            .any(|it| it.expression.as_deref() == Some("Offer.opens_at > now")),
        "{withdraw:#?}"
    );
    let offer = world.instances_of("Offer").next().expect("an offer").id.clone();
    assert_eq!(withdraw.unsettled_over, vec![Value::Ref(offer)]);
}

/// Which rules a listing reads at all: one whose `when` binds the instance
/// it is about does, and `Overdue` — `when: Chore.due_at <= now`, binding
/// nothing — does not, so a listing's silence about it is no answer.
#[test]
fn a_listing_reads_a_rule_that_binds_its_instance_and_not_one_that_binds_nothing() {
    let (graph, program, _) = recorded();
    let rule = |name: &str| {
        graph
            .nodes_of(inspect_model::NodeKind::Rule)
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("no rule {name}"))
    };
    assert!(inspect_sim::step::lists(&program, rule("Expire")), "Expire binds `task`");
    assert!(!inspect_sim::step::lists(&program, rule("Overdue")), "Overdue binds nothing");
}
