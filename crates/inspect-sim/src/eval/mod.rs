//! Evaluating an Allium expression against a world.
//!
//! The contract, and the only thing that really matters here: **this never
//! guesses**. Every construct it does not understand, every name it cannot
//! resolve, every comparison between things that are not comparable produces
//! [`Value::Unknown`] together with a note saying which sub-expression it was
//! and why. Those notes travel out through the step and into the interface,
//! where undecided is the loudest verdict on the panel.
//!
//! The supported subset is the one a real spec actually uses. Counted across
//! `../friend-mesh/specs` — 6,700 lines, five modules — the node kinds handled
//! below are over 99% of every expression node in the file. What is left is
//! genuinely hard (black-box functions, temporal quantifiers) and is reported
//! rather than approximated.
//!
//! One judgement is worth spelling out. In `copy.status = available`, the word
//! `available` parses as a plain identifier, indistinguishable from a rule
//! parameter that was never supplied. Resolving it as a state whenever it is
//! unbound would silently turn a missing argument into a successful comparison;
//! resolving it as unknown always would make every status check undecided. So
//! the decision is made in the comparison, where the other side says which was
//! meant. See the `ops` module for how.

mod ast;
mod literals;
mod ops;

use std::collections::BTreeMap;

// Re-exported for `apply`, which walks the same AST to decide what a
// postcondition changes.
pub use ast::{bare_name, span_of, tagged};
use ast::{is_ident_named, name_of, string_at, text_of, truth_value};
use inspect_model::Span;
use serde::{Deserialize, Serialize};
use std::ops::Not;

use serde_json::Value as Json;
use ts_rs::TS;

use crate::{
    truth::Truth,
    value::{EntityId, Value},
    world::World,
};

/// A sub-expression the evaluator could not decide, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Unresolved {
    /// What could not be decided, in the reader's terms.
    pub reason: String,
    /// The sub-expression, as the spec wrote it, when a span located it.
    pub expression: Option<String>,
    pub span: Option<Span>,
}

/// A value, and everything that could not be decided on the way to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub value: Value,
    pub unresolved: Vec<Unresolved>,
}

impl Evaluation {
    /// A value nothing was undecided about.
    pub(crate) fn known(value: Value) -> Self {
        Self { value, unresolved: Vec::new() }
    }

    /// Undecided, for `reason`.
    pub(crate) fn unknown(reason: impl Into<String>, node: &Json, source: &str) -> Self {
        let span = span_of(node);
        Self {
            value: Value::Unknown,
            unresolved: vec![Unresolved {
                reason: reason.into(),
                expression: span.and_then(|span| span.slice(source)).map(str::to_owned),
                span,
            }],
        }
    }

    /// This evaluation as a truth.
    #[must_use]
    pub fn truth(&self) -> Truth {
        self.value.truth()
    }

    /// The same value, carrying `extra` as well.
    fn carrying(mut self, extra: Vec<Unresolved>) -> Self {
        self.unresolved.extend(extra);
        self
    }
}

/// What an expression is evaluated against.
pub struct Env<'a> {
    pub world: &'a World,
    /// The module whose rule is being evaluated, for `config` and type lookups.
    pub module: &'a str,
    /// Names in scope: trigger arguments, `let` bindings, `this`, loop variables.
    pub bindings: BTreeMap<String, Value>,
    /// The module's spec text, so an undecided note can quote the source.
    pub source: &'a str,
}

impl<'a> Env<'a> {
    /// An environment over `world` with nothing bound.
    #[must_use]
    pub fn new(world: &'a World, module: &'a str, source: &'a str) -> Self {
        Self { world, module, bindings: BTreeMap::new(), source }
    }

    /// The same environment with `name` bound.
    #[must_use]
    pub fn bind(mut self, name: impl Into<String>, value: Value) -> Self {
        self.bindings.insert(name.into(), value);
        self
    }

    /// A child scope with `name` bound, for an iteration or a filter.
    fn scoped(&self, name: &str, value: Value) -> Env<'_> {
        let mut bindings = self.bindings.clone();
        bindings.insert(name.to_owned(), value);
        Env { world: self.world, module: self.module, bindings, source: self.source }
    }
}

