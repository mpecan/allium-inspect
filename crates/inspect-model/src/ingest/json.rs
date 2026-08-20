//! Small readers for the shapes the CLI's JSON keeps repeating.
//!
//! Ingestion reads four documents produced by a separate project on its own
//! release cycle. Every field it wants is therefore optional in practice, and
//! writing that out longhand at each of the several hundred access sites would
//! bury the mapping in `and_then` chains.
//!
//! The rule these helpers encode: a missing or wrongly-typed field yields an
//! empty value, never a panic and never a default that lies. An absent list is
//! an empty list; an absent string is `None`, not `""`. The distinction matters
//! downstream, where `Some("")` and `None` mean different things about whether
//! the spec said something.

use serde_json::Value;

use crate::span::Span;

/// The string at `key`, or `None` when it is absent or not a string.
#[must_use]
pub fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

/// The string at `key`, or `""`.
///
/// For the places where a name is structurally required and an empty one is
/// already handled as "unnamed" by the caller.
#[must_use]
pub fn string_or_empty(value: &Value, key: &str) -> String {
    string(value, key).unwrap_or_default()
}

/// The array at `key`, or an empty slice.
#[must_use]
pub fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// The strings in the array at `key`, skipping any element that is not one.
///
/// Skipping rather than stringifying: a number where a name belongs is a shape
/// this crate does not understand, and `"7"` would be a name nothing declared.
#[must_use]
pub fn strings(value: &Value, key: &str) -> Vec<String> {
    array(value, key).iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect()
}

/// The `{start, end}` span at `key`, or `None`.
#[must_use]
pub fn span(value: &Value, key: &str) -> Option<Span> {
    span_of(value.get(key)?)
}

/// A `{start, end}` object as a [`Span`].
#[must_use]
pub fn span_of(value: &Value) -> Option<Span> {
    let start = usize::try_from(value.get("start")?.as_u64()?).ok()?;
    let end = usize::try_from(value.get("end")?.as_u64()?).ok()?;
    Some(Span::new(start, end))
}

/// The single key of a one-key object, with its value.
///
/// The parser's AST is a tagged union spelled `{"Ident": {...}}`, so almost
/// every step through it is this question.
#[must_use]
pub fn tagged(value: &Value) -> Option<(&str, &Value)> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.iter().next().map(|(key, inner)| (key.as_str(), inner))
}

/// The `.name.name` of an AST node that carries a named identifier.
///
/// Names in the AST are themselves spanned nodes: `{"name": {"span": …,
/// "name": "Book"}}`. The doubled field is easy to misread as a typo at the
/// call site, so it is spelled out once here.
#[must_use]
pub fn declared_name(value: &Value) -> Option<String> {
    value.get("name").and_then(|name| match name {
        Value::String(text) => Some(text.clone()),
        _ => string(name, "name"),
    })
}

