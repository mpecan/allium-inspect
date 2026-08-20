//! What an expression evaluates to.
//!
//! Deliberately close to what an Allium spec talks about rather than to what a
//! machine offers. A `Duration` is not an integer here, a `Timestamp` is not a
//! `Duration`, and an enum member is not a string — because the spec draws those
//! distinctions and a simulator that flattens them would happily compare a state
//! name to a field name and report `true`.
//!
//! [`Value::Unknown`] is the same idea as `Truth::Unknown` one level down: a
//! value the simulator has no way to determine. It is not null. `null` is
//! something the spec can assert about — `attachment_size = null` is a real
//! precondition with a real answer — whereas `Unknown` means the simulator does
//! not know what is there, and every comparison involving it is undecided.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::truth::Truth;

/// The identity of one entity instance in a world.
///
/// Human-readable on purpose: `Message#1` is what appears in the trace, in the
/// world editor and in a shared link, and an opaque id would make all three
/// harder to read for no gain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct EntityId(pub String);

impl EntityId {
    /// The `n`th instance of `entity`.
    #[must_use]
    pub fn new(entity: &str, ordinal: u64) -> Self {
        Self(format!("{entity}#{ordinal}"))
    }

    /// The entity type this instance is of.
    #[must_use]
    pub fn entity(&self) -> &str {
        self.0.split_once('#').map_or(self.0.as_str(), |(entity, _)| entity)
    }

    /// The id as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A value in a simulated world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Value {
    /// Absent, and known to be absent. Distinct from [`Value::Unknown`].
    Null,
    Bool(bool),
    // `number`, not the `bigint` ts-rs would infer from `i64`. What actually
    // crosses the wire is a JSON number, which JavaScript parses as a double —
    // declaring it `bigint` would describe a value nothing ever sends and make
    // every arithmetic use of it a type error. Milliseconds stay exact well past
    // any timescale a specification talks about.
    Int(#[ts(type = "number")] i64),
    /// A decimal. Compared with a tolerance; see [`Value::compare`].
    Float(f64),
    Str(String),
    /// A member of an enumeration, or a state of a status field.
    Enum(String),
    /// Milliseconds. A duration is not an integer: `21.days` and `21` are
    /// different things and comparing them is a spec error worth surfacing.
    Duration(#[ts(type = "number")] i64),
    /// Milliseconds since an arbitrary origin. The simulator's clock has no
    /// calendar — `now` is a number the user advances, and nothing here needs
    /// to know what year it is.
    Timestamp(#[ts(type = "number")] i64),
    /// A reference to another instance.
    Ref(EntityId),
    /// An ordered collection. Ordered so a trace is reproducible; the language's
    /// sets are unordered and nothing here depends on the order being meaningful.
    Set(Vec<Value>),
    /// The simulator has no way to determine this.
    Unknown,
}

impl Value {
    /// A value the simulator could not determine.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Value::Unknown)
    }

    /// The value as a truth, for a bare expression used as a condition.
    ///
    /// Only a boolean is a condition. A non-empty string is not "truthy" here —
    /// Allium has no such coercion, and inventing one would let
    /// `requires: body` pass for a reason the language does not have.
    #[must_use]
    pub fn truth(&self) -> Truth {
        match self {
            Value::Bool(value) => Truth::from_bool(*value),
            Value::Unknown => Truth::Unknown,
            _ => Truth::Unknown,
        }
    }

    /// This value's kind with its article: `an integer`, `a duration`.
    ///
    /// The messages read as sentences to a person, and "a unknown cannot be
    /// ordered against a integer" is the sort of thing that makes a reader
    /// distrust everything else on the panel.
    #[must_use]
    pub fn described(&self) -> String {
        let kind = self.kind();
        let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
        format!("{article} {kind}")
    }

    /// The name of this value's kind, for messages.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Int(_) => "integer",
            Value::Float(_) => "decimal",
            Value::Str(_) => "string",
            Value::Enum(_) => "state",
            Value::Duration(_) => "duration",
            Value::Timestamp(_) => "timestamp",
            Value::Ref(_) => "reference",
            Value::Set(_) => "collection",
            Value::Unknown => "unknown",
        }
    }

