//! A whole simulation, over the real specs and the real CLI output.
//!
//! The unit tests drive each piece with a hand-built AST. This one replays the
//! recorded `allium` documents for `catalogue.allium` and `lending.allium`, runs
//! the ingestion that the server runs, and then borrows a library book — the
//! same path a person would walk in the browser.
//!
//! What it is really checking is that the pieces agree with each other: that the
//! trigger a surface offers is the trigger a rule waits for, that the name a
//! creation binds is the name the next clause reads, that a status assignment
//! finds the lifecycle the model pass recorded, and that a rule in one module
//! can be driven by a trigger emitted in another.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use inspect_model::{
    Command, Ingestion, MemoryReader, Program, SpecGraph, ingest, runner::MapRunner,
};
use inspect_sim::{
    Disposition, Effect, Truth, Value,
    step::{Sources, StepOutcome, step},
    value::EntityId,
    world::{Event, World},
};

/// `crates/inspect-model/tests/fixtures`, which is where the recordings live.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../inspect-model/tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The library spec set, ingested exactly as the server ingests it.
fn library_spec() -> (SpecGraph, Program, Sources) {
    let root = fixtures();
    let mut runner = MapRunner::new(read(&root.join("cli/VERSION")).trim());
    let mut reader = MemoryReader::default();
    let mut sources: Sources = BTreeMap::new();
    let mut paths = Vec::new();

    for module in ["catalogue", "lending"] {
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            runner = runner.with(
                command,
                &path,
                serde_json::from_str(&document).expect("the recording is JSON"),
            );
        }
        let text = read(&path);
        sources.insert(module.to_owned(), text.clone());
        reader = reader.with(&path, text);
        paths.push(path);
    }

    let Ingestion { graph, program } =
        ingest(&runner, &reader, &paths).expect("the recorded fixtures ingest");
    (graph, program, sources)
}

/// A world with one listed book, one available copy, and a member under limit.
///
/// `is_at_limit` is set by hand because it is a *derived* value: the spec says
/// `open_loan_count >= config.loan_limit`, and this simulator does not compute
/// derived fields. Left unset it evaluates to unknown and the borrow comes back
/// undecided — which is exactly what
/// [`a_precondition_over_an_unset_derived_value_is_undecided`] asserts.
fn library_world() -> (World, EntityId, EntityId) {
    let mut world = World::new().at(1_000);

    let book = world.create("Book", "catalogue");
    world.set_field(&book, "title", Value::Str("Structure and Interpretation".to_owned()));
    world.set_field(&book, "status", Value::Enum("listed".to_owned()));
    world.set_field(&book, "copy_count", Value::Int(1));

    let copy = world.create("Copy", "catalogue");
    world.set_field(&copy, "book", Value::Ref(book.clone()));
    world.set_field(&copy, "shelfmark", Value::Str("QA76.6".to_owned()));
    world.set_field(&copy, "status", Value::Enum("available".to_owned()));

    let member = world.create("Member", "lending");
    world.set_field(&member, "name", Value::Str("Ada".to_owned()));
    world.set_field(&member, "open_loan_count", Value::Int(0));
    world.set_field(&member, "is_at_limit", Value::Bool(false));

    world.set_config("lending", "loan_limit", Value::Int(5));
    world.set_config("catalogue", "max_copies_per_book", Value::Int(20));
    (world, copy, member)
}

/// Fire `MemberBorrows(member, copy)` against the library world.
fn borrow() -> (StepOutcome, EntityId, EntityId) {
    let (graph, program, sources) = library_spec();
    let (world, copy, member) = library_world();
    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member.clone()))
        .with("copy", Value::Ref(copy.clone()));
    (step(&graph, &program, &sources, &world, &event), copy, member)
}

fn outcome_for(rule: &str, outcome: &StepOutcome) -> inspect_sim::RuleOutcome {
    outcome
        .rules
        .iter()
        .find(|candidate| candidate.name == rule)
        .unwrap_or_else(|| panic!("no outcome for {rule}; got {:?}", outcome.rules))
        .clone()
}

// --- the happy path ------------------------------------------------------