/// The type an expression names, qualified when it crosses a module.
///
/// One implementation, because two diverged: a copy that reached for `name`
/// first read `catalogue/Copy` as plain `Copy`, silently turning a cross-module
/// reference into a dangling local one.
#[must_use]
pub fn type_reference(value: &Value) -> Option<String> {
    match tagged(value)? {
        // Checked before `Ident`, not after: a qualified name carries a `name`
        // field too, so reading that first discards the qualifier.
        ("QualifiedName", inner) => {
            Some(format!("{}/{}", string(inner, "qualifier")?, string(inner, "name")?))
        }
        ("Ident", inner) => string(inner, "name"),
        ("Binding", inner) => type_reference(inner.get("value")?),
        ("Where", inner) => type_reference(inner.get("source")?),
        ("GenericType", inner) => {
            // `Set<Book>` refers to `Book`; the container is not a construct.
            inner
                .get("arguments")
                .and_then(Value::as_array)
                .and_then(|arguments| arguments.first())
                .and_then(type_reference)
                .or_else(|| string(inner, "name"))
        }
        ("TypeOptional", inner) => {
            type_reference(inner.get("inner").or_else(|| inner.get("value"))?)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_type_reference_reads_a_plain_identifier() {
        assert_eq!(type_reference(&json!({"Ident": {"name": "Copy"}})).as_deref(), Some("Copy"));
    }

    #[test]
    fn a_qualified_type_reference_keeps_its_module() {
        // The bug this function exists to prevent: `QualifiedName` also carries
        // a `name`, so matching `Ident` first reads `catalogue/Copy` as `Copy`
        // and points the edge at a local entity that does not exist.
        let value = json!({"QualifiedName": {"qualifier": "catalogue", "name": "Copy"}});
        assert_eq!(type_reference(&value).as_deref(), Some("catalogue/Copy"));
    }

    #[test]
    fn a_generic_type_refers_to_its_argument() {
        let value = json!({"GenericType": {
            "name": "Set",
            "arguments": [{"Ident": {"name": "Book"}}],
        }});
        assert_eq!(type_reference(&value).as_deref(), Some("Book"));
    }

    #[test]
    fn a_generic_type_with_no_arguments_falls_back_to_its_own_name() {
        let value = json!({"GenericType": {"name": "Set", "arguments": []}});
        assert_eq!(type_reference(&value).as_deref(), Some("Set"));
    }

    #[test]
    fn an_optional_type_refers_to_what_it_wraps() {
        let value = json!({"TypeOptional": {"inner": {"Ident": {"name": "Attachment"}}}});
        assert_eq!(type_reference(&value).as_deref(), Some("Attachment"));
    }

    #[test]
    fn a_binding_and_a_filter_both_resolve_to_their_source_type() {
        let binding =
            json!({"Binding": {"name": {"name": "r"}, "value": {"Ident": {"name": "Reader"}}}});
        assert_eq!(type_reference(&binding).as_deref(), Some("Reader"));

        let filtered = json!({"Where": {
            "source": {"Ident": {"name": "Member"}},
            "condition": {"Ident": {"name": "x"}},
        }});
        assert_eq!(type_reference(&filtered).as_deref(), Some("Member"));
    }

    #[test]
    fn an_expression_that_names_no_type_is_none() {
        assert_eq!(type_reference(&json!({"NumberLiteral": {"value": "3"}})), None);
        assert_eq!(type_reference(&json!({"a": 1, "b": 2})), None);
    }

    #[test]
    fn string_reads_a_present_string() {
        assert_eq!(string(&json!({"name": "Book"}), "name").as_deref(), Some("Book"));
    }

    #[test]
    fn string_of_a_missing_or_mistyped_field_is_none() {
        assert_eq!(string(&json!({}), "name"), None);
        assert_eq!(string(&json!({"name": 7}), "name"), None);
        assert_eq!(string(&json!({"name": null}), "name"), None);
    }

    #[test]
    fn an_empty_string_is_distinguishable_from_an_absent_one() {
        // `Some("")` says the spec wrote nothing there; `None` says it wrote
        // nothing at all. Collapsing them would lose a real distinction.
        assert_eq!(string(&json!({"name": ""}), "name").as_deref(), Some(""));
        assert_eq!(string(&json!({}), "name"), None);
    }

    #[test]
    fn string_or_empty_flattens_the_absent_case() {
        assert_eq!(string_or_empty(&json!({"name": "Book"}), "name"), "Book");
        assert_eq!(string_or_empty(&json!({}), "name"), "");
    }

    #[test]
    fn array_of_a_missing_or_null_field_is_empty() {
        // The CLI writes `null`, not `[]`, for an entity with no relationships.
        assert!(array(&json!({}), "fields").is_empty());
        assert!(array(&json!({"fields": null}), "fields").is_empty());
        assert!(array(&json!({"fields": "no"}), "fields").is_empty());
    }

    #[test]
    fn array_reads_its_elements() {
        assert_eq!(array(&json!({"xs": [1, 2]}), "xs").len(), 2);
    }

    #[test]
    fn strings_skips_elements_that_are_not_strings() {
        let value = json!({"states": ["listed", 7, null, "withdrawn"]});
        assert_eq!(strings(&value, "states"), ["listed", "withdrawn"]);
    }

    #[test]
    fn strings_of_a_missing_field_is_empty() {
        assert!(strings(&json!({}), "states").is_empty());
    }

    #[test]
    fn span_reads_start_and_end() {
        let value = json!({"span": {"start": 3, "end": 9}});
        assert_eq!(span(&value, "span"), Some(Span::new(3, 9)));
    }

    #[test]
    fn a_partial_span_is_none() {
        assert_eq!(span(&json!({"span": {"start": 3}}), "span"), None);
        assert_eq!(span(&json!({"span": {}}), "span"), None);
        assert_eq!(span(&json!({}), "span"), None);
    }

    #[test]
    fn a_negative_span_bound_is_none_rather_than_wrapping() {
        // `as_u64` rejects it, and a wrapped `usize` would slice somewhere
        // arbitrary in the file.
        assert_eq!(span(&json!({"span": {"start": -1, "end": 9}}), "span"), None);
    }

    #[test]
    fn tagged_reads_a_single_key_union() {
        let value = json!({"Ident": {"name": "book"}});
        let (tag, inner) = tagged(&value).expect("one key");
        assert_eq!(tag, "Ident");
        assert_eq!(inner["name"], "book");
    }

    #[test]
    fn tagged_refuses_an_object_with_more_than_one_key() {
        // A two-key object is not a tagged union, and picking either key would
        // be picking one at random: serde_json preserves insertion order here,
        // but nothing about the CLI's output guarantees which comes first.
        assert!(tagged(&json!({"a": 1, "b": 2})).is_none());
        assert!(tagged(&json!({})).is_none());
        assert!(tagged(&json!([1])).is_none());
        assert!(tagged(&json!("Ident")).is_none());
    }

    #[test]
    fn declared_name_reads_the_nested_spanned_identifier() {
        let value = json!({"name": {"span": {"start": 0, "end": 4}, "name": "Book"}});
        assert_eq!(declared_name(&value).as_deref(), Some("Book"));
    }

    #[test]
    fn declared_name_also_reads_a_plain_string_name() {
        assert_eq!(declared_name(&json!({"name": "Book"})).as_deref(), Some("Book"));
    }

    #[test]
    fn declared_name_of_something_unnamed_is_none() {
        assert_eq!(declared_name(&json!({})), None);
        assert_eq!(declared_name(&json!({"name": {"span": {"start": 0, "end": 1}}})), None);
    }
}
