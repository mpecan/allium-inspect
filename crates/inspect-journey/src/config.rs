//! `config.<name> = …`: walking a journey against configuration the spec does
//! not default to.
//!
//! Much of a real system is off until somebody turns it on, and a journey
//! that could only start from the spec's defaults could only ever show that
//! behaviour refused. So a journey may set a parameter wherever it can set a
//! field — `given`, a file's `world`, a `stipulate` — and is reported doing it,
//! because a step holding on the strength of a switch the spec leaves off is
//! exactly the invisible pass the ledger exists to prevent.
//!
//! What is written is resolved against the `config` blocks the spec declares,
//! not against whatever the world happens to hold. A parameter with no default
//! is in no world until somebody sets it, and it is still a parameter; a name
//! no block declares is not one, however close its spelling.

use inspect_model::{NodeDetail, NodeKind, SpecGraph};
use inspect_sim::Value;

use crate::{check::Verdict, journey::Path};

/// Why a write the journey asked for did not happen, and what kind of answer
/// that is.
///
/// Two kinds, because they send a reader to two places: a parameter the spec
/// never declared is the spec's to add (`Unspecified`), and a line this tool
/// cannot make sense of is the journey's to fix (`Undecided`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unwritten {
    pub(crate) verdict: Verdict,
    pub(crate) reason: String,
}

impl Unwritten {
    pub(crate) fn undecided(reason: impl Into<String>) -> Self {
        Self { verdict: Verdict::Undecided, reason: reason.into() }
    }

    fn unspecified(reason: impl Into<String>) -> Self {
        Self { verdict: Verdict::Unspecified, reason: reason.into() }
    }
}

/// One parameter a `config` block declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) type_expr: String,
}

/// Whether `path` is about configuration: `config.x`, or `module/config.x`.
pub(crate) fn is_config(path: &Path) -> bool {
    qualifier(&path.root).is_some()
}

/// `None` for a root that is not `config` at all; `Some(None)` for bare
/// `config`; `Some(Some(module))` for `module/config`.
fn qualifier(root: &str) -> Option<Option<&str>> {
    if root == "config" {
        return Some(None);
    }
    root.strip_suffix("/config").filter(|module| !module.is_empty()).map(Some)
}

/// The parameter `path` names.
///
/// Bare `config.x` is whichever module declares `x`, when exactly one does.
/// When several do, picking one is a guess about which the journey meant, so
/// it is refused with the spelling that says.
pub(crate) fn resolve(spec: &SpecGraph, path: &Path) -> Result<Parameter, Unwritten> {
    let Some(module) = qualifier(&path.root) else {
        return Err(Unwritten::undecided(format!("`{}` is not configuration", path.as_written())));
    };
    let [name] = path.segments.as_slice() else {
        return Err(Unwritten::undecided(format!(
            "`{}` does not name one parameter — write `{}.<name>`",
            path.as_written(),
            path.root
        )));
    };

    let declared: Vec<Parameter> = declared(spec)
        .filter(|parameter| &parameter.name == name)
        .filter(|parameter| module.is_none_or(|module| parameter.module == module))
        .collect();

    match declared.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Unwritten::unspecified(match module {
            Some(module) => format!("the config of `{module}` declares no `{name}`"),
            None => format!("no `config` block declares `{name}`"),
        })),
        several => {
            let modules: Vec<String> =
                several.iter().map(|parameter| format!("`{}`", parameter.module)).collect();
            Err(Unwritten::undecided(format!(
                "`{name}` is declared in the config of {} — write `{}/config.{name}` to say which",
                modules.join(" and "),
                several[0].module
            )))
        }
    }
}

/// Whether `value` can stand for a parameter of `type_expr`.
///
/// The primitive types only. Anything else — an enumeration, an entity — is
/// accepted, because refusing a value for a type this does not read would be
/// this tool being wrong about the journey.
pub(crate) fn admits(parameter: &Parameter, value: &Value) -> Result<(), Unwritten> {
    let written = parameter.type_expr.trim();
    let optional = written.ends_with('?');
    let base = written.trim_end_matches('?').trim();
    let fits = match value {
        // Undecided in, undecided out: the evaluator says so where it is read.
        Value::Unknown => true,
        Value::Null => optional,
        _ => match base {
            "Boolean" => matches!(value, Value::Bool(_)),
            "Integer" => matches!(value, Value::Int(_)),
            "Decimal" => matches!(value, Value::Int(_) | Value::Float(_)),
            "String" => matches!(value, Value::Str(_)),
            "Duration" => matches!(value, Value::Duration(_)),
            "Timestamp" => matches!(value, Value::Timestamp(_)),
            _ => true,
        },
    };
    if fits {
        return Ok(());
    }
    Err(Unwritten::undecided(format!(
        "`config.{}` is declared `{written}`, and `{}` is not one, so it was not set",
        parameter.name,
        value.render()
    )))
}

/// Every parameter every `config` block declares.
fn declared(spec: &SpecGraph) -> impl Iterator<Item = Parameter> + '_ {
    spec.nodes_of(NodeKind::Config).flat_map(|node| {
        let parameters = match &node.detail {
            NodeDetail::Config(detail) => detail.parameters.as_slice(),
            _ => &[],
        };
        parameters.iter().map(|parameter| Parameter {
            module: node.module.clone(),
            name: parameter.name.clone(),
            type_expr: parameter.type_expr.clone(),
        })
    })
}

#[cfg(test)]
mod tests {
    use inspect_model::{
        Node,
        graph::{ConfigDetail, ConfigParameter},
    };

    use super::*;