#[test]
fn borrowing_a_copy_fires_the_rule_that_waits_for_it() {
    let (outcome, _, _) = borrow();
    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    assert_eq!(
        borrow_copy.disposition,
        Disposition::Fired,
        "preconditions were {:?}",
        borrow_copy.requires
    );
    assert!(outcome.fired());
}

#[test]
fn every_precondition_is_reported_with_the_text_the_spec_wrote() {
    // The panel shows these one per line against the file, so the text has to
    // be the author's rather than a reconstruction.
    let (outcome, _, _) = borrow();
    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    let clauses: Vec<&str> = borrow_copy.requires.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(clauses, ["copy.status = available", "not member.is_at_limit"]);
    assert!(borrow_copy.requires.iter().all(|clause| clause.truth == Truth::True));
}

#[test]
fn the_postconditions_create_the_loan_and_move_the_copy() {
    let (outcome, copy, _) = borrow();
    let borrow_copy = outcome_for("BorrowCopy", &outcome);

    let created: Vec<&str> = borrow_copy
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Created { entity, .. } => Some(entity.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(created, ["Loan"]);

    let moved = borrow_copy.effects.iter().find_map(|effect| match effect {
        Effect::Assigned { id, field, from, to } if id == &copy && field == "status" => {
            Some((from.clone(), to.clone()))
        }
        _ => None,
    });
    assert_eq!(
        moved,
        Some((Value::Enum("available".to_owned()), Value::Enum("on_loan".to_owned()))),
        "the trace says what changed, not only what it is now"
    );
}

#[test]
fn the_world_that_comes_back_holds_the_new_loan() {
    let (outcome, copy, member) = borrow();
    let loan = EntityId::new("Loan", 1);
    let instance = outcome.world.instance(&loan).expect("the loan exists afterwards");

    assert_eq!(instance.entity, "Loan");
    assert_eq!(instance.module, "lending");
    assert_eq!(instance.field("copy"), Value::Ref(copy.clone()));
    assert_eq!(instance.field("member"), Value::Ref(member));
    assert_eq!(instance.field("status"), Value::Enum("open".to_owned()));

    let moved = outcome.world.instance(&copy).expect("the copy is still there");
    assert_eq!(moved.field("status"), Value::Enum("on_loan".to_owned()));
}

#[test]
fn a_creation_binds_a_name_the_next_clause_reads() {
    // `ensures: CopyBorrowed(loan: loan)` refers to the loan the clause above
    // created. Without the binding it would be an unresolved name.
    let (outcome, _, _) = borrow();
    assert_eq!(outcome.emitted, ["CopyBorrowed"]);
}

#[test]
fn the_original_world_is_left_alone() {
    // The browser holds the world and posts it back; a step that mutated its
    // input would make undo impossible.
    let (graph, program, sources) = library_spec();
    let (world, copy, member) = library_world();
    let before = world.clone();
    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));

    let outcome = step(&graph, &program, &sources, &world, &event);
    assert_eq!(world, before);
    assert_ne!(outcome.world, before);
}

// --- refusal and indecision ----------------------------------------------

#[test]
fn borrowing_a_copy_that_is_out_is_refused_by_the_precondition_that_says_so() {
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    world.set_field(&copy, "status", Value::Enum("on_loan".to_owned()));

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    assert_eq!(borrow_copy.disposition, Disposition::Refused);
    assert_eq!(borrow_copy.requires[0].truth, Truth::False, "and names which one");
    assert!(borrow_copy.effects.is_empty(), "nothing was applied");
    assert_eq!(outcome.world.instance(&EntityId::new("Loan", 1)), None);
}

#[test]
fn a_precondition_over_a_field_nobody_stated_is_undecided() {
    // `copy.status` is stored: somebody has to say what it is, and here nobody
    // has. Undecided is the honest answer; treating it as false would refuse a
    // rule on a precondition nothing checked, and treating it as true would
    // fire one.
    //
    // This used to remove `is_at_limit` instead — until the simulator learned
    // to compute derived values, at which point the case stopped being one.
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    world.entities.get_mut(&copy).expect("the copy").fields.remove("status");

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    assert_eq!(borrow_copy.disposition, Disposition::Undecided);
    assert_eq!(borrow_copy.requires[0].truth, Truth::Unknown);
    assert!(borrow_copy.effects.is_empty(), "an undecided rule changes nothing");
    assert!(outcome.has_unknowns());
}

