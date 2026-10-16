//! A creation, and the rules that react to it (issue #9).
//!
//! `when: o: Order.created` is a trigger kind of its own in the language: not
//! a condition over the world that holds or does not, but something that
//! happened, once, to one instance. Read as a state condition it asked each
//! order for a field called `created`, which nothing ever sets, so the rule
//! never fired — and every firing about an order woke it to say so.
//!
//! Against the recorded `allium` output for the `orders`, `notes` and
//! `pinboard` fixtures.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::graph::TriggerSource;
use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{
    Disposition, Effect, Enabled, RuleOutcome, Truth, Value, enabled, reactions,
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

/// The recorded fixture modules named, ingested exactly as the server does.
fn recorded(modules: &[&str]) -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();

    for &module in modules {
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

fn rule<'a>(name: &str, outcome: &'a StepOutcome) -> Option<&'a RuleOutcome> {
    outcome.rules.iter().find(|candidate| candidate.name == name)
}

/// `Place(seed)` over a world holding one order, and what it created.
fn placed() -> (SpecGraph, Program, Sources, StepOutcome, EntityId) {
    let (graph, program, sources) = recorded(&["orders"]);
    let mut world = World::new();
    let seed = world.create("Order", "orders");
    let event = Event::new("Place", "orders").with("o", Value::Ref(seed.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let fresh = outcome
        .rules
        .iter()
        .flat_map(|rule| &rule.effects)
        .find_map(|effect| match effect {
            Effect::Created { id, .. } => Some(id.clone()),
            _ => None,
        })
        .expect("Place creates an order");
    (graph, program, sources, outcome, fresh)
}

/// The step that creates is one step: the reaction is offered, not run, the
/// same as an emission. What it is offered as is the creation of *this* order.
#[test]
fn a_creation_offers_the_rule_that_reacts_to_it() {
    let (_, _, _, outcome, fresh) = placed();
    assert!(rule("Welcome", &outcome).is_none(), "not run in the step that created");

    let welcome = outcome
        .newly_enabled
        .iter()
        .find(|candidate| candidate.name == "Welcome")
        .expect("Welcome is offered");
    assert_eq!(welcome.trigger, "Order.created");
    assert_eq!(welcome.binding, "o");
    assert_eq!(welcome.over, vec![Value::Ref(fresh.clone())]);

    let events = reactions(&outcome);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].trigger, "Order.created");
    assert_eq!(events[0].module, "orders");
    assert_eq!(events[0].arguments.get("o"), Some(&Value::Ref(fresh)));
}

/// Firing what was offered runs the reaction, about the order created.
#[test]
fn firing_the_creation_runs_the_reaction_about_that_order() {
    let (graph, program, sources, outcome, fresh) = placed();
    let event = reactions(&outcome).remove(0);
    let next = step(&graph, &program, &sources, &outcome.world, &event);

    let welcome = rule("Welcome", &next).expect("Welcome is woken");
    assert_eq!(welcome.disposition, Disposition::Fired, "{welcome:?}");
    assert_eq!(welcome.requires[0].truth, Truth::True, "the `when` is what woke it");
    assert_eq!(welcome.requires[0].text, "o: Order.created");
    assert_eq!(next.world.instance(&fresh).unwrap().field("welcomes"), Value::Int(1));
    assert!(rule("Place", &next).is_none(), "nothing else waits on a creation");
}

/// A state rule's firing names the entity; a creation is a different event.
/// Woken by one, the creation rule asked the order for a `created` field and
/// came back undecided on every firing about an order.
#[test]
fn a_firing_about_an_order_that_is_not_its_creation_does_not_wake_the_reaction() {
    let (graph, program, sources, outcome, fresh) = placed();
    let event = Event::new("Order", "orders").with("o", Value::Ref(fresh));
    let next = step(&graph, &program, &sources, &outcome.world, &event);
    assert!(rule("Welcome", &next).is_none(), "{:?}", next.rules);
}

