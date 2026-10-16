//! Walking journeys through a creation and what reacts to it (issue #9).
//!
//! Two halves, one per fixture. `orders` reacts to its own entity being
//! created — `when: o: Order.created` — which the walk recorded and never
//! reacted to. `pinboard` creates an entity another module declares, through a
//! qualified name the simulator did not read as a creation at all, so
//! `creating n: notes/Note` caught nothing.
//!
//! Against the recorded `allium` output and the real simulator, so what passes
//! here is a journey the specification actually supports.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{CELLS, ORDERS, PINBOARD, SHOPS, TICKETS};
use inspect_journey::{Verdict, Walk, parse, walk};

fn walked(source: &str, modules: &[&str]) -> Walk {
    let journeys = parse(source).expect("the journey parses");
    let (graph, program, sources) = common::library(modules);
    walk(&journeys[0], &journeys, &graph, &program, &sources)
}

/// Every outcome, flattened, with the text it was written as.
fn outcomes(walk: &Walk) -> Vec<(Verdict, String, Option<String>)> {
    walk.steps
        .iter()
        .flat_map(|step| step.outcomes.iter())
        .map(|outcome| (outcome.verdict, outcome.about.clone(), outcome.detail.clone()))
        .collect()
}

/// The outcome of the line that reads `about`.
fn line(walk: &Walk, about: &str) -> (Verdict, Option<String>) {
    outcomes(walk)
        .into_iter()
        .find(|(_, written, _)| written == about)
        .map(|(verdict, _, detail)| (verdict, detail))
        .unwrap_or_else(|| panic!("no line `{about}` in {:#?}", outcomes(walk)))
}

/// Every line of a walk held, and so did every step.
fn all_held(walk: &Walk) {
    let bad: Vec<_> = outcomes(walk)
        .into_iter()
        .filter(|(verdict, _, _)| *verdict != Verdict::Specified)
        .collect();
    assert!(bad.is_empty(), "every line should hold: {bad:#?}");
    for step in &walk.steps {
        assert_eq!(step.verdict(), Verdict::Specified, "step {}", step.number);
    }
}

const ORDERING: &str = include_str!("fixtures/orders.journey");
const PINNING: &str = include_str!("fixtures/pinboard.journey");

/// The reproduction from the issue, end to end.
#[test]
fn a_rule_waiting_on_a_creation_fires_when_the_act_creates_one() {
    let walk = walked(ORDERING, ORDERS);
    all_held(&walk);
}

/// Once per creation, about the instance created — not once per rule that
/// happened to be listening, and not again for an order created earlier.
#[test]
fn each_creation_is_reacted_to_once_and_only_about_itself() {
    let source = "journey J {
    cast:
        seed: Order
    1. two orders, one after the other
        seed does Place(seed) on Shop
            creating first: Order
        seed does Place(seed) on Shop
            creating second: Order
        then first.welcomes = 1
        then second.welcomes = 1
}";
    let walk = walked(source, ORDERS);
    all_held(&walk);
}

/// Somebody already in the cast was not created by anything the journey did,
/// so nothing reacts to them having been.
#[test]
fn the_cast_is_not_a_creation() {
    let source = "journey J {
    cast:
        seed: Order
    given:
        seed.welcomes = 0
    1. placing a new order leaves the seed alone
        seed does Place(seed) on Shop
            creating fresh: Order
        then seed.welcomes = 0
        then fresh.welcomes = 1
}";
    let walk = walked(source, ORDERS);
    all_held(&walk);
}

/// The second half of the issue, end to end: a qualified creation is a
/// creation, `creating` catches it, and a qualified reaction reacts to it.
#[test]
fn a_creation_in_another_module_is_caught_and_reacted_to() {
    let walk = walked(PINNING, PINBOARD);
    all_held(&walk);
    assert_eq!(line(&walk, "then Notice fires").0, Verdict::Specified);
}

/// Made and unmade in one act: nothing was there to react to, and the line
/// saying so is decided — a creation rule is not a condition over the world
/// whose falsity nobody could see.
#[test]
fn a_creation_removed_in_the_same_act_is_not_reacted_to() {
    let source = "journey J {
    cast:
        seed: Order
    1. an order made and withdrawn
        seed does Retract(seed) on Shop
        then Welcome does not fire
}";
    let walk = walked(source, ORDERS);
    all_held(&walk);
}

/// An act that creates nothing wakes no reaction, and that is decided too.
#[test]
fn an_act_that_creates_nothing_wakes_no_reaction() {
    let source = "journey J {
    cast:
        t: Ticket
    1. touching a ticket makes none
        t does Touch(t) on Counter
        then Greet does not fire
        then Split does not fire
}";
    let walk = walked(source, TICKETS);
    all_held(&walk);
}

/// A reaction its `requires` refused is refused, quoting the clause — not
/// "never became true", which is about the tool rather than the spec.
#[test]
fn a_reaction_refused_by_its_precondition_says_which_clause() {
    let source = "journey J {
    cast:
        t: Ticket
    1. a child ticket is not greeted
        t does OpenChild(t) on Counter
            creating kid: Ticket
        then Greet does not fire
        then kid.greeted = false
}";
    all_held(&walked(source, TICKETS));

    let source = "journey J {
    cast:
        t: Ticket
    1. a child ticket is not greeted
        t does OpenChild(t) on Counter
            creating kid: Ticket
        then Greet fires
        then kid.greeted = false
}";
    let walk = walked(source, TICKETS);
    let (verdict, detail) = line(&walk, "then Greet fires");
    assert_eq!(verdict, Verdict::Refused, "{detail:?}");
    let detail = detail.expect("a refusal says why");
    assert!(detail.contains("`Greet` refused") && detail.contains("t.placed = true"), "{detail}");
    // Nothing in the step was undecided, so the line after it is not told so.
    assert_eq!(line(&walk, "then kid.greeted = false"), (Verdict::Specified, None));
}

