//! Diagnostics and analysis findings, as the UI needs to badge them.
//!
//! Two things the CLI reports are kept apart here because they answer different
//! questions. A [`Diagnostic`] is about the *text*: something at a line and a
//! column is wrong, questionable or merely unused. A [`Finding`] is about the
//! *system*: a state with no path to a terminal, two rules that can fire on the
//! same state and disagree, a value that never flows anywhere.
//!
//! Both are deliberately tolerant of shapes we have not seen. The CLI is a
//! separate project on its own release cycle, and a finding type added upstream
//! must render as an honest "here is what it said" rather than disappear
//! because this crate has no variant for it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Severity {
    /// Something is worth knowing but nothing is wrong.
    Info,
    /// Something is probably wrong; the spec still parses.
    Warning,
    /// The spec does not parse, or contradicts itself.
    Error,
}

impl Severity {
    /// Parse the CLI's spelling, defaulting unknown words to [`Severity::Warning`].
    ///
    /// Unknown severities become warnings rather than being dropped or treated
    /// as errors. Dropping loses a real report; promoting to error would let a
    /// word we simply have not seen fail a whole graph.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "info" | "information" | "hint" | "note" => Severity::Info,
            "error" | "fatal" => Severity::Error,
            _ => Severity::Warning,
        }
    }

    /// The CLI's spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Where in a file a diagnostic points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Location {
    /// The spec file, as the CLI named it.
    pub file: String,
    /// One-based line.
    pub line: usize,
    /// One-based column.
    pub column: usize,
}

/// One structural report about a spec file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// The CLI's dotted code, such as `allium.status.noExit`. Absent for
    /// parse errors, which carry only prose.
    pub code: Option<String>,
    /// Absent when the CLI reported no location.
    pub location: Option<Location>,
    /// The module this was reported against, filled in during ingestion.
    pub module: String,
}

impl Diagnostic {
    /// Read one diagnostic from the CLI's JSON.
    ///
    /// Returns `None` only when there is no message at all — a report that says
    /// nothing cannot be shown, and inventing text for it would be worse than
    /// omitting it.
    #[must_use]
    pub fn from_json(value: &Value, module: &str) -> Option<Self> {
        let message = value.get("message")?.as_str()?.to_owned();
        let severity = value
            .get("severity")
            .and_then(Value::as_str)
            .map_or(Severity::Warning, Severity::parse);
        let code = value.get("code").and_then(Value::as_str).map(ToOwned::to_owned);
        let location = value.get("location").and_then(|location| {
            Some(Location {
                file: location.get("file")?.as_str()?.to_owned(),
                line: usize::try_from(location.get("line")?.as_u64()?).ok()?,
                // The CLI spells it `col`; the UI and every editor say column.
                column: location
                    .get("col")
                    .or_else(|| location.get("column"))
                    .and_then(Value::as_u64)
                    .and_then(|column| usize::try_from(column).ok())
                    .unwrap_or(1),
            })
        });
        Some(Self { severity, message, code, location, module: module.to_owned() })
    }
}

/// One system-level observation from `allium analyse`.
///
/// The `detail` field keeps the finding's whole original JSON. Finding types
/// differ from one another in shape — a deadlock names a state and its outbound
/// edges, a conflict names two rules and the values they disagree on — and new
/// types arrive with CLI releases. Rather than model each one and silently drop
/// the rest, the parts every finding shares are lifted out and the remainder is
/// carried through for the UI to render generically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Finding {
    /// The CLI's `type`, such as `deadlock` or `conflict`.
    pub kind: String,
    /// One sentence describing what was found.
    pub summary: String,
    /// Entities the finding is about; the UI badges their nodes.
    pub entities: Vec<String>,
    /// Rules the finding names, where it names any.
    pub rules: Vec<String>,
    /// The module this was reported against.
    pub module: String,
    /// The finding's original JSON, for details this struct does not model.
    #[ts(type = "unknown")]
    pub detail: Value,
}

impl Finding {
    /// Read one finding from the CLI's JSON.
    ///
    /// Returns `None` when there is neither a type nor a summary, which is the
    /// only shape that could not be rendered at all.
    #[must_use]
    pub fn from_json(value: &Value, module: &str) -> Option<Self> {
        let kind = value.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
        let summary = value.get("summary").and_then(Value::as_str).unwrap_or_default().to_owned();
        if kind.is_empty() && summary.is_empty() {
            return None;
        }

        let entities = value
            .get("affected_entities")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect())
            .unwrap_or_default();

