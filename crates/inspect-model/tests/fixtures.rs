//! The whole ingestion pipeline, over real recorded `allium` output.
//!
//! The unit tests each drive one pass with a hand-built document, which is how
//! their edge cases stay legible. This suite does the opposite: it replays the
//! four documents the real CLI printed for `tests/fixtures/specs/*.allium` and
//! asserts on the graph that comes out.
//!
//! Both tiers are needed. A hand-built document proves the code does what the
//! test author believed the CLI emits; only recorded output proves that belief
//! was right. The recordings are refreshed with `just refresh-fixtures` and the
//! CLI version that produced them is stamped alongside, so an upgrade that
//! changes a shape surfaces here rather than as an empty panel in the browser.

// `allow-panic-in-tests` and friends in clippy.toml only reach `#[cfg(test)]`
// modules; an integration test is its own target, so the allowance has to be
// stated here. It is the same judgement either way: a fixture that cannot be
// read or parsed is a broken test setup, and failing loudly at the line that
// found it says more than threading a Result through every helper.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inspect_model::{
    Command, Ingestion, MemoryReader, NodeId, NodeKind, SpecGraph,
    graph::{EdgeKind, EntityKind, TriggerSource},
    ingest,
    runner::MapRunner,
};

/// `crates/inspect-model/tests/fixtures`.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
}

/// The version stamped when the recordings were made.
fn recorded_version() -> String {
    read(&fixtures().join("cli/VERSION")).trim().to_owned()
}

/// A runner and reader replaying every recorded module.
fn replay() -> (MapRunner, MemoryReader, Vec<PathBuf>) {
    let root = fixtures();
    let mut runner = MapRunner::new(recorded_version());
    let mut reader = MemoryReader::default();
    let mut paths = Vec::new();

    for module in ["catalogue", "lending"] {
        // Ingestion is driven with the same path the CLI was run with, so the
        // module name it derives matches the recordings.
        let path = root.join(format!("specs/{module}.allium"));
        for command in Command::ALL {
            let document = read(&root.join(format!("cli/{module}.{command}.json")));
            let value = serde_json::from_str(&document)
                .unwrap_or_else(|error| panic!("{module}.{command}.json is JSON: {error}"));
            runner = runner.with(command, &path, value);
        }
        reader = reader.with(&path, read(&path));
        paths.push(path);
    }
    (runner, reader, paths)
}

fn ingested() -> Ingestion {
    let (runner, reader, paths) = replay();
    ingest(&runner, &reader, &paths).expect("the recorded fixtures ingest")
}

fn graph() -> SpecGraph {
    ingested().graph
}

fn id(module: &str, kind: NodeKind, name: &str) -> NodeId {
    NodeId::new(module, kind, name)
}

#[test]
fn the_installed_cli_still_matches_the_recordings() {
    // The gate that turns a CLI upgrade into one loud, specific failure rather
    // than a misparse three layers down. If this fails and nothing else does,
    // run `just refresh-fixtures` and read the diff.
    let Ok(output) = std::process::Command::new("allium").arg("--version").output() else {
        // Not installed here. The rest of this suite is fixture-driven and must
        // still run, which is the whole point of recording them.
        return;
    };
    let installed = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert_eq!(
        installed,
        recorded_version(),
        "the installed allium differs from the one the fixtures were recorded from.\n\
         Run `just refresh-fixtures` and review the diff."
    );
}

// journey: SomebodyMeetsASpecTheyDidNotWrite.2 — it finishes reading, and says
// how much there is. The other half of the pair a browser cannot reach.
#[test]
fn both_modules_are_ingested_with_their_language_version() {
    let graph = graph();
    let names: Vec<&str> = graph.modules.iter().map(|module| module.name.as_str()).collect();
    assert_eq!(names, ["catalogue", "lending"]);
    assert!(graph.modules.iter().all(|module| module.language_version == Some(3)));
}