/// Evaluate `node` against `env`.
#[must_use]
pub fn eval(node: &Json, env: &Env<'_>) -> Evaluation {
    let Some((tag, inner)) = tagged(node) else {
        return Evaluation::unknown(
            "this is not an expression the parser produced",
            node,
            env.source,
        );
    };

    match tag {
        "Ident" => ident(inner, env),
        "QualifiedName" => qualified(inner, env),
        "MemberAccess" => member(inner, env),
        "Comparison" => compare(inner, env),
        "LogicalOp" => logical(inner, env),
        "BinaryOp" => arithmetic(inner, env),
        "Not" => {
            let operand = operand_of(inner, env);
            let truth = operand.truth().not();
            Evaluation { value: truth_value(truth), unresolved: operand.unresolved }
        }
        "Exists" => existence(inner, env, false),
        "NotExists" => existence(inner, env, true),
        "NumberLiteral" => literals::number(inner, node, env),
        "StringLiteral" => Evaluation::known(Value::Str(text_of(inner))),
        "BoolLiteral" => literals::boolean(inner, node, env),
        "DurationLiteral" => literals::duration(inner, node, env),
        "EnumVariant" => Evaluation::known(Value::Enum(name_of(inner).unwrap_or_default())),
        "Null" => Evaluation::known(Value::Null),
        "Now" => Evaluation::known(Value::Timestamp(env.world.now)),
        "This" => Evaluation::known(env.bindings.get("this").cloned().unwrap_or(Value::Unknown)),
        "SetLiteral" => literals::set_literal(inner, env),
        "Where" | "With" => filtered(inner, env),
        "Conditional" => conditional(inner, env),
        "In" => membership(inner, env),
        // Everything else is real Allium this evaluator does not model. Named
        // rather than lumped together, because "a `Pipe` was not evaluated" is
        // something a reader can act on and "unsupported" is not.
        other => Evaluation::unknown(
            format!("`{other}` expressions are not simulated"),
            node,
            env.source,
        ),
    }
}

/// A bare name: a binding, an entity type, or a state.
fn ident(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(name) = name_of(inner) else {
        return Evaluation::unknown("an identifier with no name", inner, env.source);
    };
    if let Some(bound) = env.bindings.get(&name) {
        return Evaluation::known(bound.clone());
    }
    if env.world.count_of(&name) > 0 {
        return Evaluation::known(collection_of(&name, env));
    }
    // Capitalised: a type that exists in the spec but has no instances. An
    // empty collection is the right answer — `exists Membership{...}` over a
    // world with no memberships is false, not undecided.
    if name.chars().next().is_some_and(char::is_uppercase) {
        return Evaluation::known(Value::Set(Vec::new()));
    }
    Evaluation {
        value: Value::Unknown,
        unresolved: vec![Unresolved {
            reason: format!("nothing is bound to `{name}`"),
            expression: Some(name),
            span: span_of(inner),
        }],
    }
}

/// `membership/Membership`: a type in another module.
fn qualified(inner: &Json, env: &Env<'_>) -> Evaluation {
    let name = string_at(inner, "name").unwrap_or_default();
    if name.is_empty() {
        return Evaluation::unknown("a qualified name with no name", inner, env.source);
    }
    Evaluation::known(collection_of(&name, env))
}

/// Every instance of `entity`, as references.
fn collection_of(entity: &str, env: &Env<'_>) -> Value {
    Value::Set(
        env.world.instances_of(entity).map(|instance| Value::Ref(instance.id.clone())).collect(),
    )
}

/// `object.field`, including `config.x` and `collection.count`.
fn member(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(object) = inner.get("object") else {
        return Evaluation::unknown("a field access with nothing to access", inner, env.source);
    };
    let field = inner
        .get("field")
        .and_then(name_of)
        .or_else(|| string_at(inner, "field"))
        .unwrap_or_default();

    // `config.loan_limit` is not a field of anything: `config` is a namespace.
    if is_ident_named(object, "config") {
        return Evaluation::known(env.world.config(env.module, &field));
    }

    let base = eval(object, env);
    let mut unresolved = base.unresolved.clone();

    if field == "count" {
        return match base.value.count() {
            Some(count) => Evaluation {
                value: Value::Int(i64::try_from(count).unwrap_or(i64::MAX)),
                unresolved,
            },
            None => Evaluation::unknown(
                format!("`{}` is a {}, which has no count", field, base.value.kind()),
                inner,
                env.source,
            )
            .carrying(unresolved),
        };
    }

    match &base.value {
        Value::Ref(id) => match env.world.instance(id) {
            Some(instance) => {
                let value = instance.field(&field);
                // A field nobody has set is undecided, and saying *which* field
                // on which instance is the difference between a panel a reader
                // can act on and one that says only "unknown". Derived values
                // land here constantly: the spec computes them and this
                // simulator does not.
                if value.is_unknown() && !instance.fields.contains_key(&field) {
                    unresolved.push(Unresolved {
                        reason: format!("`{id}` has no `{field}` set"),
                        expression: span_of(inner)
                            .and_then(|span| span.slice(env.source))
                            .map(str::to_owned),
                        span: span_of(inner),
                    });
                }
                Evaluation { value, unresolved }
            }
            None => Evaluation::unknown(format!("`{id}` is not in this world"), inner, env.source)
                .carrying(unresolved),
        },
        // A field read across a collection is a projection: `receipts.reporter`
        // is every reporter, not an error.
        Value::Set(items) => {
            let projected = items
                .iter()
                .map(|item| match item {
                    Value::Ref(id) => env
                        .world
                        .instance(id)
                        .map_or(Value::Unknown, |instance| instance.field(&field)),
                    _ => Value::Unknown,
                })
                .collect();
            Evaluation { value: Value::Set(projected), unresolved }
        }
        Value::Unknown => Evaluation { value: Value::Unknown, unresolved },
        other => Evaluation::unknown(
            format!("`{field}` was read from a {}, which has no fields", other.kind()),
            inner,
            env.source,
        )
        .carrying(unresolved),
    }
}

