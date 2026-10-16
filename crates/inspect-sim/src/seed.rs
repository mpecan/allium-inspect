//! Starting a simulation from the specification's own defaults.
//!
//! A world you have to fill in by hand before anything can happen is a world
//! nobody starts. The spec already states its configuration — `loan_limit:
//! Integer = 5`, `loan_period: Duration = 21.days` — so a new world begins with
//! those in force and the user only has to supply what is genuinely theirs to
//! choose: which entities exist.
//!
//! Defaults are parsed from the text the spec wrote rather than from an
//! evaluated tree, because that is what `allium model` reports. That covers
//! the literals, the values of an enumeration the parameter's type names, and
//! a set or list of either — `apps: Set<App> = {a, b}` (issue #25). A default this
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
            let reading = Reading { spec, module: &node.module, block: detail };
            if let Some(value) = reading.default(text, &parameter.type_expr) {
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

/// Where a default is read: the spec, for which enumerations exist, and the
/// block it is written in, for which bare words are its other parameters.
struct Reading<'a> {
    spec: &'a SpecGraph,
    module: &'a str,
    block: &'a inspect_model::graph::ConfigDetail,
}

impl Reading<'_> {
    /// A default written for a parameter declared `type_expr`.
    ///
    /// A collection — `{a, b}`, `{"x"}`, `[3, 1]`, `{}` — is read element by
    /// element, and is read only when every element is: a set with one member
    /// left out is a smaller set, which is a confident wrong answer rather
    /// than an undecided one.
    fn default(&self, text: &str, type_expr: &str) -> Option<Value> {
        let text = text.trim();
        let type_expr = type_expr.trim().trim_end_matches('?').trim();
        let (inside, set) = if let Some(inside) = enclosed(text, '{', '}') {
            (inside, true)
        } else if let Some(inside) = enclosed(text, '[', ']') {
            (inside, false)
        } else {
            return self.scalar(text, type_expr);
        };
        let element_type = element_type(type_expr)?;
        let items = elements(inside)?
            .into_iter()
            .map(|part| self.scalar(part, element_type))
            .collect::<Option<Vec<Value>>>()?;
        // `{a, a}` is the set of one thing, built the way the evaluator builds
        // the same words in a rule. A list keeps what it was given.
        Some(if set { Value::set_of(items) } else { Value::Set(items) })
    }

    /// One value: a literal [`literal`] reads, or a bare word naming a value
    /// of the enumeration `type_expr` names.
    ///
    /// The bare word is the case that needs care, because `second: Integer =
    /// first` is a bare word too and means the parameter `first`. So a word
    /// is a value only when the declared type is an enumeration that declares
    /// it, and no parameter of the same block shares its name — both, or it
    /// is left unset rather than picked.
    fn scalar(&self, text: &str, type_expr: &str) -> Option<Value> {
        let text = text.trim();
        if let Some(value) = literal(text) {
            return Some(value);
        }
        if !is_word(text) {
            return None;
        }
        let declared = self.spec.enum_values_of(self.module, type_expr);
        let a_parameter = self.block.parameters.iter().any(|parameter| parameter.name == text);
        (declared.iter().any(|value| value == text) && !a_parameter)
            .then(|| Value::Enum(text.to_owned()))
    }
}

/// `text` without its `open` and `close`, when it is wrapped in them.
fn enclosed(text: &str, open: char, close: char) -> Option<&str> {
    text.strip_prefix(open)?.strip_suffix(close)
}

/// `App` out of `Set<App>` or `List<App>`. Nothing for a type that is not a
/// collection: a collection default for a scalar parameter is the spec's
/// mistake, and `allium check` is the place that says so.
fn element_type(type_expr: &str) -> Option<&str> {
    ["Set<", "List<"]
        .iter()
        .find_map(|prefix| type_expr.strip_prefix(prefix))
        .and_then(|rest| rest.strip_suffix('>'))
        .map(str::trim)
}