#[test]
fn every_construct_the_fixtures_declare_becomes_a_node() {
    // The fixtures exist to cover the whole vocabulary, so a projection that
    // silently drops a construct fails here rather than rendering nothing.
    let graph = graph();
    for (module, kind, name) in [
        ("catalogue", NodeKind::Entity, "Book"),
        ("catalogue", NodeKind::Entity, "Copy"),
        ("catalogue", NodeKind::Entity, "Staff"),
        ("catalogue", NodeKind::Enum, "Medium"),
        ("catalogue", NodeKind::Config, "config"),
        ("catalogue", NodeKind::Rule, "AddBook"),
        ("catalogue", NodeKind::Rule, "ReportLostCopy"),
        ("catalogue", NodeKind::Trigger, "LibrarianAddsBook"),
        ("catalogue", NodeKind::Surface, "CatalogueDesk"),
        ("catalogue", NodeKind::Actor, "Librarian"),
        ("catalogue", NodeKind::Invariant, "CopyCountIsBounded"),
        ("lending", NodeKind::Entity, "Loan"),
        ("lending", NodeKind::Entity, "Member"),
        ("lending", NodeKind::Entity, "Reservation"),
        ("lending", NodeKind::Value, "LoanWindow"),
        ("lending", NodeKind::Rule, "BorrowCopy"),
        ("lending", NodeKind::Rule, "LoanFallsOverdue"),
        ("lending", NodeKind::Surface, "MemberShelf"),
        ("lending", NodeKind::Actor, "Reader"),
        ("lending", NodeKind::Invariant, "OpenLoansAreWithinTheLimit"),
    ] {
        assert!(
            graph.node(&id(module, kind, name)).is_some(),
            "{module}/{name} ({kind:?}) is missing from the graph"
        );
    }
}

#[test]
fn every_node_that_the_spec_declares_knows_where_it_is() {
    // Spans come only from `parse`, and only because the model pass runs first
    // and the parse pass locates what it made. A regression in that order
    // leaves the source panel blank for everything.
    let graph = graph();
    let unlocated: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|node| node.span.is_none() && node.kind != NodeKind::External)
        // A trigger is not declared anywhere: it is named by the rule that
        // waits for it and by the surface that offers it.
        .filter(|node| node.kind != NodeKind::Trigger)
        .map(|node| node.id.as_str())
        .collect();
    assert!(unlocated.is_empty(), "these nodes carry no span: {unlocated:?}");
}

#[test]
fn a_cross_module_field_links_the_two_modules() {
    // The headline claim: `Loan.copy: catalogue/Copy` becomes a real edge into
    // the other module, not a dangling name.
    let graph = graph();
    let loan = id("lending", NodeKind::Entity, "Loan");
    let edge =
        graph.edges_from(&loan).find(|edge| edge.label == "copy").expect("Loan.copy is an edge");
    assert_eq!(edge.to, id("catalogue", NodeKind::Entity, "Copy"));
}

#[test]
fn the_import_that_makes_that_link_possible_is_resolved() {
    let graph = graph();
    let lending = graph.modules.iter().find(|m| m.name == "lending").expect("lending");
    let import = lending.imports.iter().find(|i| i.alias == "catalogue").expect("the import");
    assert_eq!(import.target.as_deref(), Some("catalogue"));
    assert_eq!(import.path, "./catalogue.allium");
}

#[test]
fn no_reference_in_the_fixtures_is_left_unresolved() {
    // The fixtures are self-contained by construction, so any external node
    // here is a resolver bug rather than a fact about the spec.
    let graph = graph();
    let unresolved: Vec<&str> =
        graph.nodes_of(NodeKind::External).map(|node| node.id.as_str()).collect();
    assert!(unresolved.is_empty(), "unresolved references: {unresolved:?}");
}

#[test]
fn an_entity_carries_its_fields_relationships_and_lifecycle() {
    let graph = graph();
    let book = graph
        .node(&id("catalogue", NodeKind::Entity, "Book"))
        .and_then(|node| node.detail.as_entity())
        .expect("Book");

    assert_eq!(book.kind, EntityKind::Internal);
    assert_eq!(book.field("title").map(|f| f.type_expr.as_str()), Some("String"));
    assert!(book.field("copies").expect("the relationship").relationship);
    assert!(book.field("copy_count").expect("the derived value").derived);
    assert!(book.field("status").expect("the status").is_status());

    let lifecycle = book.transitions_for("status").expect("Book has a lifecycle");
    assert!(lifecycle.allows("listed", "withdrawn"));
    assert!(lifecycle.is_terminal("withdrawn"));
}

#[test]
fn an_external_entity_is_marked_as_governed_elsewhere() {
    let graph = graph();
    let staff = graph
        .node(&id("catalogue", NodeKind::Entity, "Staff"))
        .and_then(|node| node.detail.as_entity())
        .expect("Staff");
    assert_eq!(staff.kind, EntityKind::External);
}

#[test]
fn a_value_type_is_ingested_even_though_model_never_reports_one() {
    // `LoanWindow` appears in the `model` document only as a type expression on
    // `Loan.window`. Without the parse pass it would exist solely as an
    // unresolved name.
    let graph = graph();
    let window = graph
        .node(&id("lending", NodeKind::Value, "LoanWindow"))
        .and_then(|node| node.detail.as_entity())
        .expect("LoanWindow");
    assert_eq!(window.kind, EntityKind::Value);
    assert_eq!(window.field("due_at").map(|f| f.type_expr.as_str()), Some("Timestamp"));
}

