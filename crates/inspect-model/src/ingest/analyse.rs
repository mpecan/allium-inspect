//! Reading diagnostics and findings out of whichever document carries them.
//!
//! All four commands print a `diagnostics` array and they largely agree, so
//! reading every one of them would show the same warning four times. They do not
//! agree *entirely* though — a document that failed to parse reports the parse
//! error and the others report nothing — so reading only one risks showing
//! nothing at all.
//!
//! The resolution is to read them all and de-duplicate on what a reader would
//! call the same report: same severity, same message, same place. `findings`
//! come only from `analyse` and are not de-duplicated against anything.

use serde_json::Value;

use crate::{
    diagnostic::{Diagnostic, Finding},
    graph::SpecGraph,
    ingest::json,
    span::{LineIndex, Span},
};

/// Add the diagnostics `document` carries for `module` to `graph`, skipping any
/// already reported.
pub fn ingest_diagnostics(document: &Value, module: &str, graph: &mut SpecGraph) {
    for value in json::array(document, "diagnostics") {
        let Some(diagnostic) = Diagnostic::from_json(value, module) else { continue };
        if graph.diagnostics.iter().any(|existing| same_report(existing, &diagnostic)) {
            continue;
        }
        graph.diagnostics.push(diagnostic);
    }
}

/// Add the findings `document` carries for `module` to `graph`.
pub fn ingest_findings(document: &Value, module: &str, graph: &mut SpecGraph) {
    for value in json::array(document, "findings") {
        if let Some(finding) = Finding::from_json(value, module) {
            graph.findings.push(finding);
        }
    }
}

/// Attach each of `module`'s diagnostics to the construct that encloses it.
///
/// A diagnostic names a line; a node owns a byte range. Joining the two needs
/// the spec text, which only this side of the wire has — so the attribution is
/// done here and the browser reads the answer.
///
/// The *innermost* enclosing declaration wins. A warning about a field inside
/// an entity is about the field, and badging the module's config block because
/// its span happens to be longer would point the reader at the wrong thing.
pub fn attribute(graph: &mut SpecGraph, module: &str, source: &str) {
    let index = LineIndex::new(source);

    let candidates: Vec<(String, Span)> = graph
        .nodes
        .iter()
        .filter(|node| node.module == module)
        .filter_map(|node| Some((node.id.to_string(), node.span?)))
        .collect();

    for diagnostic in &mut graph.diagnostics {
        if diagnostic.module != module || diagnostic.node.is_some() {
            continue;
        }
        let Some(location) = &diagnostic.location else { continue };
        let Some(line) = index.line_span(location.line) else { continue };

        // The line, not a point in it: a diagnostic's column can land past the
        // construct it is about when the parser recovered mid-token, and the
        // line it is on is the reliable part.
        diagnostic.node = candidates
            .iter()
            .filter(|(_, span)| span.start < line.end && span.end > line.start)
            .min_by_key(|(_, span)| span.len())
            .map(|(id, _)| id.clone());
    }
}