/// The elements of a collection's inside, as `allium parse` separates them.
///
/// A comma, a line break or a space ends an element, and a `--` comment runs
/// to the end of its line: a default written one member to a line, with a
/// note beside each and a comma after the last, is the shape a long one takes
/// (issue #25). Inside a string none of those is anything but text — Allium
/// strings have no escapes, so the next `"` always closes one. Nothing for a
/// comma with no element before it — `{a,, b}`, `{, a}` — which `allium`
/// refuses too, or for a string left open.
///
/// Anything else an element might be — a nested collection, an expression —
/// is split wherever it is split and then refused by the element reader, so
/// the whole default is left unset rather than read short.
fn elements(inside: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let (mut quoted, mut start, mut comma_allowed) = (false, None, false);
    let mut characters = inside.char_indices().peekable();
    while let Some((at, character)) = characters.next() {
        if quoted {
            quoted = character != '"';
            continue;
        }
        let ends = character == ','
            || character.is_whitespace()
            || (character == '-' && characters.peek().is_some_and(|(_, next)| *next == '-'));
        if !ends {
            quoted = character == '"';
            start.get_or_insert(at);
            continue;
        }
        if let Some(from) = start.take() {
            parts.push(&inside[from..at]);
            comma_allowed = true;
        }
        match character {
            ',' if !comma_allowed => return None,
            ',' => comma_allowed = false,
            '-' => while characters.next_if(|(_, next)| *next != '\n').is_some() {},
            _ => {}
        }
    }
    if let Some(from) = start {
        parts.push(&inside[from..]);
    }
    (!quoted).then_some(parts)
}

