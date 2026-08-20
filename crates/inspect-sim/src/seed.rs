//! Starting a simulation from the specification's own defaults.
//!
//! A world you have to fill in by hand before anything can happen is a world
//! nobody starts. The spec already states its configuration — `loan_limit:
//! Integer = 5`, `loan_period: Duration = 21.days` — so a new world begins with
//! those in force and the user only has to supply what is genuinely theirs to
//! choose: which entities exist.
//!
//! Defaults are parsed from the text the spec wrote rather than from an
//! evaluated tree, because that is what `allium model` reports. A default this
//! module cannot read is left unset rather than guessed at, and reading it back
//! then comes out as undecided with a note — the same treatment as anything else
//! the simulator does not know.

use inspect_model::{NodeKind, SpecGraph};

use crate::{value::Value, world::World};

/// A world with every module's configuration defaults in force.
#[must_use]
pub fn seed(spec: &SpecGraph) -> World {
    let mut world = World::new();
    for node in spec.nodes_of(NodeKind::Config) {
        let Some(detail) = as_config(node) else { continue };
        for parameter in &detail.parameters {
            let Some(text) = &parameter.default_expr else { continue };
            if let Some(value) = literal(text) {
                world.set_config(&node.module, &parameter.name, value);
            }
        }
    }
    world
}

fn as_config(node: &inspect_model::Node) -> Option<&inspect_model::graph::ConfigDetail> {
    match &node.detail {
        inspect_model::NodeDetail::Config(detail) => Some(detail),
        _ => None,
    }
}

/// A configuration default, as written.
///
/// Deliberately narrow. These are the four shapes a config block actually uses,
/// and anything else — an expression, a reference to another parameter — is left
/// for the user to set, because a wrong default is worse than an absent one.
#[must_use]
pub fn literal(text: &str) -> Option<Value> {
    let text = text.trim();
    match text {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        "null" => return Some(Value::Null),
        _ => {}
    }

    if let Some(quoted) = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) {
        return Some(Value::Str(quoted.to_owned()));
    }

    let cleaned = text.replace('_', "");
    if let Ok(whole) = cleaned.parse::<i64>() {
        return Some(Value::Int(whole));
    }

    if let Some((amount, unit)) = cleaned.split_once('.') {
        if let (Ok(amount), Some(millis)) = (amount.parse::<i64>(), unit_millis(unit)) {
            return amount.checked_mul(millis).map(Value::Duration);
        }
        // `1.5` is a decimal, not a duration with a unit called `5`.
        if let Ok(decimal) = cleaned.parse::<f64>() {
            return Some(Value::Float(decimal));
        }
    }

    None
}