/// A reaction reached through an emission: `Queue` makes nothing and emits
/// `Open`, which does, and the ticket it made is greeted.
#[test]
fn a_creation_made_one_emission_away_is_caught_and_reacted_to() {
    let source = "journey J {
    cast:
        t: Ticket
    1. queueing opens a ticket
        t does Queue(t) on Counter
            creating fresh: Ticket
        then Open fires
        then Greet fires
        then fresh.greeted = true
}";
    all_held(&walked(source, TICKETS));
}

/// `creating` catches what the act made, not what a reaction to it made — and
/// not whichever sorts first as text, which is `Ticket#10` before `Ticket#9`
/// once eight tickets are already there.
#[test]
fn creating_catches_the_acts_own_instance_whatever_the_count() {
    let source = "journey J {
    cast:
        a: Ticket
        b: Ticket
        c: Ticket
        d: Ticket
        e: Ticket
        f: Ticket
        g: Ticket
        h: Ticket
    1. opening a ticket splits it
        a does Open(a) on Counter
            creating fresh: Ticket
        then Split fires
        then fresh.placed = true
        then fresh.child = false
}";
    all_held(&walked(source, TICKETS));
}

/// A reaction that makes another of what it reacts to never stops. The walk
/// says so, rather than reporting a world it cut off partway through.
#[test]
fn a_reaction_that_never_stops_creating_is_undecided_and_says_so() {
    let source = "journey J {
    cast:
        c: Cell
    1. one cell, and then another, and then another
        c does Seed(c) on Dish
            creating first: Cell
        then first.gen = 0
}";
    let walk = walked(source, CELLS);
    let (verdict, detail) = line(&walk, "c does Seed(c) on Dish");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let detail = detail.expect("it says why");
    assert!(detail.contains("Cell.created") && detail.contains("32 rounds"), "{detail}");
    // What the act itself made reads as it was made, and is not reported as
    // settled: nothing checked that a reaction past the bound leaves it alone.
    let (verdict, detail) = line(&walk, "then first.gen = 0");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let detail = detail.expect("it says why");
    assert!(detail.starts_with("first.gen is 0, but this world is not settled"), "{detail}");
    assert_eq!(walk.steps[0].verdict(), Verdict::Undecided);
}

/// The warehouse making its own `Order` is not a shop order being placed.
#[test]
fn another_modules_creation_of_the_same_name_is_not_reacted_to() {
    let source = "journey J {
    cast:
        s: Order
    1. stock arrives
        s does Stock(s) on Dock
            creating w: warehouse/Order
        then Welcome does not fire
        then w.crate = 1
}";
    all_held(&walked(source, SHOPS));

    let source = "journey J {
    cast:
        s: Order
    1. a sale
        s does Buy(s) on Till
            creating o: Order
        then Welcome fires
        then o.welcomed = true
}";
    all_held(&walked(source, SHOPS));
}

/// What ran is what ran in *this* step. Carrying the previous step's firings
/// over would let `Welcome`, which ran for the order placed in step 1, answer
/// for step 2, where nothing was left to welcome — and refuse a line that
/// holds.
#[test]
fn a_reaction_in_one_step_does_not_answer_for_the_next() {
    let source = "journey J {
    cast:
        seed: Order
    1. an order placed
        seed does Place(seed) on Shop
        then Welcome fires
    2. an order made and withdrawn
        seed does Retract(seed) on Shop
        then Welcome does not fire
}";
    let walk = walked(source, ORDERS);
    all_held(&walk);
}

/// A creation wakes only the reactions to its own entity. `tickets` reacts to
/// `Ticket.created`, twice over, and placing an order makes no ticket — so
/// neither of its reactions is woken to report itself undecided about an
/// order it was never written about.
#[test]
fn a_creation_wakes_only_the_reactions_to_its_own_entity() {
    let walk = walked(ORDERING, &["orders", "tickets"]);
    all_held(&walk);
    for (_, about, detail) in outcomes(&walk) {
        let detail = detail.unwrap_or_default();
        assert!(!detail.contains("Greet") && !detail.contains("Split"), "{about}: {detail}");
    }
}

/// Two modules each declaring an `Order`, each with a reaction to its own
/// being made. Placing one of `orders`' is not the shop's to react to: woken by
/// the name alone, the shop's `Welcome` reported itself undecided and took the
/// act that placed the order down with it.
#[test]
fn another_modules_reaction_to_the_same_name_is_not_woken() {
    let walk = walked(ORDERING, &["orders", "shop"]);
    all_held(&walk);
    let source = "journey J {
    cast:
        seed: Order
    1. placing an order is not a sale at the till
        seed does Place(seed) on Shop
            creating fresh: Order
        then fresh.welcomes = 1
}";
    all_held(&walked(source, &["orders", "shop"]));
}

/// A reaction that makes *two* of what it reacts to doubles every round, and
/// a bound on rounds alone would take hours to reach. The walk comes back,
/// undecided, saying what was still being set off.
#[test]
fn a_reaction_that_doubles_every_round_is_cut_off_and_says_so() {
    let source = "journey J {
    cast:
        s: Spore
    1. one spore, and then two, and then four
        s does Sow(s) on Petri
}";
    let walk = walked(source, CELLS);
    let (verdict, detail) = line(&walk, "s does Sow(s) on Petri");
    assert_eq!(verdict, Verdict::Undecided, "{detail:?}");
    let detail = detail.expect("it says why");
    assert!(
        detail.contains("`Spore.created` was still being set off after 512 firings"),
        "{detail}"
    );
}