/// An identifier, and nothing else: no dot, no operator, no space.
fn is_word(text: &str) -> bool {
    let mut characters = text.chars();
    characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

/// A configuration default, as written.
///
/// Deliberately narrow. These are the scalar shapes a config block actually
/// uses, and anything else — an expression, a reference to another parameter —
/// is left for the user to set, because a wrong default is worse than an
/// absent one. A collection, and a bare enumeration value, are read by
/// [`seed`], which knows the type the parameter was declared with; this does
/// not, and a journey reads its own terms with it.
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
        graph::{ConfigDetail, ConfigParameter, EnumDetail},
    };

    use super::*;

    fn an_enum(module: &str, name: &str, values: &[&str]) -> Node {
        Node::new(module, NodeKind::Enum, name).with(NodeDetail::Enum(EnumDetail {
            values: values.iter().map(|value| (*value).to_owned()).collect(),
        }))
    }

    /// One module `m` declaring `enum App { a | b | c }` and `parameters`.
    fn seeded(parameters: &[(&str, &str, Option<&str>)]) -> World {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(an_enum("m", "App", &["a", "b", "c"]));
        spec.nodes.push(config("m", parameters));
        seed(&spec)
    }

    fn enums(values: &[&str]) -> Value {
        Value::Set(values.iter().map(|value| Value::Enum((*value).to_owned())).collect())
    }

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
    fn a_set_of_enumeration_values_is_seeded() {
        // Issue #25: the reproduction as filed.
        let world = seeded(&[("apps", "Set<App>", Some("{a, b}"))]);
        assert_eq!(world.config("m", "apps"), enums(&["a", "b"]));
    }

    #[test]
    fn a_set_of_strings_and_a_list_of_integers_are_seeded() {
        let world = seeded(&[
            ("labels", "Set<String>", Some(r#"{"alpha", "a, b"}"#)),
            ("sizes", "List<Integer>", Some("[3, 1, 3]")),
            ("waits", "Set<Duration>", Some("{1.day, 2.hours}")),
        ]);
        assert_eq!(
            world.config("m", "labels"),
            Value::Set(vec![Value::Str("alpha".to_owned()), Value::Str("a, b".to_owned())])
        );
        // A list keeps what it was given, repeats and order both.
        assert_eq!(
            world.config("m", "sizes"),
            Value::Set(vec![Value::Int(3), Value::Int(1), Value::Int(3)])
        );
        assert_eq!(
            world.config("m", "waits"),
            Value::Set(vec![Value::Duration(86_400_000), Value::Duration(7_200_000)])
        );
    }

    /// A lone `-` is a sign, not the start of a comment: only `--` is one.
    #[test]
    fn a_negative_member_is_a_number_and_not_a_comment() {
        let world = seeded(&[
            ("offsets", "Set<Integer>", Some("{-1, 2}")),
            ("steps", "List<Integer>", Some("[2, -3]")),
        ]);
        assert_eq!(world.config("m", "offsets"), Value::Set(vec![Value::Int(-1), Value::Int(2)]));
        assert_eq!(world.config("m", "steps"), Value::Set(vec![Value::Int(2), Value::Int(-3)]));
    }

    #[test]
    fn a_set_written_with_a_repeat_holds_it_once() {
        let world = seeded(&[("apps", "Set<App>", Some("{a, b, a}"))]);
        assert_eq!(world.config("m", "apps"), enums(&["a", "b"]));
    }

    #[test]
    fn an_empty_set_or_list_is_a_default_and_not_an_absence() {
        let world =
            seeded(&[("none", "Set<App>", Some("{}")), ("nothing", "List<String>", Some("[ ]"))]);
        assert_eq!(world.config("m", "none"), Value::Set(Vec::new()));
        assert_eq!(world.config("m", "nothing"), Value::Set(Vec::new()));
    }

    #[test]
    fn an_optional_collection_is_read_the_same() {
        let world = seeded(&[("apps", "Set<App>?", Some("{c}"))]);
        assert_eq!(world.config("m", "apps"), enums(&["c"]));
    }

    #[test]
    fn a_bare_enumeration_value_is_seeded() {
        let world = seeded(&[("favourite", "App", Some("c")), ("maybe", "App?", Some("b"))]);
        assert_eq!(world.config("m", "favourite"), Value::Enum("c".to_owned()));
        assert_eq!(world.config("m", "maybe"), Value::Enum("b".to_owned()));
    }

    #[test]
    fn a_word_the_enumeration_does_not_declare_is_left_unset() {
        // A typo is not a value: `allium check` says so, and the simulator
        // says it could not read the default rather than inventing a state.
        let world =
            seeded(&[("favourite", "App", Some("z")), ("apps", "Set<App>", Some("{a, z}"))]);
        assert_eq!(world.config("m", "favourite"), Value::Unknown);
        assert_eq!(world.config("m", "apps"), Value::Unknown);
        assert!(!world.has_config("m", "apps"));
    }

    #[test]
    fn a_word_for_a_type_that_is_no_enumeration_is_left_unset() {
        // `second: Integer = first` names the parameter `first`; it is not a
        // value of anything.
        let world = seeded(&[
            ("first", "Integer", Some("1")),
            ("second", "Integer", Some("first")),
            ("words", "Set<String>", Some("{first}")),
        ]);
        assert_eq!(world.config("m", "second"), Value::Unknown);
        assert_eq!(world.config("m", "words"), Value::Unknown);
    }

    #[test]
    fn a_word_that_is_also_a_parameter_is_left_unset() {
        // `a` is a value of `App` and the name of a parameter in the same
        // block. Which one the default means is not this reader's to pick.
        let world = seeded(&[("a", "App", Some("b")), ("apps", "Set<App>", Some("{a, b}"))]);
        assert_eq!(world.config("m", "a"), Value::Enum("b".to_owned()));
        assert_eq!(world.config("m", "apps"), Value::Unknown);
    }

    #[test]
    fn a_set_with_one_unreadable_member_is_left_unset_whole() {
        // Dropping the member would leave a smaller set, read back as known.
        let world = seeded(&[
            ("favourite", "App", Some("c")),
            ("chosen", "Set<App>", Some("{a, favourite}")),
            ("sums", "Set<Integer>", Some("{1, other + 1}")),
            ("deep", "Set<Set<App>>", Some("{{a}, {b}}")),
        ]);
        assert_eq!(world.config("m", "chosen"), Value::Unknown);
        assert_eq!(world.config("m", "sums"), Value::Unknown);
        assert_eq!(world.config("m", "deep"), Value::Unknown);
    }

    #[test]
    fn a_collection_written_wrong_is_left_unset() {
        let world = seeded(&[
            ("gap", "Set<App>", Some("{a,, b}")),
            ("leading", "Set<App>", Some("{, a}")),
            ("alone", "Set<App>", Some("{,}")),
            ("open", "Set<String>", Some(r#"{"a, b}"#)),
            ("unbalanced", "Set<App>", Some("{a)}")),
            ("scalar", "App", Some("{a}")),
            ("mismatched", "Set<App>", Some("{a]")),
        ]);
        for name in ["gap", "leading", "alone", "open", "unbalanced", "scalar", "mismatched"] {
            assert_eq!(world.config("m", name), Value::Unknown, "{name}");
        }
    }

    #[test]
    fn a_collection_written_a_member_to_a_line_is_read_as_allium_reads_it() {
        // Each of these is `{a, b}` to `allium parse`: a trailing comma, a
        // line break or a space between members, a comment beside one. Read
        // as text split only on commas, all four were left unset.
        let world = seeded(&[
            ("trailing", "Set<App>", Some("{a, b,}")),
            ("lines", "Set<App>", Some("{\n    a\n    b\n}")),
            ("spaced", "Set<App>", Some("{a b}")),
            (
                "noted",
                "Set<App>",
                Some("{\n    a, -- the first, with a comma, here\n    b, -- b\n}"),
            ),
            ("quoted", "Set<String>", Some("{\"x -- y\", -- note\n \"a b\",}")),
        ]);
        for name in ["trailing", "lines", "spaced", "noted"] {
            assert_eq!(world.config("m", name), enums(&["a", "b"]), "{name}");
        }
        assert_eq!(
            world.config("m", "quoted"),
            Value::Set(vec![Value::Str("x -- y".to_owned()), Value::Str("a b".to_owned())])
        );
    }

    #[test]
    fn a_set_written_with_a_repeat_agrees_with_the_same_words_in_a_rule() {
        // `{1, 1.0}` is one member to `Value::equals`; held twice, the set the
        // seed made and the set a rule spelled alike made had different sizes.
        let world = seeded(&[("ones", "Set<Integer>", Some("{1, 1.0}"))]);
        assert_eq!(world.config("m", "ones"), Value::Set(vec![Value::Int(1)]));
    }

    #[test]
    fn an_enumeration_is_read_from_the_parameters_own_module() {
        // Two modules each declare an `App` with different values; `m`'s
        // default is read against `m`'s.
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(an_enum("m", "App", &["a"]));
        spec.nodes.push(an_enum("n", "App", &["z"]));
        spec.nodes.push(config("m", &[("apps", "Set<App>", Some("{a}"))]));
        spec.nodes.push(config("n", &[("apps", "Set<App>", Some("{a}"))]));
        let world = seed(&spec);
        assert_eq!(world.config("m", "apps"), enums(&["a"]));
        assert_eq!(world.config("n", "apps"), Value::Unknown);
    }

    #[test]
    fn a_word_is_an_identifier_and_nothing_more() {
        assert!(is_word("a"));
        assert!(is_word("_x1"));
        assert!(is_word("snake_case"));
        assert!(!is_word(""));
        assert!(!is_word("1a"));
        assert!(!is_word("a.b"));
        assert!(!is_word("a b"));
        assert!(!is_word("a-b"));
    }

    #[test]
    fn seeding_is_deterministic() {
        let mut spec = SpecGraph::new("test");
        spec.nodes.push(config("m", &[("a", "Integer", Some("1")), ("b", "Integer", Some("2"))]));
        assert_eq!(seed(&spec), seed(&spec));
    }
}