fn unit_millis(unit: &str) -> Option<i64> {
    // One `s`, not every trailing `s`: `ms` is a unit in its own right and
    // de-pluralising it leaves `m`, which is nothing.
    let singular = if unit == "ms" { unit } else { unit.strip_suffix('s').unwrap_or(unit) };
    match singular {
        "millisecond" | "ms" => Some(1),
        "second" | "sec" => Some(1_000),
        "minute" | "min" => Some(60_000),
        "hour" | "hr" => Some(3_600_000),
        "day" => Some(86_400_000),
        "week" => Some(604_800_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use inspect_model::{
        Node, NodeDetail, SpecGraph,
        graph::{ConfigDetail, ConfigParameter},
    };

    use super::*;

    fn config(module: &str, parameters: &[(&str, &str, Option<&str>)]) -> Node {
        Node::new(module, NodeKind::Config, "config").with(NodeDetail::Config(ConfigDetail {
            parameters: parameters
                .iter()
                .map(|(name, type_expr, default)| ConfigParameter {
                    name: (*name).to_owned(),
                    type_expr: (*type_expr).to_owned(),
                    default_expr: default.map(ToOwned::to_owned),
                })
                .collect(),
        }))
    }

    #[test]
    fn an_integer_default_is_read() {
        assert_eq!(literal("5"), Some(Value::Int(5)));
        assert_eq!(literal("-3"), Some(Value::Int(-3)));
        assert_eq!(literal(" 20 "), Some(Value::Int(20)));
    }

    #[test]
    fn digit_separators_survive() {
        assert_eq!(literal("2_000_000_000"), Some(Value::Int(2_000_000_000)));
    }

    #[test]
    fn a_duration_default_keeps_its_kind() {
        assert_eq!(literal("21.days"), Some(Value::Duration(21 * 86_400_000)));
        assert_eq!(literal("24.hours"), Some(Value::Duration(86_400_000)));
        assert_eq!(literal("200.seconds"), Some(Value::Duration(200_000)));
        assert_eq!(literal("1.day"), Some(Value::Duration(86_400_000)));
    }

    #[test]
    fn every_unit_the_language_writes_converts_to_the_same_scale() {
        // The whole set, because a unit that is silently unrecognised does not
        // fail — it leaves the parameter unset, and every rule that reads it
        // comes back undecided for a reason nobody can see.
        assert_eq!(literal("500.milliseconds"), Some(Value::Duration(500)));
        assert_eq!(literal("500.ms"), Some(Value::Duration(500)));
        assert_eq!(literal("30.minutes"), Some(Value::Duration(1_800_000)));
        assert_eq!(literal("30.min"), Some(Value::Duration(1_800_000)));
        assert_eq!(literal("2.weeks"), Some(Value::Duration(1_209_600_000)));
        assert_eq!(literal("1.week"), Some(Value::Duration(604_800_000)));
    }

    #[test]
    fn a_unit_nobody_writes_is_not_a_duration() {
        assert_eq!(literal("3.fortnights"), None);
    }

    #[test]
    fn a_decimal_is_not_mistaken_for_a_duration() {
        // `1.5` splits on the dot exactly as `1.days` does; the unit is what
        // separates them.
        assert_eq!(literal("1.5"), Some(Value::Float(1.5)));
    }

    #[test]
    fn booleans_and_null_and_strings_are_read() {
        assert_eq!(literal("true"), Some(Value::Bool(true)));
        assert_eq!(literal("false"), Some(Value::Bool(false)));
        assert_eq!(literal("null"), Some(Value::Null));
        assert_eq!(literal("\"hello\""), Some(Value::Str("hello".to_owned())));
    }

    #[test]
    fn a_default_this_module_cannot_read_is_left_unset() {
        // A wrong default is worse than an absent one: absent reads back as
        // undecided with a note, and wrong reads back as a confident lie.
        assert_eq!(literal("other_parameter * 2"), None);
        assert_eq!(literal("3.fortnights"), None);
        assert_eq!(literal(""), None);
    }

    #[test]
    fn seeding_puts_every_modules_defaults_in_force() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config(
            "lending",
            &[("loan_limit", "Integer", Some("5")), ("loan_period", "Duration", Some("21.days"))],
        ));
        spec.nodes.push(config("catalogue", &[("max_copies", "Integer", Some("20"))]));

        let world = seed(&spec);
        assert_eq!(world.config("lending", "loan_limit"), Value::Int(5));
        assert_eq!(world.config("lending", "loan_period"), Value::Duration(21 * 86_400_000));
        assert_eq!(world.config("catalogue", "max_copies"), Value::Int(20));
    }

    #[test]
    fn two_modules_may_use_the_same_parameter_name() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config("a", &[("limit", "Integer", Some("1"))]));
        spec.nodes.push(config("b", &[("limit", "Integer", Some("2"))]));

        let world = seed(&spec);
        assert_eq!(world.config("a", "limit"), Value::Int(1));
        assert_eq!(world.config("b", "limit"), Value::Int(2));
    }

    #[test]
    fn a_parameter_with_no_default_is_left_for_the_user() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config("m", &[("chosen", "Integer", None)]));
        assert_eq!(seed(&spec).config("m", "chosen"), Value::Unknown);
    }

    #[test]
    fn a_seeded_world_holds_no_entities_and_starts_at_zero() {
        // Which entities exist is the user's to choose; the configuration is
        // the spec's.
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config("m", &[("limit", "Integer", Some("1"))]));
        let world = seed(&spec);
        assert!(world.entities.is_empty());
        assert_eq!(world.now, 0);
    }

    #[test]
    fn seeding_a_spec_with_no_config_gives_an_empty_world() {
        assert_eq!(seed(&SpecGraph::new("test")), World::new());
    }

    #[test]
    fn seeding_is_deterministic() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config("m", &[("a", "Integer", Some("1")), ("b", "Integer", Some("2"))]));
        assert_eq!(seed(&spec), seed(&spec));
    }
}