#[test]
fn an_external_stimulus_and_a_state_condition_are_told_apart() {
    // The distinction the simulator is built on.
    let graph = graph();
    let source_of = |module: &str, rule: &str| {
        graph
            .node(&id(module, NodeKind::Rule, rule))
            .and_then(|node| node.detail.as_rule())
            .unwrap_or_else(|| panic!("rule {rule}"))
            .source
    };
    assert_eq!(source_of("lending", "BorrowCopy"), TriggerSource::External);
    assert_eq!(source_of("catalogue", "ReportLostCopy"), TriggerSource::State);
    assert_eq!(
        source_of("lending", "LoanFallsOverdue"),
        TriggerSource::Temporal,
        "`Loan.window.due_at <= now` reads the clock"
    );
}

#[test]
fn a_rules_clauses_are_the_text_the_author_wrote() {
    let graph = graph();
    let borrow = graph
        .node(&id("lending", NodeKind::Rule, "BorrowCopy"))
        .and_then(|node| node.detail.as_rule())
        .expect("BorrowCopy");

    let requires: Vec<&str> =
        borrow.clauses_of("requires").map(|clause| clause.text.as_str()).collect();
    assert_eq!(requires, ["copy.status = available", "not member.is_at_limit"]);
    assert_eq!(borrow.clauses_of("ensures").count(), 3);
}

#[test]
fn a_rule_knows_what_it_creates_and_emits() {
    let graph = graph();
    let borrow = graph
        .node(&id("lending", NodeKind::Rule, "BorrowCopy"))
        .and_then(|node| node.detail.as_rule())
        .expect("BorrowCopy");
    assert_eq!(borrow.creates, ["Loan"]);
    assert_eq!(borrow.emits, ["CopyBorrowed"]);
}

#[test]
fn a_surface_lists_the_operations_its_actor_can_perform() {
    // The journey view's entry points and the simulator's opening moves.
    let graph = graph();
    let shelf = graph
        .node(&id("lending", NodeKind::Surface, "MemberShelf"))
        .and_then(|node| node.detail.as_surface())
        .expect("MemberShelf");

    assert_eq!(shelf.actor.as_deref(), Some("Reader"));
    let offered: Vec<&str> =
        shelf.provides.iter().map(|operation| operation.trigger.as_str()).collect();
    assert_eq!(
        offered,
        [
            "MemberBorrows",
            "MemberReturns",
            "MemberReportsLoss",
            "MemberReserves",
            "MemberReservesQuietly",
            "MemberWithdrawsReservation"
        ]
    );
    assert_eq!(shelf.guarantees, ["ALoanIsVisibleToItsHolderOnly"]);
}

#[test]
fn a_guarded_operation_keeps_the_condition_that_offers_it() {
    let graph = graph();
    let desk = graph
        .node(&id("catalogue", NodeKind::Surface, "CatalogueDesk"))
        .and_then(|node| node.detail.as_surface())
        .expect("CatalogueDesk");
    let withdraw = desk
        .provides
        .iter()
        .find(|operation| operation.trigger == "LibrarianWithdrawsBook")
        .expect("the guarded operation");
    assert_eq!(withdraw.when.as_deref(), Some("book.status = listed"));
}

#[test]
fn an_actor_resolves_to_the_entity_that_identifies_them() {
    let graph = graph();
    let librarian = graph.node(&id("catalogue", NodeKind::Actor, "Librarian")).expect("Librarian");
    let edge = graph
        .edges_from(&librarian.id)
        .find(|edge| edge.kind == EdgeKind::IdentifiedBy)
        .expect("an identified_by edge");
    assert_eq!(edge.to, id("catalogue", NodeKind::Entity, "Staff"));
}

#[test]
fn an_invariant_is_linked_to_what_it_constrains() {
    let graph = graph();
    let invariant = id("lending", NodeKind::Invariant, "OpenLoansAreWithinTheLimit");
    let constrained: Vec<&str> = graph
        .edges_from(&invariant)
        .filter(|edge| edge.kind == EdgeKind::Constrains)
        .map(|edge| edge.to.name())
        .collect();
    assert_eq!(constrained, ["Member"], "`for m in Members` names the Member entity");
}

#[test]
fn the_analysis_findings_survive_into_the_graph() {
    // The fixtures deliberately contain both a deadlock and a conflict, so the
    // overlay has something real to render.
    let graph = graph();
    let kinds: std::collections::BTreeSet<&str> =
        graph.findings.iter().map(|finding| finding.kind.as_str()).collect();
    assert!(kinds.contains("deadlock"), "findings were {kinds:?}");
    assert!(kinds.contains("conflict"), "findings were {kinds:?}");

    let conflict = graph
        .findings
        .iter()
        .find(|finding| finding.kind == "conflict")
        .expect("a conflict finding");
    assert!(!conflict.rules.is_empty(), "a conflict names the rules that disagree");
}