    /// Whether two values are equal, in three-valued logic.
    ///
    /// Anything compared with [`Value::Unknown`] is undecided — including
    /// `Unknown = Unknown`, because two things the simulator cannot see are not
    /// thereby the same thing.
    ///
    /// Comparing values of different kinds is `false`, not undecided: the spec
    /// asked a question with a definite answer, and the answer is no. The one
    /// exception is a number against a number of the other numeric kind.
    #[must_use]
    pub fn equals(&self, other: &Value) -> Truth {
        match (self, other) {
            (Value::Unknown, _) | (_, Value::Unknown) => Truth::Unknown,
            (Value::Int(left), Value::Float(right)) | (Value::Float(right), Value::Int(left)) => {
                Truth::from_bool(close(*left as f64, *right))
            }
            (Value::Float(left), Value::Float(right)) => Truth::from_bool(close(*left, *right)),
            (Value::Set(left), Value::Set(right)) => {
                if left.len() != right.len() {
                    return Truth::False;
                }
                Truth::all(left.iter().zip(right).map(|(a, b)| a.equals(b)))
            }
            _ => Truth::from_bool(self == other),
        }
    }

    /// Order two values, when they are of comparable kinds.
    ///
    /// `None` when they are not — comparing a string to a duration is a question
    /// with no answer rather than a question answered no, so the caller reports
    /// it as undecided.
    #[must_use]
    pub fn compare(&self, other: &Value) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Int(left), Value::Int(right)) => Some(left.cmp(right)),
            (Value::Duration(left), Value::Duration(right))
            | (Value::Timestamp(left), Value::Timestamp(right)) => Some(left.cmp(right)),
            (Value::Float(left), Value::Float(right)) => {
                if close(*left, *right) {
                    Some(Ordering::Equal)
                } else {
                    left.partial_cmp(right)
                }
            }
            (Value::Int(left), Value::Float(right)) => {
                let left = *left as f64;
                if close(left, *right) { Some(Ordering::Equal) } else { left.partial_cmp(right) }
            }
            (Value::Float(left), Value::Int(right)) => {
                let right = *right as f64;
                if close(*left, right) { Some(Ordering::Equal) } else { left.partial_cmp(&right) }
            }
            (Value::Str(left), Value::Str(right)) => Some(left.cmp(right)),
            _ => None,
        }
    }

    /// How many elements a collection has.
    #[must_use]
    pub fn count(&self) -> Option<usize> {
        match self {
            Value::Set(items) => Some(items.len()),
            // A null relationship is an empty collection, not an error: an
            // entity with no receipts has `receipts.count = 0`.
            Value::Null => Some(0),
            _ => None,
        }
    }
}

/// Whether two decimals are equal to within a tolerance.
///
/// Spec values are written by hand — `0.1`, `99.9`, `1.5` — and exact float
/// equality would report `0.1 + 0.2 = 0.3` as false. The spec means the two to
/// be the same number, and a simulator that disagreed would be technically
/// correct and useless.
fn close(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= f64::EPSILON * 64.0 * scale
}

/// One entity instance: its type, and what its fields hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Instance {
    pub id: EntityId,
    /// The entity type, unqualified: `Message`.
    pub entity: String,
    /// The module that declares it.
    pub module: String,
    /// Field values, ordered so a world serialises identically twice.
    pub fields: BTreeMap<String, Value>,
}

impl Instance {
    /// An instance of `entity` from `module`, with no fields set.
    #[must_use]
    pub fn new(id: EntityId, entity: impl Into<String>, module: impl Into<String>) -> Self {
        Self { id, entity: entity.into(), module: module.into(), fields: BTreeMap::new() }
    }

    /// The value of `field`.
    ///
    /// A field the instance does not carry is [`Value::Unknown`], not null. The
    /// spec may well require it to be set; the simulator simply has not been
    /// told what it is, and saying "null" would answer a question about the
    /// world with a fact about the simulator.
    #[must_use]
    pub fn field(&self, field: &str) -> Value {
        self.fields.get(field).cloned().unwrap_or(Value::Unknown)
    }

    /// Set `field` to `value`.
    pub fn set(&mut self, field: impl Into<String>, value: Value) {
        self.fields.insert(field.into(), value);
    }