        // Conflicts name their two rules in `rule_a`/`rule_b`; other types use
        // a `rules` array. Both are read so the UI can badge rule nodes without
        // knowing which finding type it is looking at.
        let mut rules: Vec<String> = ["rule_a", "rule_b"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect();
        if let Some(listed) = value.get("rules").and_then(Value::as_array) {
            rules.extend(listed.iter().filter_map(Value::as_str).map(ToOwned::to_owned));
        }
        rules.sort();
        rules.dedup();

        Some(Self {
            kind: if kind.is_empty() { "finding".to_owned() } else { kind },
            summary,
            entities,
            rules,
            module: module.to_owned(),
            detail: value.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn severity_parses_the_spellings_the_cli_uses() {
        assert_eq!(Severity::parse("info"), Severity::Info);
        assert_eq!(Severity::parse("warning"), Severity::Warning);
        assert_eq!(Severity::parse("error"), Severity::Error);
    }

    #[test]
    fn severity_parsing_ignores_case_and_padding() {
        assert_eq!(Severity::parse("  ERROR "), Severity::Error);
        assert_eq!(Severity::parse("Info"), Severity::Info);
    }

    #[test]
    fn an_unknown_severity_becomes_a_warning() {
        // Not dropped, which would lose a real report, and not promoted to
        // error, which would let an unfamiliar word fail a whole graph.
        assert_eq!(Severity::parse("catastrophe"), Severity::Warning);
        assert_eq!(Severity::parse(""), Severity::Warning);
    }

    #[test]
    fn severity_orders_by_how_much_it_matters() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }

    #[test]
    fn severity_round_trips_through_its_string() {
        for severity in [Severity::Info, Severity::Warning, Severity::Error] {
            assert_eq!(Severity::parse(severity.as_str()), severity);
        }
    }

    #[test]
    fn a_diagnostic_reads_its_location() {
        let value = json!({
            "code": "allium.status.noExit",
            "location": {"col": 13, "file": "catalogue.allium", "line": 39},
            "message": "Status 'available' has no observed transition.",
            "severity": "warning",
        });
        let diagnostic = Diagnostic::from_json(&value, "catalogue").expect("has a message");
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(diagnostic.code.as_deref(), Some("allium.status.noExit"));
        assert_eq!(diagnostic.module, "catalogue");
        let location = diagnostic.location.expect("has a location");
        assert_eq!(location.line, 39);
        assert_eq!(location.column, 13, "the CLI spells it `col`");
        assert_eq!(location.file, "catalogue.allium");
    }

    #[test]
    fn a_diagnostic_with_a_null_code_keeps_its_message() {
        // Parse errors carry prose and no code. Requiring a code would drop
        // exactly the diagnostics that matter most.
        let value = json!({
            "code": null,
            "location": {"col": 5, "file": "a.allium", "line": 1},
            "message": "expected '{', found 'external'",
            "severity": "error",
        });
        let diagnostic = Diagnostic::from_json(&value, "a").expect("has a message");
        assert_eq!(diagnostic.code, None);
        assert_eq!(diagnostic.severity, Severity::Error);
    }

    #[test]
    fn a_diagnostic_with_no_location_is_still_kept() {
        let value = json!({"message": "something is wrong", "severity": "error"});
        let diagnostic = Diagnostic::from_json(&value, "a").expect("has a message");
        assert_eq!(diagnostic.location, None);
        assert_eq!(diagnostic.message, "something is wrong");
    }

    #[test]
    fn a_diagnostic_with_a_partial_location_drops_only_the_location() {
        // A location missing its line cannot be pointed at, but the message
        // still says something worth showing.
        let value = json!({"message": "m", "severity": "info", "location": {"file": "a.allium"}});
        let diagnostic = Diagnostic::from_json(&value, "a").expect("has a message");
        assert_eq!(diagnostic.location, None);
        assert_eq!(diagnostic.severity, Severity::Info);
    }

    #[test]
    fn a_location_spelled_column_is_read_too() {
        let value = json!({
            "message": "m",
            "location": {"column": 7, "file": "a.allium", "line": 2},
        });
        let location = Diagnostic::from_json(&value, "a").expect("m").location.expect("located");
        assert_eq!(location.column, 7);
    }

    #[test]
    fn a_location_with_no_column_defaults_to_the_first() {
        let value = json!({"message": "m", "location": {"file": "a.allium", "line": 2}});
        let location = Diagnostic::from_json(&value, "a").expect("m").location.expect("located");
        assert_eq!(location.column, 1);
    }

    #[test]
    fn a_diagnostic_with_no_message_is_dropped() {
        assert!(Diagnostic::from_json(&json!({"severity": "error"}), "a").is_none());
        assert!(Diagnostic::from_json(&json!({"message": 12}), "a").is_none());
    }

    #[test]
    fn a_diagnostic_with_no_severity_is_a_warning() {
        let diagnostic = Diagnostic::from_json(&json!({"message": "m"}), "a").expect("m");
        assert_eq!(diagnostic.severity, Severity::Warning);
    }

    #[test]
    fn a_deadlock_finding_keeps_its_entities_and_whole_detail() {
        let value = json!({
            "affected_entities": ["Copy"],
            "outbound_edges": [{"from": "available", "reason": "no witnessing rule", "to": "lost"}],
            "state": "available",
            "summary": "Entity 'Copy' can reach state 'available' but has no path to a terminal",
            "type": "deadlock",
        });
        let finding = Finding::from_json(&value, "catalogue").expect("has a type");
        assert_eq!(finding.kind, "deadlock");
        assert_eq!(finding.entities, ["Copy"]);
        assert!(finding.rules.is_empty());
        assert_eq!(finding.module, "catalogue");
        // The shape-specific part survives for the UI to render.
        assert_eq!(finding.detail["outbound_edges"][0]["reason"], "no witnessing rule");
    }

    #[test]
    fn a_conflict_finding_collects_both_named_rules() {
        let value = json!({
            "affected_entities": ["Loan"],
            "rule_a": "ReportCopyLost",
            "rule_b": "LoanFallsOverdue",
            "summary": "two rules disagree",
            "type": "conflict",
        });
        let finding = Finding::from_json(&value, "lending").expect("has a type");
        assert_eq!(finding.rules, ["LoanFallsOverdue", "ReportCopyLost"], "sorted for determinism");
    }

    #[test]
    fn a_finding_listing_rules_in_an_array_is_read_too() {
        let value = json!({"type": "cycle", "summary": "s", "rules": ["B", "A", "B"]});
        let finding = Finding::from_json(&value, "m").expect("has a type");
        assert_eq!(finding.rules, ["A", "B"], "sorted and de-duplicated");
    }

    #[test]
    fn a_finding_type_this_crate_has_never_seen_still_survives() {
        // The CLI is a separate project on its own release cycle. A finding
        // type added upstream must render as "here is what it said", not vanish
        // because there is no variant for it.
        let value = json!({
            "type": "some_future_analysis",
            "summary": "a thing was noticed",
            "novel_field": {"nested": [1, 2, 3]},
        });
        let finding = Finding::from_json(&value, "m").expect("has a type");
        assert_eq!(finding.kind, "some_future_analysis");
        assert_eq!(finding.summary, "a thing was noticed");
        assert_eq!(finding.detail["novel_field"]["nested"][2], 3);
    }

    #[test]
    fn a_finding_with_a_summary_but_no_type_is_kept_under_a_generic_kind() {
        let finding = Finding::from_json(&json!({"summary": "something"}), "m").expect("summary");
        assert_eq!(finding.kind, "finding");
        assert_eq!(finding.summary, "something");
    }

    #[test]
    fn a_finding_with_neither_type_nor_summary_is_dropped() {
        assert!(Finding::from_json(&json!({"affected_entities": ["X"]}), "m").is_none());
        assert!(Finding::from_json(&json!({}), "m").is_none());
    }

    #[test]
    fn non_string_entities_are_skipped_rather_than_stringified() {
        let value = json!({"type": "t", "summary": "s", "affected_entities": ["Copy", 7, null]});
        let finding = Finding::from_json(&value, "m").expect("has a type");
        assert_eq!(finding.entities, ["Copy"]);
    }
}
