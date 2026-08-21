//! Handing allium's own types to passes that read JSON.
//!
//! `allium_parser` returns a typed tree. The passes below read
//! `serde_json::Value`, and will keep doing so while the simulator evaluates
//! expressions straight off `Value` — a pass rewritten against the typed tree
//! would have to serialise every clause back on its way out.
//!
//! So this is the one place the two meet, and it does exactly two things:
//! serialise, and give each diagnostic a line to point at. The second is not
//! cosmetic. A parser reports a byte span, which is the honest thing for it to
//! carry, and the browser shows a diagnostic against a line — a diagnostic with
//! no line is one nothing can point at.

use serde_json::{Map, Value};

use crate::span::LineIndex;

/// `value` as the document the passes read.
///
/// # Errors
///
/// Returns the serialisation error, which can only happen if allium's own AST
/// stops being serialisable — in which case `allium parse` would print nothing
/// either, and failing loudly is the only useful answer.
pub fn document<T: serde::Serialize>(
    value: &T,
    source: &str,
    file: &str,
) -> Result<Value, serde_json::Error> {
    let mut document = serde_json::to_value(value)?;
    locate(&mut document, source, file);
    Ok(document)
}

/// Give every diagnostic in `document` the `file`/`line`/`col` the CLI reports.
///
/// All three, not just the line. A location missing its file is discarded
/// whole by [`crate::Diagnostic::from_json`], which then leaves the diagnostic
/// unattributable — it still appears in the report and silently stops badging
/// the construct it is about.
fn locate(document: &mut Value, source: &str, file: &str) {
    let index = LineIndex::new(source);
    let Some(diagnostics) = document.get_mut("diagnostics").and_then(Value::as_array_mut) else {
        return;
    };
    for diagnostic in diagnostics {
        let Some(start) = diagnostic
            .get("span")
            .and_then(|span| span.get("start"))
            .and_then(Value::as_u64)
            .and_then(|start| usize::try_from(start).ok())
        else {
            continue;
        };
        let at = index.position(source, start);
        let Some(object) = diagnostic.as_object_mut() else { continue };
        let mut location = Map::new();
        location.insert("file".to_owned(), Value::from(file));
        location.insert("line".to_owned(), Value::from(at.line));
        location.insert("col".to_owned(), Value::from(at.column));
        object.insert("location".to_owned(), Value::Object(location));
    }
}