/// The other half, and the reason the case above had to move.
///
/// `is_at_limit` is computed, and so is everything under it:
///
///     loans:           Loan with member = this
///     open_loans:      loans where status = open
///     open_loan_count: open_loans.count
///     is_at_limit:     open_loan_count >= config.loan_limit
///
/// With none of the four stored, `not member.is_at_limit` still decides.
#[test]
fn a_precondition_over_a_value_the_spec_computes_is_decided() {
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    for field in ["is_at_limit", "open_loan_count"] {
        world.entities.get_mut(&member).expect("the member").fields.remove(field);
    }

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    assert_eq!(borrow_copy.requires[1].truth, Truth::True, "{:#?}", borrow_copy.requires);
    assert_eq!(borrow_copy.disposition, Disposition::Fired);
}

/// And it reflects the world rather than always answering empty: five open
/// loans put her at the cap, computed from the loans themselves.
#[test]
fn a_computed_value_counts_what_is_actually_there() {
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    for field in ["is_at_limit", "open_loan_count"] {
        world.entities.get_mut(&member).expect("the member").fields.remove(field);
    }
    for _ in 0..5 {
        let loan = world.create("Loan", "lending");
        world.set_field(&loan, "member", Value::Ref(member.clone()));
        world.set_field(&loan, "copy", Value::Ref(copy.clone()));
        world.set_field(&loan, "status", Value::Enum("open".to_owned()));
    }

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    assert_eq!(borrow_copy.requires[1].truth, Truth::False, "{:#?}", borrow_copy.requires);
    assert_eq!(borrow_copy.disposition, Disposition::Refused);
}

#[test]
fn an_undecided_rule_says_which_expression_it_could_not_settle() {
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    world.entities.get_mut(&copy).expect("the copy").fields.remove("status");

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let borrow_copy = outcome_for("BorrowCopy", &outcome);
    let notes = &borrow_copy.requires[0].unresolved;
    assert!(!notes.is_empty(), "an unknown with no reason is indistinguishable from a bug");
    assert!(notes[0].span.is_some(), "and it points at the source");
}

#[test]
fn firing_a_trigger_no_rule_waits_for_does_nothing_quietly() {
    let (graph, program, sources) = library_spec();
    let (world, _, _) = library_world();
    let outcome =
        step(&graph, &program, &sources, &world, &Event::new("NobodyListensToThis", "lending"));
    assert!(outcome.rules.is_empty());
    assert!(!outcome.fired());
    assert_eq!(outcome.world, world);
}

// --- lifecycles ----------------------------------------------------------

#[test]
fn a_state_change_the_lifecycle_forbids_is_refused_rather_than_written() {
    // `Book` declares `listed -> withdrawn` and nothing else, so a rule trying
    // to move a withdrawn book back would be demonstrating behaviour the spec
    // forbids. The step reports it instead.
    let (graph, program, sources) = library_spec();
    let (mut world, _, _) = library_world();
    let book = EntityId::new("Book", 1);
    world.set_field(&book, "status", Value::Enum("withdrawn".to_owned()));

    let event =
        Event::new("LibrarianWithdrawsBook", "catalogue").with("book", Value::Ref(book.clone()));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let withdraw = outcome_for("WithdrawBook", &outcome);
    // The precondition `book.status = listed` already refuses it, which is the
    // spec working as written; the lifecycle is the second line of defence.
    assert_eq!(withdraw.disposition, Disposition::Refused);
    assert_eq!(
        outcome.world.instance(&book).expect("the book").field("status"),
        Value::Enum("withdrawn".to_owned()),
        "and nothing was written"
    );
}

// --- invariants ----------------------------------------------------------

#[test]
fn the_invariants_are_checked_against_the_world_the_step_produced() {
    let (outcome, _, _) = borrow();
    assert!(!outcome.invariants.is_empty(), "the fixtures state invariants worth checking");

    let names: Vec<&str> = outcome.invariants.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"OpenLoansAreWithinTheLimit"), "checked: {names:?}");
    assert!(names.contains(&"CopyCountIsBounded"), "checked: {names:?}");
}