    /// The same instance with `field` set.
    #[must_use]
    pub fn with(mut self, field: impl Into<String>, value: Value) -> Self {
        self.set(field, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entity_id_names_its_type_and_ordinal() {
        let id = EntityId::new("Message", 1);
        assert_eq!(id.as_str(), "Message#1");
        assert_eq!(id.entity(), "Message");
        assert_eq!(id.to_string(), "Message#1");
    }

    #[test]
    fn an_entity_id_with_no_ordinal_is_all_type() {
        assert_eq!(EntityId("Seeded".to_owned()).entity(), "Seeded");
    }

    #[test]
    fn equality_is_undecided_whenever_either_side_is_unknown() {
        assert_eq!(Value::Unknown.equals(&Value::Int(1)), Truth::Unknown);
        assert_eq!(Value::Int(1).equals(&Value::Unknown), Truth::Unknown);
        // Two things the simulator cannot see are not thereby the same thing.
        assert_eq!(Value::Unknown.equals(&Value::Unknown), Truth::Unknown);
    }

    #[test]
    fn null_is_a_value_the_spec_can_ask_about() {
        // `attachment_size = null` is a real precondition with a real answer,
        // which is exactly what separates null from unknown.
        assert_eq!(Value::Null.equals(&Value::Null), Truth::True);
        assert_eq!(Value::Null.equals(&Value::Int(0)), Truth::False);
        assert_eq!(Value::Null.equals(&Value::Unknown), Truth::Unknown);
    }

    #[test]
    fn values_of_different_kinds_are_definitely_not_equal() {
        // A question with a definite answer, and the answer is no.
        assert_eq!(
            Value::Str("held".to_owned()).equals(&Value::Enum("held".to_owned())),
            Truth::False
        );
        assert_eq!(Value::Int(1).equals(&Value::Bool(true)), Truth::False);
        assert_eq!(Value::Duration(1).equals(&Value::Int(1)), Truth::False);
    }

    #[test]
    fn a_state_name_and_a_string_are_different_things() {
        // The distinction the whole enum kind exists for. Flattened to strings,
        // `status = "held"` and `status = held` become the same question.
        let held = Value::Enum("held".to_owned());
        assert_eq!(held.equals(&Value::Enum("held".to_owned())), Truth::True);
        assert_eq!(held.equals(&Value::Enum("absent".to_owned())), Truth::False);
    }

    #[test]
    fn the_two_numeric_kinds_compare_across() {
        assert_eq!(Value::Int(3).equals(&Value::Float(3.0)), Truth::True);
        assert_eq!(Value::Float(3.0).equals(&Value::Int(3)), Truth::True);
        assert_eq!(Value::Int(3).equals(&Value::Float(3.5)), Truth::False);
    }

    #[test]
    fn decimals_compare_with_a_tolerance() {
        // Spec numbers are written by hand and meant as numbers. Exact float
        // equality would report this pair as different.
        assert_eq!(Value::Float(0.1 + 0.2).equals(&Value::Float(0.3)), Truth::True);
        assert_eq!(Value::Float(1.0).equals(&Value::Float(1.000001)), Truth::False);
    }

    #[test]
    fn collections_compare_element_by_element() {
        let one = Value::Set(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(one.equals(&Value::Set(vec![Value::Int(1), Value::Int(2)])), Truth::True);
        assert_eq!(one.equals(&Value::Set(vec![Value::Int(1)])), Truth::False);
        assert_eq!(
            one.equals(&Value::Set(vec![Value::Int(1), Value::Unknown])),
            Truth::Unknown,
            "one undecided element leaves the whole comparison undecided"
        );
    }

    #[test]
    fn ordering_works_within_a_kind() {
        use std::cmp::Ordering;
        assert_eq!(Value::Int(1).compare(&Value::Int(2)), Some(Ordering::Less));
        assert_eq!(Value::Duration(5).compare(&Value::Duration(5)), Some(Ordering::Equal));
        assert_eq!(Value::Timestamp(9).compare(&Value::Timestamp(2)), Some(Ordering::Greater));
        assert_eq!(
            Value::Str("a".to_owned()).compare(&Value::Str("b".to_owned())),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn ordering_across_incomparable_kinds_has_no_answer() {
        // Not "false": the question has no answer, and the caller reports it as
        // undecided rather than as a definite no.
        assert_eq!(Value::Str("a".to_owned()).compare(&Value::Duration(1)), None);
        assert_eq!(Value::Duration(1).compare(&Value::Timestamp(1)), None);
        assert_eq!(Value::Unknown.compare(&Value::Int(1)), None);
        assert_eq!(Value::Null.compare(&Value::Int(1)), None);
    }

    #[test]
    fn only_a_boolean_is_a_condition() {
        // Allium has no truthiness. Inventing it would let `requires: body`
        // pass for a reason the language does not have.
        assert_eq!(Value::Bool(true).truth(), Truth::True);
        assert_eq!(Value::Bool(false).truth(), Truth::False);
        assert_eq!(Value::Str("anything".to_owned()).truth(), Truth::Unknown);
        assert_eq!(Value::Int(1).truth(), Truth::Unknown);
        assert_eq!(Value::Null.truth(), Truth::Unknown);
    }

    #[test]
    fn counting_a_collection_gives_its_size() {
        assert_eq!(Value::Set(vec![Value::Int(1), Value::Int(2)]).count(), Some(2));
        assert_eq!(Value::Set(Vec::new()).count(), Some(0));
    }

    #[test]
    fn counting_an_absent_relationship_gives_zero() {
        // An entity with no receipts has `receipts.count = 0`, not an error.
        assert_eq!(Value::Null.count(), Some(0));
    }

    #[test]
    fn counting_something_that_is_not_a_collection_has_no_answer() {
        assert_eq!(Value::Int(3).count(), None);
        assert_eq!(Value::Unknown.count(), None);
    }

    #[test]
    fn every_kind_gets_the_right_article() {
        // "a unknown cannot be ordered against a integer" is the sort of thing
        // that makes a reader distrust everything else on the panel.
        assert_eq!(Value::Unknown.described(), "an unknown");
        assert_eq!(Value::Int(0).described(), "an integer");
        assert_eq!(Value::Duration(0).described(), "a duration");
        assert_eq!(Value::Str(String::new()).described(), "a string");
        assert_eq!(Value::Enum(String::new()).described(), "a state");
        assert_eq!(Value::Ref(EntityId::new("E", 1)).described(), "a reference");
    }

    #[test]
    fn every_kind_names_itself_for_a_message() {
        let kinds = [
            Value::Null,
            Value::Bool(true),
            Value::Int(0),
            Value::Float(0.0),
            Value::Str(String::new()),
            Value::Enum(String::new()),
            Value::Duration(0),
            Value::Timestamp(0),
            Value::Ref(EntityId::new("E", 1)),
            Value::Set(Vec::new()),
            Value::Unknown,
        ];
        let mut names: Vec<&str> = kinds.iter().map(Value::kind).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two kinds share a name, so a message would be ambiguous");
    }

    #[test]
    fn an_unset_field_is_unknown_rather_than_null() {
        // Saying "null" would answer a question about the world with a fact
        // about the simulator.
        let instance = Instance::new(EntityId::new("Message", 1), "Message", "messaging");
        assert_eq!(instance.field("body"), Value::Unknown);
        assert!(instance.field("body").is_unknown());
    }

    #[test]
    fn a_set_field_reads_back() {
        let instance = Instance::new(EntityId::new("Message", 1), "Message", "messaging")
            .with("body", Value::Str("hello".to_owned()))
            .with("status", Value::Enum("visible".to_owned()));
        assert_eq!(instance.field("body"), Value::Str("hello".to_owned()));
        assert_eq!(instance.field("status"), Value::Enum("visible".to_owned()));
    }

    #[test]
    fn a_field_set_twice_keeps_the_later_value() {
        let mut instance = Instance::new(EntityId::new("M", 1), "M", "m");
        instance.set("status", Value::Enum("visible".to_owned()));
        instance.set("status", Value::Enum("tombstoned".to_owned()));
        assert_eq!(instance.field("status"), Value::Enum("tombstoned".to_owned()));
    }

    #[test]
    fn a_value_round_trips_through_the_wire_format() {
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::Int(-3),
            Value::Str("hello".to_owned()),
            Value::Enum("visible".to_owned()),
            Value::Duration(1000),
            Value::Timestamp(42),
            Value::Ref(EntityId::new("Message", 2)),
            Value::Set(vec![Value::Int(1)]),
            Value::Unknown,
        ] {
            let json = serde_json::to_string(&value).expect("serialises");
            let back: Value = serde_json::from_str(&json).expect("parses");
            assert_eq!(back, value, "{json}");
        }
    }
}