/// Whether two diagnostics are the same report seen twice.
///
/// The code is deliberately not compared. `check` and `analyse` have been seen
/// to report the same problem at the same place with one of them omitting the
/// code, and a reader looking at two identical lines does not care that one of
/// them was better labelled.
fn same_report(left: &Diagnostic, right: &Diagnostic) -> bool {
    left.module == right.module
        && left.severity == right.severity
        && left.message == right.message
        && left.location == right.location
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::diagnostic::Severity;

    fn warning(message: &str, line: u64) -> Value {
        json!({
            "severity": "warning",
            "message": message,
            "code": "allium.status.noExit",
            "location": {"file": "catalogue.allium", "line": line, "col": 13},
        })
    }

    #[test]
    fn a_diagnostic_is_recorded_against_its_module() {
        let mut graph = SpecGraph::new("test");
        ingest_diagnostics(&json!({"diagnostics": [warning("m", 39)]}), "catalogue", &mut graph);
        assert_eq!(graph.diagnostics.len(), 1);
        assert_eq!(graph.diagnostics[0].module, "catalogue");
        assert_eq!(graph.diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn the_same_report_from_two_documents_is_recorded_once() {
        // All four commands print diagnostics and they largely agree. Reading
        // every one without this would show each warning up to four times.
        let mut graph = SpecGraph::new("test");
        let document = json!({"diagnostics": [warning("same message", 39)]});
        ingest_diagnostics(&document, "catalogue", &mut graph);
        ingest_diagnostics(&document, "catalogue", &mut graph);
        assert_eq!(graph.diagnostics.len(), 1);
    }

    #[test]
    fn the_same_message_at_a_different_line_is_two_reports() {
        let mut graph = SpecGraph::new("test");
        let document =
            json!({"diagnostics": [warning("same message", 39), warning("same message", 57)]});
        ingest_diagnostics(&document, "catalogue", &mut graph);
        assert_eq!(graph.diagnostics.len(), 2);
    }

    #[test]
    fn the_same_message_in_a_different_module_is_two_reports() {
        let mut graph = SpecGraph::new("test");
        let document = json!({"diagnostics": [warning("same message", 39)]});
        ingest_diagnostics(&document, "catalogue", &mut graph);
        ingest_diagnostics(&document, "lending", &mut graph);
        assert_eq!(graph.diagnostics.len(), 2);
    }

    #[test]
    fn the_same_report_labelled_with_and_without_a_code_is_recorded_once() {
        // Two identical lines differing only in how well one was labelled is
        // still one problem to a reader.
        let mut graph = SpecGraph::new("test");
        let coded = json!({"diagnostics": [warning("m", 39)]});
        let uncoded = json!({"diagnostics": [{
            "severity": "warning",
            "message": "m",
            "code": null,
            "location": {"file": "catalogue.allium", "line": 39, "col": 13},
        }]});
        ingest_diagnostics(&coded, "catalogue", &mut graph);
        ingest_diagnostics(&uncoded, "catalogue", &mut graph);
        assert_eq!(graph.diagnostics.len(), 1);
        assert_eq!(graph.diagnostics[0].code.as_deref(), Some("allium.status.noExit"));
    }

    #[test]
    fn a_parse_error_only_one_document_reports_still_lands() {
        // The reason every document is read rather than just one: when a spec
        // does not parse, the other commands have nothing to say about it.
        let mut graph = SpecGraph::new("test");
        ingest_diagnostics(&json!({"diagnostics": []}), "broken", &mut graph);
        ingest_diagnostics(
            &json!({"diagnostics": [{"severity": "error", "message": "expected '{'"}]}),
            "broken",
            &mut graph,
        );
        assert_eq!(graph.diagnostics.len(), 1);
        assert_eq!(graph.diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn a_findings_array_is_read_whole() {
        let mut graph = SpecGraph::new("test");
        let document = json!({"findings": [
            {"type": "deadlock", "summary": "no path to a terminal", "affected_entities": ["Copy"]},
            {"type": "conflict", "summary": "two rules disagree", "rule_a": "A", "rule_b": "B"},
        ]});
        ingest_findings(&document, "catalogue", &mut graph);
        assert_eq!(graph.findings.len(), 2);
        assert_eq!(graph.findings[0].kind, "deadlock");
        assert_eq!(graph.findings[1].rules, ["A", "B"]);
    }

    #[test]
    fn findings_are_not_de_duplicated_against_diagnostics() {
        // They answer different questions — one is about the text, the other
        // about the system — and both belong in their own panel.
        let mut graph = SpecGraph::new("test");
        let document = json!({
            "diagnostics": [warning("Status 'available' has no observed transition.", 39)],
            "findings": [{"type": "deadlock", "summary": "no path to a terminal"}],
        });
        ingest_diagnostics(&document, "catalogue", &mut graph);
        ingest_findings(&document, "catalogue", &mut graph);
        assert_eq!(graph.diagnostics.len(), 1);
        assert_eq!(graph.findings.len(), 1);
    }

    // --- attribution -----------------------------------------------------

    const SOURCE: &str =
        "entity Book {\n    title: String\n}\n\nrule AddBook {\n    when: X()\n}\n";

    /// A graph holding the two constructs `SOURCE` declares, with their spans.
    fn located() -> SpecGraph {
        use crate::graph::{Node, NodeKind};
        let mut graph = SpecGraph::new("test");
        let book = SOURCE.find("entity Book {").expect("present");
        let book_end = SOURCE.find("}\n\nrule").expect("present") + 1;
        let rule = SOURCE.find("rule AddBook {").expect("present");
        graph
            .nodes
            .push(Node::new("m", NodeKind::Entity, "Book").at(Some(Span::new(book, book_end))));
        graph.nodes.push(
            Node::new("m", NodeKind::Rule, "AddBook").at(Some(Span::new(rule, SOURCE.len()))),
        );
        graph
    }

    fn attributed(line: usize) -> Option<String> {
        let mut graph = located();
        ingest_diagnostics(
            &json!({"diagnostics": [{
                "severity": "warning",
                "message": "m",
                "location": {"file": "m.allium", "line": line, "col": 5},
            }]}),
            "m",
            &mut graph,
        );
        attribute(&mut graph, "m", SOURCE);
        graph.diagnostics[0].node.clone()
    }

    #[test]
    fn a_diagnostic_is_attributed_to_the_construct_that_encloses_it() {
        assert_eq!(attributed(2).as_deref(), Some("m::entity::Book"));
        assert_eq!(attributed(6).as_deref(), Some("m::rule::AddBook"));
    }

    #[test]
    fn a_diagnostic_on_a_declaration_line_belongs_to_that_declaration() {
        assert_eq!(attributed(1).as_deref(), Some("m::entity::Book"));
        assert_eq!(attributed(5).as_deref(), Some("m::rule::AddBook"));
    }

    #[test]
    fn a_declaration_ending_exactly_where_a_line_begins_does_not_claim_it() {
        // Spans are half-open, so a node ending at the first byte of a line
        // stops before that line. Treating the boundary as overlap would let
        // every declaration claim the line after it — which, for a file of
        // back-to-back declarations, is every diagnostic in the file.
        use crate::graph::{Node, NodeKind};
        let source = "entity A {}\nentity B {}\n";
        let second = source.find("entity B").expect("present");
        let mut graph = SpecGraph::new("test");
        graph.nodes.push(Node::new("m", NodeKind::Entity, "A").at(Some(Span::new(0, second))));

        ingest_diagnostics(
            &json!({"diagnostics": [{
                "severity": "warning", "message": "m",
                "location": {"file": "m.allium", "line": 2, "col": 1},
            }]}),
            "m",
            &mut graph,
        );
        attribute(&mut graph, "m", source);
        assert_eq!(graph.diagnostics[0].node, None, "A ends before line 2 begins");
    }

    #[test]
    fn a_diagnostic_between_declarations_is_attributed_to_neither() {
        // Line 4 is the blank line. Reaching for the nearest node would badge a
        // construct the reader would then look at and find nothing wrong with.
        assert_eq!(attributed(4), None);
    }

    #[test]
    fn the_innermost_declaration_wins() {
        // A module-wide construct whose span happens to cover everything must
        // not claim a warning about one field inside it.
        use crate::graph::{Node, NodeKind};
        let mut graph = located();
        graph
            .nodes
            .push(Node::new("m", NodeKind::Config, "config").at(Some(Span::new(0, SOURCE.len()))));
        ingest_diagnostics(
            &json!({"diagnostics": [{
                "severity": "warning", "message": "m",
                "location": {"file": "m.allium", "line": 2, "col": 5},
            }]}),
            "m",
            &mut graph,
        );
        attribute(&mut graph, "m", SOURCE);
        assert_eq!(graph.diagnostics[0].node.as_deref(), Some("m::entity::Book"));
    }

    #[test]
    fn a_diagnostic_with_no_location_is_left_unattributed() {
        let mut graph = located();
        ingest_diagnostics(
            &json!({"diagnostics": [{"severity": "error", "message": "m"}]}),
            "m",
            &mut graph,
        );
        attribute(&mut graph, "m", SOURCE);
        assert_eq!(graph.diagnostics[0].node, None);
    }

    #[test]
    fn attribution_does_not_reach_into_another_module() {
        let mut graph = located();
        ingest_diagnostics(
            &json!({"diagnostics": [{
                "severity": "warning", "message": "m",
                "location": {"file": "other.allium", "line": 2, "col": 1},
            }]}),
            "other",
            &mut graph,
        );
        attribute(&mut graph, "m", SOURCE);
        assert_eq!(graph.diagnostics[0].node, None);
    }

    #[test]
    fn a_diagnostic_past_the_end_of_the_file_is_left_unattributed() {
        // A stale line number from a file edited since the run.
        assert_eq!(attributed(900), None);
    }

    #[test]
    fn attribution_is_idempotent() {
        // The server re-links on every file change; a second pass must not
        // move a badge that the first pass placed correctly.
        let mut graph = located();
        ingest_diagnostics(
            &json!({"diagnostics": [{
                "severity": "warning", "message": "m",
                "location": {"file": "m.allium", "line": 2, "col": 5},
            }]}),
            "m",
            &mut graph,
        );
        attribute(&mut graph, "m", SOURCE);
        let first = graph.diagnostics[0].node.clone();
        attribute(&mut graph, "m", SOURCE);
        assert_eq!(graph.diagnostics[0].node, first);
    }

    #[test]
    fn a_document_with_neither_array_records_nothing() {
        let mut graph = SpecGraph::new("test");
        ingest_diagnostics(&json!({}), "m", &mut graph);
        ingest_findings(&json!({}), "m", &mut graph);
        assert!(graph.diagnostics.is_empty());
        assert!(graph.findings.is_empty());
    }

    #[test]
    fn an_unreadable_entry_is_skipped_without_disturbing_its_siblings() {
        let mut graph = SpecGraph::new("test");
        let document = json!({
            "diagnostics": [{"severity": "error"}, warning("real", 1)],
            "findings": [{}, {"type": "deadlock", "summary": "real"}],
        });
        ingest_diagnostics(&document, "m", &mut graph);
        ingest_findings(&document, "m", &mut graph);
        assert_eq!(graph.diagnostics.len(), 1);
        assert_eq!(graph.findings.len(), 1);
    }
}