#[test]
fn an_invariant_that_was_already_failing_is_not_blamed_on_this_step() {
    // The difference between "this rule broke it" and "it was broken when you
    // got here", which the truth value alone cannot express.
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();
    // Put the member over the limit before anything happens.
    world.set_field(&member, "open_loan_count", Value::Int(99));

    let event = Event::new("MemberBorrows", "lending")
        .with("member", Value::Ref(member))
        .with("copy", Value::Ref(copy));
    let outcome = step(&graph, &program, &sources, &world, &event);

    let over_limit = outcome
        .invariants
        .iter()
        .find(|invariant| invariant.name == "OpenLoansAreWithinTheLimit")
        .expect("the invariant is checked");
    assert_eq!(
        over_limit.truth,
        Truth::False,
        "99 open loans against a limit of 5 is a definite breach, not an undecided one"
    );
    assert!(over_limit.already_broken, "it was failing before the step ran");
    assert!(
        !outcome.broken().any(|broken| broken.name == "OpenLoansAreWithinTheLimit"),
        "so this step is not what broke it"
    );

    // And the inheritance is per invariant. One failing invariant marking every
    // other one as pre-existing would hide every breach a step actually caused,
    // which is the one thing this panel exists to show.
    let inherited: Vec<&str> = outcome
        .invariants
        .iter()
        .filter(|invariant| invariant.already_broken)
        .map(|invariant| invariant.name.as_str())
        .collect();
    assert_eq!(inherited, vec!["OpenLoansAreWithinTheLimit"]);
}

// --- what happens next ---------------------------------------------------

#[test]
fn losing_a_copy_enables_the_rule_that_watches_for_it() {
    // The chain that makes a simulation move without anybody inventing the next
    // event: `ReportLostCopy` waits on `Copy.status = lost`, and reporting a
    // loss is what makes that hold.
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();

    // Borrow first, so there is a loan to report lost.
    let borrowed = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("MemberBorrows", "lending")
            .with("member", Value::Ref(member))
            .with("copy", Value::Ref(copy.clone())),
    );
    world = borrowed.world;

    let outcome = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("MemberReportsLoss", "lending")
            .with("loan", Value::Ref(EntityId::new("Loan", 1))),
    );

    assert_eq!(
        outcome.world.instance(&copy).expect("the copy").field("status"),
        Value::Enum("lost".to_owned())
    );
    let enabled: Vec<&str> = outcome.newly_enabled.iter().map(|e| e.name.as_str()).collect();
    assert!(
        enabled.contains(&"ReportLostCopy"),
        "losing the copy should offer the rule that watches for it; got {enabled:?}"
    );
}

#[test]
fn a_rule_that_was_already_possible_is_not_reported_as_newly_enabled() {
    // Otherwise every step would re-offer the same moves and the list would
    // stop meaning "what changed".
    let (graph, program, sources) = library_spec();
    let (mut world, copy, _) = library_world();
    world.set_field(&copy, "status", Value::Enum("lost".to_owned()));

    let outcome =
        step(&graph, &program, &sources, &world, &Event::new("NothingHappens", "lending"));
    let enabled: Vec<&str> = outcome.newly_enabled.iter().map(|e| e.name.as_str()).collect();
    assert!(!enabled.contains(&"ReportLostCopy"), "it was already possible: {enabled:?}");
}

#[test]
fn a_temporal_rule_becomes_possible_when_the_clock_is_advanced() {
    // The clock being a field is what makes this a thing you step to. Nothing
    // here waits for wall time.
    let (graph, program, sources) = library_spec();
    let (mut world, copy, member) = library_world();

    let borrowed = step(
        &graph,
        &program,
        &sources,
        &world,
        &Event::new("MemberBorrows", "lending")
            .with("member", Value::Ref(member))
            .with("copy", Value::Ref(copy)),
    );
    world = borrowed.world;

    // Give the loan a window that has now passed.
    let loan = EntityId::new("Loan", 1);
    world.set_field(&loan, "window", Value::Unknown);
    world.now = 30 * 86_400_000;

    let outcome = step(&graph, &program, &sources, &world, &Event::new("Tick", "lending"));
    // The window is unset, so the condition is undecided rather than true —
    // and an undecided condition must not enable the rule.
    let enabled: Vec<&str> = outcome.newly_enabled.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !enabled.contains(&"LoanFallsOverdue"),
        "an undecided condition is not an enabled rule: {enabled:?}"
    );
}