/// Not a condition over the world: an order that exists has not thereby just
/// been created, and listing the rule as undecided for every order there is
/// told a reader it might be about to run.
#[test]
fn a_creation_rule_is_never_enabled_by_the_world() {
    let (graph, program, sources, outcome, _) = placed();
    let listed = enabled(&graph, &program, &sources, &outcome.world);
    assert!(listed.iter().all(|candidate| candidate.name != "Welcome"), "{listed:?}");
}

/// Fired by hand with nothing to be about, it does not guess which order: it
/// says it was told of no creation, naming the clause.
#[test]
fn a_creation_event_carrying_nothing_is_undecided_and_says_why() {
    let (graph, program, sources) = recorded(&["orders"]);
    let mut world = World::new();
    world.create("Order", "orders");
    let event = Event::new("Order.created", "orders");
    let outcome = step(&graph, &program, &sources, &world, &event);

    let welcome = rule("Welcome", &outcome).expect("Welcome is woken");
    assert_eq!(welcome.disposition, Disposition::Undecided);
    assert!(welcome.effects.is_empty());
    let why = &welcome.unresolved[0];
    assert!(why.reason.contains("`o`"), "{why:?}");
    assert!(why.reason.contains("Order.created"), "{why:?}");
    assert_eq!(why.expression.as_deref(), Some("o: Order.created"));
}