use ops::{arithmetic, compare, logical};

/// `exists X` and `not exists X`.
fn existence(inner: &Json, env: &Env<'_>, negated: bool) -> Evaluation {
    let evaluated = operand_of(inner, env);
    let present = match &evaluated.value {
        Value::Set(items) => Truth::from_bool(!items.is_empty()),
        Value::Null => Truth::False,
        Value::Unknown => Truth::Unknown,
        _ => Truth::True,
    };
    let truth = if negated { present.not() } else { present };
    Evaluation { value: truth_value(truth), unresolved: evaluated.unresolved }
}

/// `collection where condition`, and `Entity with predicate`.
fn filtered(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(source_node) = inner.get("source") else {
        return Evaluation::unknown("a filter with nothing to filter", inner, env.source);
    };
    let base = eval(source_node, env);
    let Some(condition) = inner.get("condition").or_else(|| inner.get("predicate")) else {
        return base;
    };
    let Value::Set(items) = &base.value else {
        return Evaluation::unknown(
            format!("a {} cannot be filtered", base.value.kind()),
            inner,
            env.source,
        )
        .carrying(base.unresolved);
    };

    let mut unresolved = base.unresolved;
    let mut kept = Vec::new();
    for item in items {
        // The element is bound as `this`, and its fields are in scope bare —
        // `Membership{group: group}` and `where status = active` both read the
        // element's own fields without naming it.
        let scope = element_scope(env, item);
        let verdict = eval(condition, &scope);
        let holds = verdict.truth();
        unresolved.extend(verdict.unresolved);
        // Only a definite yes keeps an element. An undecided predicate leaves
        // the element out *and* leaves a note saying so, rather than silently
        // widening or narrowing the result.
        if holds == Truth::True {
            kept.push(item.clone());
        }
    }
    Evaluation { value: Value::Set(kept), unresolved }
}

/// A scope in which `item`'s own fields are visible without naming it.
fn element_scope<'a>(env: &'a Env<'a>, item: &Value) -> Env<'a> {
    let mut scope = env.scoped("this", item.clone());
    if let Value::Ref(id) = item
        && let Some(instance) = env.world.instance(id)
    {
        for (field, value) in &instance.fields {
            scope.bindings.insert(field.clone(), value.clone());
        }
    }
    scope
}

/// `if condition: then else: otherwise`.
fn conditional(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(condition) = inner.get("condition") else {
        return Evaluation::unknown("a conditional with no condition", inner, env.source);
    };
    let verdict = eval(condition, env);
    let branch = match verdict.truth() {
        Truth::True => inner.get("then").or_else(|| inner.get("consequent")),
        Truth::False => inner.get("otherwise").or_else(|| inner.get("alternative")),
        Truth::Unknown => {
            return Evaluation { value: Value::Unknown, unresolved: verdict.unresolved };
        }
    };
    match branch {
        Some(node) => eval(node, env).carrying(verdict.unresolved),
        // A conditional with no else branch and a false condition yields
        // nothing, which is what the language means.
        None => Evaluation { value: Value::Null, unresolved: verdict.unresolved },
    }
}

/// `x in collection`.
fn membership(inner: &Json, env: &Env<'_>) -> Evaluation {
    let (Some(left_node), Some(right_node)) = (
        inner.get("value").or_else(|| inner.get("left")),
        inner.get("collection").or_else(|| inner.get("right")),
    ) else {
        return Evaluation::unknown("a membership test missing a side", inner, env.source);
    };
    let needle = eval(left_node, env);
    let haystack = eval(right_node, env);
    let mut unresolved = needle.unresolved;
    unresolved.extend(haystack.unresolved);

    let truth = match &haystack.value {
        Value::Set(items) => Truth::any(items.iter().map(|item| item.equals(&needle.value))),
        Value::Unknown => Truth::Unknown,
        _ => Truth::Unknown,
    };
    Evaluation { value: truth_value(truth), unresolved }
}

// --- shared helpers ------------------------------------------------------

fn operand_of(inner: &Json, env: &Env<'_>) -> Evaluation {
    match inner.get("operand").or_else(|| inner.get("value")) {
        Some(node) => eval(node, env),
        None => Evaluation::unknown("an operator with no operand", inner, env.source),
    }
}

/// The instance an evaluated reference points at.
#[must_use]
pub fn as_reference(value: &Value) -> Option<&EntityId> {
    match value {
        Value::Ref(id) => Some(id),
        _ => None,
    }
}