    /// A module and the `(name, type)` of each parameter its block declares.
    type Block<'a> = (&'a str, &'a [(&'a str, &'a str)]);

    fn spec(blocks: &[Block<'_>]) -> SpecGraph {
        let mut graph = SpecGraph::new("test");
        for (module, parameters) in blocks {
            let parameters = parameters
                .iter()
                .map(|(name, type_expr)| ConfigParameter {
                    name: (*name).to_owned(),
                    type_expr: (*type_expr).to_owned(),
                    default_expr: None,
                })
                .collect();
            graph.nodes.push(
                Node::new(module, NodeKind::Config, "config")
                    .with(NodeDetail::Config(ConfigDetail { parameters })),
            );
        }
        graph
    }

    fn path(root: &str, segments: &[&str]) -> Path {
        Path {
            root: root.to_owned(),
            segments: segments.iter().map(|segment| (*segment).to_owned()).collect(),
        }
    }

    fn parameter(type_expr: &str) -> Parameter {
        Parameter { module: "m".to_owned(), name: "p".to_owned(), type_expr: type_expr.to_owned() }
    }

    #[test]
    fn config_bare_or_qualified_is_configuration_and_nothing_else_is() {
        assert!(is_config(&path("config", &["x"])));
        assert!(is_config(&path("lending/config", &["x"])));
        assert!(!is_config(&path("ada", &["config"])));
        assert!(!is_config(&path("/config", &["x"])));
        assert!(!is_config(&path("configs", &["x"])));
    }

    #[test]
    fn a_name_one_module_declares_resolves_to_that_module() {
        let graph = spec(&[("a", &[("on", "Boolean")]), ("b", &[("limit", "Integer")])]);
        let found = resolve(&graph, &path("config", &["limit"])).unwrap();
        assert_eq!(found.module, "b");
        assert_eq!(found.type_expr, "Integer");
    }

    #[test]
    fn a_name_two_modules_declare_is_not_guessed_between() {
        let graph = spec(&[("a", &[("limit", "Integer")]), ("b", &[("limit", "Integer")])]);
        let refused = resolve(&graph, &path("config", &["limit"])).unwrap_err();
        assert_eq!(refused.verdict, Verdict::Undecided);
        assert!(refused.reason.contains("`a` and `b`"), "{}", refused.reason);
        assert!(refused.reason.contains("a/config.limit"), "{}", refused.reason);
        // And the spelling it offers is the one that settles it.
        let chosen = resolve(&graph, &path("b/config", &["limit"])).unwrap();
        assert_eq!(chosen.module, "b");
    }

    #[test]
    fn a_name_nobody_declares_is_the_specs_to_add() {
        let graph = spec(&[("a", &[("limit", "Integer")])]);
        let bare = resolve(&graph, &path("config", &["limt"])).unwrap_err();
        assert_eq!(bare.verdict, Verdict::Unspecified);
        assert!(bare.reason.contains("`limt`"), "{}", bare.reason);
        let elsewhere = resolve(&graph, &path("b/config", &["limit"])).unwrap_err();
        assert_eq!(elsewhere.verdict, Verdict::Unspecified);
        assert!(elsewhere.reason.contains("`b`"), "{}", elsewhere.reason);
    }

    #[test]
    fn a_path_that_is_not_one_parameter_is_refused() {
        let graph = spec(&[("a", &[("limit", "Integer")])]);
        for segments in [&[][..], &["limit", "deep"][..]] {
            let refused = resolve(&graph, &path("config", segments)).unwrap_err();
            assert_eq!(refused.verdict, Verdict::Undecided, "{segments:?}");
        }
        let elsewhere = resolve(&graph, &path("ada", &["limit"])).unwrap_err();
        assert!(elsewhere.reason.contains("not configuration"), "{}", elsewhere.reason);
    }

    #[test]
    fn a_value_of_the_declared_type_is_admitted() {
        assert!(admits(&parameter("Boolean"), &Value::Bool(true)).is_ok());
        assert!(admits(&parameter("Integer"), &Value::Int(2)).is_ok());
        assert!(admits(&parameter("Decimal"), &Value::Int(2)).is_ok());
        assert!(admits(&parameter("Decimal"), &Value::Float(0.5)).is_ok());
        assert!(admits(&parameter("String"), &Value::Str("x".to_owned())).is_ok());
        assert!(admits(&parameter("Duration"), &Value::Duration(1)).is_ok());
        assert!(admits(&parameter("Timestamp"), &Value::Timestamp(1)).is_ok());
        assert!(admits(&parameter("Boolean"), &Value::Unknown).is_ok());
        assert!(admits(&parameter("Integer?"), &Value::Null).is_ok());
        // A type this does not read is not second-guessed.
        assert!(admits(&parameter("Mode"), &Value::Enum("fast".to_owned())).is_ok());
    }

    #[test]
    fn a_value_of_another_type_is_refused_naming_the_declared_one() {
        for (type_expr, value) in [
            ("Boolean", Value::Int(5)),
            ("Integer", Value::Bool(true)),
            ("Integer", Value::Float(0.5)),
            ("Decimal", Value::Str("1".to_owned())),
            ("String", Value::Int(1)),
            ("Duration", Value::Int(1)),
            ("Timestamp", Value::Duration(1)),
            ("Integer", Value::Null),
        ] {
            let refused = admits(&parameter(type_expr), &value).unwrap_err();
            assert_eq!(refused.verdict, Verdict::Undecided);
            assert!(refused.reason.contains(&format!("`{type_expr}`")), "{}", refused.reason);
        }
    }
}