/// `memo/Note.created(...)`: a creation through an alias, of an entity another
/// module declares. It used to be read as neither a creation nor a trigger.
#[test]
fn a_qualified_creation_creates_the_entity_in_its_own_module() {
    let (graph, program, sources) = recorded(&["notes", "pinboard"]);
    let mut world = World::new();
    let board = world.create("Board", "pinboard");
    let event = Event::new("Pin", "pinboard")
        .with("board", Value::Ref(board))
        .with("text", Value::Str("hello".to_owned()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let pin = rule("Pin", &outcome).expect("Pin fires");
    assert_eq!(pin.disposition, Disposition::Fired);
    let Some(Effect::Created { id, entity }) = pin.effects.first() else {
        panic!("Pin creates a note: {:?}", pin.effects)
    };
    assert_eq!(entity, "Note");
    let note = outcome.world.instance(id).expect("the note exists");
    assert_eq!(note.module, "notes", "where the entity is declared, read through the alias");
    assert_eq!(note.field("text"), Value::Str("hello".to_owned()));
    assert_eq!(note.field("seen"), Value::Bool(false));

    // And the qualified reaction is woken by it.
    let events = reactions(&outcome);
    assert_eq!(events.len(), 1, "{events:?}");
    let next = step(&graph, &program, &sources, &outcome.world, &events[0]);
    let notice = rule("Notice", &next).expect("Notice is woken");
    assert_eq!(events[0].module, "notes", "the creation is the note's, not the rule's");
    assert_eq!(notice.disposition, Disposition::Fired, "{notice:?}");
    assert_eq!(next.world.instance(id).unwrap().field("seen"), Value::Bool(true));
}

/// The same world and event, the same outcome, reactions included.
#[test]
fn reacting_is_deterministic() {
    let (_, _, _, first, _) = placed();
    let (_, _, _, second, _) = placed();
    assert_eq!(first, second);
    assert_eq!(reactions(&first), reactions(&second));
}

/// Made and unmade in one step: nothing was there afterwards for a reaction
/// to be about, so none is offered.
#[test]
fn an_instance_created_and_removed_in_one_step_is_not_reacted_to() {
    let (graph, program, sources) = recorded(&["orders"]);
    let mut world = World::new();
    let seed = world.create("Order", "orders");
    let event = Event::new("Retract", "orders").with("o", Value::Ref(seed));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let retract = rule("Retract", &outcome).expect("Retract fires");
    assert!(
        retract.effects.iter().any(|effect| matches!(effect, Effect::Created { .. })),
        "it did create one: {:?}",
        retract.effects
    );
    assert!(
        retract.effects.iter().any(|effect| matches!(effect, Effect::Removed { .. })),
        "and removed it: {:?}",
        retract.effects
    );
    assert!(outcome.newly_enabled.iter().all(|candidate| candidate.name != "Welcome"));
    assert!(reactions(&outcome).is_empty());
}

/// A name the event gave under the rule's own binding is kept, even when it is
/// not an order. Swapping in some other order the event carried would be the
/// rule choosing what it is about.
#[test]
fn a_binding_the_event_gave_is_not_replaced_by_another_order() {
    let (graph, program, sources) = recorded(&["orders"]);
    let mut world = World::new();
    let order = world.create("Order", "orders");
    world.set_field(&order, "welcomes", Value::Int(0));
    let event = Event::new("Order.created", "orders")
        .with("o", Value::Str("not an order".to_owned()))
        .with("other", Value::Ref(order.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let welcome = rule("Welcome", &outcome).expect("Welcome is woken");
    assert_eq!(welcome.disposition, Disposition::Undecided, "{welcome:?}");
    assert_eq!(outcome.world.instance(&order).unwrap().field("welcomes"), Value::Int(0));
}

fn offered(name: &str, trigger: &str, binding: &str, over: &[&EntityId]) -> Enabled {
    Enabled {
        rule: format!("orders::rule::{name}"),
        name: name.to_owned(),
        module: "orders".to_owned(),
        trigger: trigger.to_owned(),
        source: TriggerSource::State,
        over: over.iter().map(|id| Value::Ref((*id).clone())).collect(),
        binding: binding.to_owned(),
        undecided: Vec::new(),
        unsettled_over: Vec::new(),
    }
}

/// An outcome offering exactly `newly_enabled`, over a world holding `world`.
fn offering(world: World, newly_enabled: Vec<Enabled>) -> StepOutcome {
    StepOutcome {
        world,
        event: Event::new("Place", "orders"),
        rules: Vec::new(),
        invariants: Vec::new(),
        newly_enabled,
        emitted: Vec::new(),
    }
}

/// Two rules reacting to one order, calling it different things: one firing,
/// carrying both names. One per rule would run each of them twice.
#[test]
fn two_reactions_to_one_creation_are_one_firing_carrying_both_names() {
    let mut world = World::new();
    let first = world.create("Order", "orders");
    let second = world.create("Order", "orders");
    let outcome = offering(
        world,
        vec![
            offered("Welcome", "Order.created", "o", &[&first, &second]),
            offered("Audit", "Order.created", "order", &[&first, &second]),
        ],
    );

    let events = reactions(&outcome);
    assert_eq!(events.len(), 2, "one per order: {events:?}");
    for (event, id) in events.iter().zip([&first, &second]) {
        assert_eq!(event.trigger, "Order.created");
        assert_eq!(event.arguments.get("o"), Some(&Value::Ref(id.clone())));
        assert_eq!(event.arguments.get("order"), Some(&Value::Ref(id.clone())));
    }
}

/// A rule the world made true is offered beside a creation's reaction, and is
/// not one: firing it is settling, which is somebody else's business.
#[test]
fn a_state_rule_offered_beside_it_is_not_a_reaction() {
    let mut world = World::new();
    let order = world.create("Order", "orders");
    let outcome = offering(world, vec![offered("Ship", "Order", "o", &[&order])]);
    assert!(reactions(&outcome).is_empty());
}

/// A creation event carrying an instance of some other entity is not that
/// entity's creation. The browser can post one; firing `Notice` on a board
/// would be unknown taken for true.
#[test]
fn a_creation_event_carrying_another_entity_is_undecided_and_writes_nothing() {
    let (graph, program, sources) = recorded(&["notes", "pinboard"]);
    let mut world = World::new();
    let board = world.create("Board", "pinboard");
    let event = Event::new("Note.created", "notes").with("note", Value::Ref(board.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let notice = rule("Notice", &outcome).expect("Notice is woken");
    assert_eq!(notice.disposition, Disposition::Undecided, "{notice:?}");
    assert_eq!(notice.requires[0].truth, Truth::Unknown);
    assert!(!notice.requires[0].unresolved.is_empty(), "an unknown says why");
    assert!(!outcome.world.instance(&board).unwrap().fields.contains_key("seen"));
}

/// What a step made is offered only to the rules reacting to *its* entity:
/// pinning a note is not an order being placed.
#[test]
fn a_creation_is_offered_only_to_the_rules_reacting_to_its_entity() {
    let (graph, program, sources) = recorded(&["orders", "notes", "pinboard"]);
    let mut world = World::new();
    let board = world.create("Board", "pinboard");
    let event = Event::new("Pin", "pinboard")
        .with("board", Value::Ref(board))
        .with("text", Value::Str("hello".to_owned()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let offered: Vec<&str> =
        outcome.newly_enabled.iter().map(|candidate| candidate.name.as_str()).collect();
    assert!(offered.contains(&"Notice"), "{offered:?}");
    assert!(!offered.contains(&"Welcome"), "{offered:?}");
    let events = reactions(&outcome);
    let triggers: Vec<&str> = events.iter().map(|event| event.trigger.as_str()).collect();
    assert_eq!(triggers, ["Note.created"]);
}

/// Two modules, two entities called `Order`. The warehouse making one of its
/// own is filed in the warehouse — not in whichever module the graph lists
/// first — and the shop's reaction to *its* orders is not offered it.
#[test]
fn another_modules_order_is_filed_there_and_not_reacted_to_here() {
    let (graph, program, sources) = recorded(&["shop", "warehouse"]);
    let mut world = World::new();
    let dock = world.create("Order", "warehouse");
    let event = Event::new("Stock", "warehouse").with("o", Value::Ref(dock));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let stock = rule("Stock", &outcome).expect("Stock fires");
    let Some(Effect::Created { id, .. }) = stock.effects.first() else {
        panic!("Stock creates an order: {:?}", stock.effects)
    };
    let made = outcome.world.instance(id).expect("the order exists");
    assert_eq!(made.module, "warehouse", "the module that wrote it declares one");
    assert_eq!(made.field("crate"), Value::Int(1));
    assert!(outcome.newly_enabled.iter().all(|it| it.name != "Welcome"), "{outcome:?}");
    assert!(reactions(&outcome).is_empty());

    // Fired by hand as the warehouse's creation, it is not the shop's to
    // react to: the shop's rule is not woken at all, rather than woken to
    // report itself undecided and take whatever fired it down with it.
    let event = Event::new("Order.created", "warehouse").with("o", Value::Ref(id.clone()));
    let next = step(&graph, &program, &sources, &outcome.world, &event);
    assert!(rule("Welcome", &next).is_none(), "{:?}", next.rules);

    // Fired by hand naming no module, the shop's reaction is woken by the name
    // and says whose order it is rather than welcoming it.
    let event = Event::new("Order.created", "").with("o", Value::Ref(id.clone()));
    let next = step(&graph, &program, &sources, &outcome.world, &event);
    let welcome = rule("Welcome", &next).expect("Welcome is woken by the name");
    assert_eq!(welcome.disposition, Disposition::Undecided, "{welcome:?}");
    let why = &welcome.unresolved[0].reason;
    assert!(why.contains("warehouse") && why.contains("shop"), "{why}");
    assert!(!next.world.instance(id).unwrap().fields.contains_key("welcomed"));
}

/// The shop's own order is still welcomed, through the same filter.
#[test]
fn this_modules_order_is_still_reacted_to_beside_another_modules() {
    let (graph, program, sources) = recorded(&["shop", "warehouse"]);
    let mut world = World::new();
    let till = world.create("Order", "shop");
    let event = Event::new("Buy", "shop").with("o", Value::Ref(till));
    let outcome = step(&graph, &program, &sources, &world, &event);
    let events = reactions(&outcome);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].module, "shop");
    let next = step(&graph, &program, &sources, &outcome.world, &events[0]);
    assert_eq!(rule("Welcome", &next).expect("woken").disposition, Disposition::Fired);
}

/// A receipt made in the shop is not an order made in the shop: the same
/// module, another entity, and nothing reacting to orders is offered it.
#[test]
fn a_creation_of_another_entity_in_the_same_module_is_not_offered() {
    let (graph, program, sources) = recorded(&["shop"]);
    let mut world = World::new();
    let till = world.create("Order", "shop");
    let event = Event::new("Pay", "shop").with("o", Value::Ref(till));
    let outcome = step(&graph, &program, &sources, &world, &event);
    assert!(
        rule("Pay", &outcome).is_some_and(|pay| pay.disposition == Disposition::Fired),
        "{outcome:?}"
    );
    assert!(outcome.newly_enabled.iter().all(|it| it.name != "Welcome"), "{outcome:?}");
    assert!(reactions(&outcome).is_empty());
}

/// And fired by hand about the receipt, `o: Order.created` is not satisfied
/// by something that is not an order, though it comes from the right module.
#[test]
fn a_creation_event_carrying_another_entity_of_the_same_module_is_undecided() {
    let (graph, program, sources) = recorded(&["shop"]);
    let mut world = World::new();
    let receipt = world.create("Receipt", "shop");
    let event = Event::new("Order.created", "shop").with("o", Value::Ref(receipt.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let welcome = rule("Welcome", &outcome).expect("Welcome is woken");
    assert_eq!(welcome.disposition, Disposition::Undecided, "{welcome:?}");
    assert_eq!(welcome.requires[0].truth, Truth::Unknown);
    assert!(!welcome.requires[0].unresolved.is_empty(), "an unknown says why");
    assert!(!outcome.world.instance(&receipt).unwrap().fields.contains_key("welcomed"));
}

/// A creation wakes only the reactions to its own entity, in its own module
/// too: the shop makes a `Receipt` beside its `Order`, and a receipt being
/// made is not an order being made — whoever fired it, and whatever module
/// it names.
#[test]
fn a_creation_wakes_no_reaction_to_another_entity_of_its_module() {
    let (graph, program, sources) = recorded(&["shop", "warehouse"]);
    let mut world = World::new();
    let till = world.create("Order", "shop");
    let paid = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("Pay", "shop").with("o", Value::Ref(till)),
    );
    assert!(reactions(&paid).is_empty(), "{:?}", paid.newly_enabled);
    let Some(Effect::Created { id, .. }) = rule("Pay", &paid).and_then(|it| it.effects.first())
    else {
        panic!("Pay makes a receipt: {paid:?}")
    };

    let event = Event::new("Receipt.created", "shop").with("o", Value::Ref(id.clone()));
    let next = step(&graph, &program, &sources, &paid.world, &event);
    let woken: Vec<&str> = next.rules.iter().map(|it| it.name.as_str()).collect();
    assert!(woken.is_empty(), "{woken:?}");
}

/// A creation fired by position, as the browser and a journey fire an act,
/// still carries the instance: the rule's own name for it is not needed to
/// find the one `Order` it holds.
#[test]
fn a_creation_fired_by_position_carries_its_instance() {
    let (graph, program, sources) = recorded(&["orders"]);
    let mut world = World::new();
    let made = world.create("Order", "orders");
    world.set_field(&made, "welcomes", Value::Int(0));
    let event = Event::new("Order.created", "orders").passing(Value::Ref(made.clone()));
    let next = step(&graph, &program, &sources, &world, &event);
    let welcome = rule("Welcome", &next).expect("woken");
    assert_eq!(welcome.disposition, Disposition::Fired, "{welcome:?}");
    assert_eq!(next.world.instance(&made).unwrap().field("welcomes"), Value::Int(1));
}