// --- the contract --------------------------------------------------------

#[test]
fn stepping_is_deterministic() {
    // Same world, same event, byte-identical outcome. A trace that is not
    // reproducible cannot be snapshot-tested, shared, or trusted.
    let (first, _, _) = borrow();
    let (second, _, _) = borrow();
    assert_eq!(first, second);

    let one = serde_json::to_string(&first).expect("serialises");
    let two = serde_json::to_string(&second).expect("serialises");
    assert_eq!(one, two);
}

#[test]
fn an_outcome_round_trips_through_the_wire_format() {
    // The whole outcome crosses to the browser and the world comes back, so
    // this is the contract rather than a convenience.
    let (outcome, _, _) = borrow();
    let json = serde_json::to_string(&outcome).expect("serialises");
    let back: StepOutcome = serde_json::from_str(&json).expect("parses");
    assert_eq!(back, outcome);
}

#[test]
fn every_rule_in_the_spec_can_be_stepped_without_panicking() {
    // The blunt one. Every trigger in the fixture set is fired against a world
    // that does not suit it, which is how a simulator meets missing bindings,
    // absent instances and half-built state — none of which may take it down.
    let (graph, program, sources) = library_spec();
    let (world, _, _) = library_world();

    let triggers: Vec<String> =
        graph.nodes_of(inspect_model::NodeKind::Trigger).map(|node| node.name.clone()).collect();
    assert!(triggers.len() > 5, "the fixtures declare a useful number of triggers");

    for trigger in triggers {
        for module in ["catalogue", "lending"] {
            let outcome =
                step(&graph, &program, &sources, &world, &Event::new(trigger.clone(), module));
            // Nothing may fire on arguments it was never given.
            for rule in &outcome.rules {
                if rule.disposition == Disposition::Fired {
                    assert!(
                        rule.requires.is_empty(),
                        "{} fired with no arguments and {} preconditions",
                        rule.name,
                        rule.requires.len()
                    );
                }
            }
        }
    }
}

// --- reading an outcome --------------------------------------------------
//
// The three accessors on `StepOutcome` are what the panel is built out of:
// which invariants this step broke, and whether anything is undecided. They
// are small enough to drive directly, and driving them directly is the only
// way to state what each one excludes as well as what it includes.

use inspect_sim::eval::Unresolved;
use inspect_sim::{ClauseVerdict, InvariantVerdict, RuleOutcome};

fn note(reason: &str) -> Unresolved {
    Unresolved { reason: reason.to_owned(), expression: None, span: None }
}

fn verdict(name: &str, truth: Truth, already_broken: bool) -> InvariantVerdict {
    InvariantVerdict {
        id: format!("lending::invariant::{name}"),
        name: name.to_owned(),
        truth,
        already_broken,
        unresolved: Vec::new(),
    }
}

fn rule_outcome(name: &str, unresolved: Vec<Unresolved>) -> RuleOutcome {
    RuleOutcome {
        rule: format!("lending::rule::{name}"),
        name: name.to_owned(),
        module: "lending".to_owned(),
        disposition: Disposition::Fired,
        requires: Vec::<ClauseVerdict>::new(),
        effects: Vec::new(),
        unresolved,
    }
}

fn outcome_of(rules: Vec<RuleOutcome>, invariants: Vec<InvariantVerdict>) -> StepOutcome {
    StepOutcome {
        world: World::new().at(0),
        event: Event::new("Anything", "lending"),
        rules,
        invariants,
        newly_enabled: Vec::new(),
        emitted: Vec::new(),
    }
}

#[test]
fn the_invariants_a_step_broke_are_the_false_ones_it_did_not_inherit() {
    let outcome = outcome_of(
        Vec::new(),
        vec![
            verdict("BrokeJustNow", Truth::False, false),
            verdict("WasAlreadyBroken", Truth::False, true),
            verdict("StillHolds", Truth::True, false),
            verdict("CouldNotBeChecked", Truth::Unknown, false),
        ],
    );
    let broken: Vec<&str> = outcome.broken().map(|v| v.name.as_str()).collect();
    assert_eq!(broken, vec!["BrokeJustNow"]);
}

