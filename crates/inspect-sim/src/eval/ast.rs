//! Small readers over the parser's JSON, and the truth/value bridge.
//!
//! Separated from the evaluator because they are about the *shape* of the
//! parser's output rather than about what an expression means — and because
//! keeping them apart is what lets each be tested for what it is: these against
//! malformed and surprising JSON, the evaluator against worlds.

use inspect_model::Span;
use serde_json::Value as Json;

use crate::{truth::Truth, value::Value};

/// The single key of a one-key object, with its value.
///
/// The parser's AST is a tagged union spelled `{"Ident": {...}}`, so almost
/// every step through it is this question.
#[must_use]
pub fn tagged(node: &Json) -> Option<(&str, &Json)> {
    let object = node.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.iter().next().map(|(tag, inner)| (tag.as_str(), inner))
}

/// The `name` of a node, whether it is a bare string or a spanned identifier.
#[must_use]
pub fn name_of(node: &Json) -> Option<String> {
    match node.get("name")? {
        Json::String(text) => Some(text.clone()),
        nested => nested.get("name")?.as_str().map(ToOwned::to_owned),
    }
}

/// The string at `key`.
#[must_use]
pub fn string_at(node: &Json, key: &str) -> Option<String> {
    node.get(key)?.as_str().map(ToOwned::to_owned)
}

/// A literal's `value`, as text.
///
/// Numbers arrive as strings — `{"value": "20"}`, not `{"value": 20}` — because
/// the parser preserves how they were written, digit separators and all.
#[must_use]
pub fn text_of(node: &Json) -> String {
    string_at(node, "value").unwrap_or_default()
}

/// The span of a node, whether it is tagged or already unwrapped.
#[must_use]
pub fn span_of(node: &Json) -> Option<Span> {
    let source =
        node.get("span").or_else(|| tagged(node).and_then(|(_, inner)| inner.get("span")))?;
    let start = usize::try_from(source.get("start")?.as_u64()?).ok()?;
    let end = usize::try_from(source.get("end")?.as_u64()?).ok()?;
    Some(Span::new(start, end))
}

/// The name of a bare `Ident`, when that is what `node` is.
#[must_use]
pub fn bare_name(node: &Json) -> Option<String> {
    match tagged(node)? {
        ("Ident", inner) => name_of(inner),
        _ => None,
    }
}

/// Whether `node` is the identifier `name`.
#[must_use]
pub fn is_ident_named(node: &Json, name: &str) -> bool {
    bare_name(node).is_some_and(|found| found == name)
}

/// A truth as the value an expression yields.
///
/// Undecided becomes [`Value::Unknown`] rather than `Bool(false)`, which is the
/// whole reason the two types exist separately.
#[must_use]
pub fn truth_value(truth: Truth) -> Value {
    match truth {
        Truth::True => Value::Bool(true),
        Truth::False => Value::Bool(false),
        Truth::Unknown => Value::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_tagged_node_yields_its_tag_and_body() {
        let node = json!({"Ident": {"name": "book"}});
        let (tag, inner) = tagged(&node).expect("one key");
        assert_eq!(tag, "Ident");
        assert_eq!(inner["name"], "book");
    }

    #[test]
    fn an_object_with_two_keys_is_not_a_tagged_node() {
        // Picking either key would be picking one at random.
        assert!(tagged(&json!({"a": 1, "b": 2})).is_none());
        assert!(tagged(&json!({})).is_none());
        assert!(tagged(&json!([1])).is_none());
        assert!(tagged(&json!("Ident")).is_none());
    }

    #[test]
    fn a_name_is_read_through_its_span_wrapper() {
        let spanned = json!({"name": {"span": {"start": 0, "end": 4}, "name": "Book"}});
        assert_eq!(name_of(&spanned).as_deref(), Some("Book"));
        assert_eq!(name_of(&json!({"name": "Book"})).as_deref(), Some("Book"));
    }

    #[test]
    fn something_unnamed_has_no_name() {
        assert_eq!(name_of(&json!({})), None);
        assert_eq!(name_of(&json!({"name": {"span": {}}})), None);
        assert_eq!(name_of(&json!({"name": 7})), None);
    }

    #[test]
    fn a_literal_keeps_the_text_it_was_written_as() {
        // The parser reports `20` as the string "20", digit separators intact,
        // which is what lets `2_000_000_000` survive to the evaluator.
        assert_eq!(text_of(&json!({"value": "2_000_000_000"})), "2_000_000_000");
        assert_eq!(text_of(&json!({})), "");
    }

    #[test]
    fn a_span_is_read_tagged_or_bare() {
        let bare = json!({"span": {"start": 3, "end": 9}});
        assert_eq!(span_of(&bare), Some(Span::new(3, 9)));

        let wrapped = json!({"Ident": {"span": {"start": 1, "end": 2}, "name": "x"}});
        assert_eq!(span_of(&wrapped), Some(Span::new(1, 2)));
    }

    #[test]
    fn a_partial_or_negative_span_is_none() {
        assert_eq!(span_of(&json!({"span": {"start": 3}})), None);
        assert_eq!(span_of(&json!({"span": {"start": -1, "end": 4}})), None);
        assert_eq!(span_of(&json!({})), None);
    }

    #[test]
    fn a_bare_identifier_is_recognised_and_anything_else_is_not() {
        assert_eq!(
            bare_name(&json!({"Ident": {"name": "available"}})).as_deref(),
            Some("available")
        );
        assert_eq!(bare_name(&json!({"NumberLiteral": {"value": "3"}})), None);
        assert!(is_ident_named(&json!({"Ident": {"name": "config"}}), "config"));
        assert!(!is_ident_named(&json!({"Ident": {"name": "other"}}), "config"));
        assert!(!is_ident_named(&json!({"Now": {}}), "config"));
    }

    #[test]
    fn undecided_becomes_unknown_rather_than_false() {
        // The whole reason Truth and Value are separate types.
        assert_eq!(truth_value(Truth::True), Value::Bool(true));
        assert_eq!(truth_value(Truth::False), Value::Bool(false));
        assert_eq!(truth_value(Truth::Unknown), Value::Unknown);
    }
}