#[test]
fn diagnostics_are_recorded_once_despite_four_documents_reporting_them() {
    let graph = graph();
    let mut seen: Vec<(&str, &str)> = graph
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.module.as_str(), diagnostic.message.as_str()))
        .collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), total, "a diagnostic was recorded more than once");
    assert!(total > 0, "the fixtures do produce diagnostics worth showing");
}

#[test]
fn obligations_are_attributed_to_the_construct_that_owes_them() {
    let graph = graph();
    let owed: Vec<&str> = graph
        .obligations_for("lending", "BorrowCopy")
        .map(|obligation| obligation.category.as_str())
        .collect();
    assert!(owed.contains(&"rule_success"), "owed: {owed:?}");
    assert!(owed.contains(&"rule_failure"), "owed: {owed:?}");
}

#[test]
fn diagnostics_are_attributed_to_the_constructs_they_are_about() {
    // The badge on a node comes from this. The fixtures warn about `Copy`'s
    // unreachable states and about `Staff`'s unreferenced fields, so both
    // should land on their own entity rather than on the module at large.
    let graph = graph();
    let attributed: Vec<(&str, &str)> = graph
        .diagnostics
        .iter()
        .filter_map(|diagnostic| Some((diagnostic.node.as_deref()?, diagnostic.message.as_str())))
        .collect();

    assert!(!attributed.is_empty(), "no diagnostic found a construct");
    assert!(
        attributed.iter().any(|(node, _)| *node == "catalogue::entity::Copy"),
        "the Copy status warnings should land on Copy: {attributed:?}"
    );
    assert!(
        attributed.iter().any(|(node, _)| *node == "catalogue::entity::Staff"),
        "the Staff field notes should land on Staff: {attributed:?}"
    );
    for (node, message) in &attributed {
        assert!(graph.node(&NodeId(node.to_string())).is_some(), "{node} ({message})");
    }
}

#[test]
fn ingestion_is_deterministic() {
    // Everything downstream assumes it: snapshot tests, diffs between two
    // versions of a spec, and a URL that names a node.
    assert_eq!(graph(), graph());
}

#[test]
fn the_graph_is_sorted_so_its_json_is_stable() {
    let graph = graph();
    let mut sorted = graph.clone();
    sorted.normalise();
    assert_eq!(graph, sorted, "ingestion must leave the graph normalised");
}

#[test]
fn the_program_carries_the_clauses_the_graph_only_describes() {
    // The two halves of an ingestion, and the reason they are separate: the
    // graph holds the text a reader reads, and the program holds the same
    // clauses parsed, which is an order of magnitude more data and only the
    // simulator's business.
    let Ingestion { graph, program } = ingested();

    let borrow = program.rule("lending::rule::BorrowCopy").expect("the program carries BorrowCopy");
    assert!(borrow.when.is_some(), "the trigger it waits for");
    assert_eq!(borrow.requires.len(), 2, "one tree per precondition");
    assert_eq!(borrow.ensures.len(), 3, "one tree per postcondition");

    // The same rule in the graph carries the same clauses as text.
    let described = graph
        .node(&id("lending", NodeKind::Rule, "BorrowCopy"))
        .and_then(|node| node.detail.as_rule())
        .expect("the graph describes BorrowCopy");
    assert_eq!(described.clauses_of("requires").count(), borrow.requires.len());
    assert_eq!(described.clauses_of("ensures").count(), borrow.ensures.len());
}

#[test]
fn every_rule_the_graph_shows_has_clauses_the_simulator_can_read() {
    // A rule present in one half and missing from the other would show in the
    // canvas and then refuse to simulate, with nothing saying why.
    let Ingestion { graph, program } = ingested();
    let missing: Vec<&str> = graph
        .nodes_of(NodeKind::Rule)
        .map(|node| node.id.as_str())
        .filter(|id| program.rule(id).is_none())
        .collect();
    assert!(missing.is_empty(), "rules with no parsed clauses: {missing:?}");
}

#[test]
fn an_invariant_with_an_expression_carries_its_condition() {
    let Ingestion { graph, program } = ingested();
    for node in graph.nodes_of(NodeKind::Invariant) {
        let checkable = matches!(
            &node.detail,
            inspect_model::NodeDetail::Invariant(detail) if detail.is_checkable()
        );
        if checkable {
            assert!(
                program.invariant(node.id.as_str()).is_some(),
                "{} reads as checkable but carries no condition",
                node.id
            );
        }
    }
}
