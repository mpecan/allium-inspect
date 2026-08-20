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