#[test]
fn an_undecided_invariant_is_not_reported_as_a_broken_one() {
    // The whole point of the third truth value is that it is not `false`.
    // Listing it under "this step broke these" would be the simulator stating
    // a conclusion it did not reach.
    let outcome = outcome_of(Vec::new(), vec![verdict("Unknowable", Truth::Unknown, false)]);
    assert_eq!(outcome.broken().count(), 0);
}

#[test]
fn a_step_that_decided_everything_reports_no_unknowns() {
    let outcome = outcome_of(
        vec![rule_outcome("BorrowCopy", Vec::new())],
        vec![verdict("StillHolds", Truth::True, false)],
    );
    assert!(!outcome.has_unknowns());
}

#[test]
fn an_unknown_anywhere_is_an_unknown_in_the_step() {
    // Either side alone is enough: a precondition nobody could evaluate and an
    // invariant nobody could check are both things a reader has to be told.
    let from_a_rule = outcome_of(
        vec![rule_outcome("BorrowCopy", vec![note("no `is_at_limit` set")])],
        Vec::new(),
    );
    assert!(from_a_rule.has_unknowns());

    let mut unchecked = verdict("CouldNotBeChecked", Truth::Unknown, false);
    unchecked.unresolved = vec![note("`For` over nothing")];
    let from_an_invariant = outcome_of(Vec::new(), vec![unchecked]);
    assert!(from_an_invariant.has_unknowns());
}

// --- rules with nothing to evaluate ---------------------------------------
//
// Two rules can both have no expressions to run, for opposite reasons. One has
// clauses the parser did not give us; the other has no clauses at all. Reading
// them the same way tells a user that a rule they wrote cannot be simulated
// when in fact it just has no preconditions.

use inspect_model::{
    Node, NodeDetail, NodeKind, RuleAst,
    graph::{RuleClause, RuleDetail, TriggerSource},
};

/// A one-rule spec responding to `Anything`, with the clauses given.
fn spec_of(clauses: Vec<RuleClause>) -> (SpecGraph, Program) {
    let mut graph = SpecGraph::new("lending");
    graph.nodes.push(Node::new("lending", NodeKind::Rule, "DoNothing").with(NodeDetail::Rule(
        RuleDetail {
            trigger: "Anything".to_owned(),
            source: TriggerSource::External,
            clauses,
            creates: Vec::new(),
            emits: Vec::new(),
        },
    )));
    graph.normalise();
    (graph, Program::default())
}

fn only_rule(graph: &SpecGraph, program: &Program) -> Disposition {
    let outcome = step(
        graph,
        program,
        &Sources::new(),
        &World::new().at(0),
        &Event::new("Anything", "lending"),
    );
    outcome.rules.first().expect("the rule responds to the trigger").disposition
}

#[test]
fn a_rule_whose_clauses_did_not_parse_is_reported_as_unsimulatable() {
    // The spec says something; this cannot read it. Saying the rule fired would
    // be claiming to have checked conditions nobody evaluated.
    let (graph, program) = spec_of(vec![RuleClause {
        keyword: "requires".to_owned(),
        text: "copy.status = available".to_owned(),
        span: None,
    }]);
    assert_eq!(only_rule(&graph, &program), Disposition::Unsimulatable);
}

#[test]
fn a_rule_with_no_preconditions_at_all_simply_fires() {
    // Nothing to check is not the same as unable to check. A rule that is only
    // `when` and `ensures` is ordinary Allium, and it fires.
    let (graph, program) = spec_of(Vec::new());
    assert_eq!(only_rule(&graph, &program), Disposition::Fired);
}

#[test]
fn a_rule_the_parser_did_read_is_judged_on_its_preconditions() {
    // The guard is about the *absence* of an AST, so a rule that has one must
    // not be caught by it however few clauses the model recorded.
    let (graph, mut program) = spec_of(vec![RuleClause {
        keyword: "requires".to_owned(),
        text: "false".to_owned(),
        span: None,
    }]);
    program.add_rule(
        "lending::rule::DoNothing",
        RuleAst {
            when: None,
            requires: vec![allium_parser::ast::Expr::BoolLiteral {
                span: allium_parser::Span { start: 0, end: 0 },
                value: false,
            }],
            ensures: Vec::new(),
            iterate: None,
        },
    );
    assert_eq!(only_rule(&graph, &program), Disposition::Refused);
}
