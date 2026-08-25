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
//! The tree is allium's own, taken from `allium_parser` rather than read back
//! out of the JSON the CLI prints. That is what turns "an expression form this
//! evaluator has never heard of" from a run-time `unknown` into a `match` that
//! does not compile — and the list below is now provably the whole language
//! minus what is deliberately left out, rather than provably a list somebody
//! kept up to date by hand.
//!
//! One judgement is worth spelling out. In `copy.status = available`, the word
//! `available` parses as a plain identifier, indistinguishable from a rule
//! parameter that was never supplied. Resolving it as a state whenever it is
//! unbound would silently turn a missing argument into a successful comparison;
//! resolving it as unknown always would make every status check undecided. So
//! the decision is made in the comparison, where the other side says which was
//! meant. See the `ops` module for how.

mod ast;
mod collections;
mod literals;
mod ops;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use allium_parser::ast::{CondBranch, Expr, Ident, QualifiedName};
// Re-exported for `apply`, which walks the same tree to decide what a
// postcondition changes.
pub use ast::{bare_name, span_of};
use ast::{is_ident_named, literal_text, truth_value};
use inspect_model::Span;
use serde::{Deserialize, Serialize};
use std::ops::Not;

use ts_rs::TS;

use crate::{
    truth::Truth,
    value::{Instance, Value},
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

/// One reason, anchored to the sub-expression it is about.
///
/// Split out of [`Evaluation::unknown`] because a few places decide something
/// is undecided while already holding reasons from their operands, and have to
/// add to that list rather than replace it.
pub(crate) fn unresolved_at(reason: impl Into<String>, at: &Expr, source: &str) -> Unresolved {
    let span = span_of(at);
    Unresolved {
        reason: reason.into(),
        expression: span.and_then(|span| span.slice(source)).map(str::to_owned),
        span,
    }
}

impl Evaluation {
    /// A value nothing was undecided about.
    pub(crate) fn known(value: Value) -> Self {
        Self { value, unresolved: Vec::new() }
    }

    /// Undecided, for `reason`.
    pub(crate) fn unknown(reason: impl Into<String>, at: &Expr, source: &str) -> Self {
        Self { value: Value::Unknown, unresolved: vec![unresolved_at(reason, at, source)] }
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
    /// What the spec computes rather than stores, by `inspect_model::derived_key`.
    ///
    /// Empty by default, which is the honest default: an environment nobody
    /// gave the definitions to answers "nothing set it" rather than inventing
    /// a value, the same as before any of this existed.
    pub derived: &'a BTreeMap<String, Expr>,
    /// The computed fields being computed right now.
    ///
    /// `is_stranded: active_devices.count = 0` reaches `active_devices`, which
    /// reaches `devices`, and a spec is free to write a definition that comes
    /// back to itself. Without this that is a stack overflow — a crash rather
    /// than an answer, over a specification that is merely wrong.
    resolving: BTreeSet<String>,
}

/// The empty set of definitions, for an environment given none.
static NOTHING_DERIVED: LazyLock<BTreeMap<String, Expr>> = LazyLock::new(BTreeMap::new);

impl<'a> Env<'a> {
    /// An environment over `world` with nothing bound.
    #[must_use]
    pub fn new(world: &'a World, module: &'a str, source: &'a str) -> Self {
        Self {
            world,
            module,
            bindings: BTreeMap::new(),
            source,
            derived: &NOTHING_DERIVED,
            resolving: BTreeSet::new(),
        }
    }

    /// The same environment, able to compute what the spec computes.
    #[must_use]
    pub fn deriving(mut self, derived: &'a BTreeMap<String, Expr>) -> Self {
        self.derived = derived;
        self
    }

    /// How `field` on `instance` is computed, when it is and we are not already
    /// in the middle of computing it.
    fn derivation(&self, instance: &Instance, field: &str) -> Option<(&'a Expr, String)> {
        let key = inspect_model::derived_key(&instance.module, &instance.entity, field);
        if self.resolving.contains(&key) {
            return None;
        }
        self.derived.get(key.as_str()).map(|expr| (expr, key))
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
        Env {
            world: self.world,
            module: self.module,
            bindings,
            source: self.source,
            derived: self.derived,
            resolving: self.resolving.clone(),
        }
    }

    /// The scope a computed field is computed in.
    ///
    /// `this` is the instance and its stored fields are visible bare, which is
    /// what lets `devices where status = active` mean what it reads as. The key
    /// is remembered so a definition that comes back to itself stops.
    fn computing(&self, instance: &Instance, key: String) -> Env<'_> {
        let mut scope = self.scoped("this", Value::Ref(instance.id.clone()));
        for (field, value) in &instance.fields {
            scope.bindings.insert(field.clone(), value.clone());
        }
        scope.resolving.insert(key);
        scope
    }
}

/// Evaluate `expr` against `env`.
#[must_use]
pub fn eval(expr: &Expr, env: &Env<'_>) -> Evaluation {
    match expr {
        Expr::Ident(name) => ident(name, env),
        Expr::QualifiedName(name) => qualified(name, env),
        Expr::MemberAccess { object, field, .. } | Expr::OptionalAccess { object, field, .. } => {
            member(expr, object, field, env)
        }
        Expr::Comparison { left, op, right, .. } => compare(left, *op, right, env),
        Expr::LogicalOp { left, op, right, .. } => logical(left, *op, right, env),
        Expr::BinaryOp { left, op, right, .. } => {
            // `exists membership/Membership{…}` arrives here as a division. See
            // `collections::misparsed_qualified_lookup` for why, and for the
            // test that says when it stops arriving that way.
            match collections::misparsed_qualified_lookup(left, *op, right) {
                Some(lookup) => collections::existence(lookup, env, false),
                None => arithmetic(left, *op, right, env),
            }
        }
        Expr::Not { operand, .. } => {
            let operand = eval(operand, env);
            let truth = operand.truth().not();
            Evaluation { value: truth_value(truth), unresolved: operand.unresolved }
        }
        Expr::Exists { operand, .. } => existence(operand, env, false),
        Expr::NotExists { operand, .. } => existence(operand, env, true),
        Expr::NumberLiteral { value, .. } => literals::number(value, expr, env),
        Expr::StringLiteral(literal) => Evaluation::known(Value::Str(literal_text(literal))),
        // A backtick literal is a name with characters an identifier cannot
        // hold — `` `de-CH-1996` `` — and it means the state it spells.
        Expr::BacktickLiteral { value, .. } => Evaluation::known(Value::Enum(value.clone())),
        Expr::BoolLiteral { value, .. } => Evaluation::known(Value::Bool(*value)),
        Expr::DurationLiteral { value, .. } => literals::duration(value, expr, env),
        Expr::Null { .. } => Evaluation::known(Value::Null),
        Expr::Now { .. } => Evaluation::known(Value::Timestamp(env.world.now)),
        Expr::This { .. } => match env.bindings.get("this") {
            Some(value) => Evaluation::known(value.clone()),
            // `this` is bound by the entity context an invariant or a rule is
            // read in, and both `check_invariants` and `run_rule` reach here
            // without binding it. Wrapping the miss in `known` made it an
            // unknown with no reason — the one shape this crate's contract
            // names as indistinguishable from a bug.
            None => Evaluation::unknown("`this` is not bound here", expr, env.source),
        },
        Expr::SetLiteral { elements, .. } | Expr::ListLiteral { elements, .. } => {
            literals::set_literal(elements, env)
        }
        Expr::Where { source, condition, .. } => filtered(source, condition, env),
        Expr::With { source, predicate, .. } => filtered(source, predicate, env),
        Expr::Conditional { branches, else_body, .. } => {
            conditional(branches, else_body.as_deref(), env)
        }
        Expr::In { element, collection, .. } => membership(element, collection, env, false),
        Expr::NotIn { element, collection, .. } => membership(element, collection, env, true),
        Expr::For { binding, collection, filter, body, .. } => {
            quantified(binding, collection, filter.as_deref(), body, env)
        }
        // A block yields its last statement, which is what a multi-line
        // `requires` means; an empty one has nothing to yield.
        Expr::Block { items, .. } => match items.last() {
            Some(last) => eval(last, env),
            None => Evaluation::known(Value::Null),
        },
        // A binding in an expression position is its value: `entry: Outbox.x`
        // in a `when` clause is asked for the condition, not the name.
        Expr::Binding { value, .. } | Expr::LetExpr { value, .. } => eval(value, env),
        // Everything else is real Allium this evaluator does not model. Named
        // one by one rather than lumped together, because "a pipe was not
        // evaluated" is something a reader can act on and "unsupported" is not.
        Expr::Pipe { .. } => unsupported("a `|` alternation", expr, env),
        Expr::Lambda { .. } => unsupported("a lambda", expr, env),
        Expr::Call { .. } => unsupported("a function call", expr, env),
        Expr::JoinLookup { entity, fields, .. } => {
            collections::join_lookup(expr, entity, fields, env)
        }
        Expr::ObjectLiteral { .. } => unsupported("an object literal", expr, env),
        Expr::GenericType { .. } | Expr::TypeOptional { .. } => {
            unsupported("a type annotation", expr, env)
        }
        Expr::NullCoalesce { .. } => unsupported("a `??` coalesce", expr, env),
        Expr::ProjectionMap { .. } => unsupported("a projection", expr, env),
        Expr::TransitionsTo { .. } => unsupported("a `transitions_to` assertion", expr, env),
        Expr::Becomes { .. } => unsupported("a `becomes` assertion", expr, env),
        Expr::WhenGuard { .. } => unsupported("a `when` guard", expr, env),
        Expr::Within { .. } => unsupported("a `within` deadline", expr, env),
    }
}

/// Undecided because the language has a form this evaluator does not model.
///
/// Distinct from every other unknown here, which is about a *world* rather than
/// about this tool: "nothing is bound to `member`" is something a reader can
/// fix, and "a lambda is not simulated" is something only this crate can.
fn unsupported(what: &str, expr: &Expr, env: &Env<'_>) -> Evaluation {
    Evaluation::unknown(format!("{what} is not simulated"), expr, env.source)
}

/// One field of one instance, computed if the spec computes it rather than
/// storing it.
///
/// The one implementation, called from the expression evaluator and from the
/// journey walker both. It was two for about ten minutes, and in that time the
/// second one forgot to put the instance's stored fields in scope — so a rule
/// guarded by `not member.is_at_limit` refused while the assertion about
/// `ada.is_at_limit` on the line above it said `false`. The tool contradicting
/// itself about one field of one instance in one step is worse than either
/// answer alone.
#[must_use]
pub fn field_of(instance: &Instance, field: &str, env: &Env<'_>) -> Evaluation {
    // Stored wins. A `stipulate` writes to the instance, and a journey that
    // says a value outright must not be overruled by a definition — that is
    // the whole point of saying it.
    if instance.fields.contains_key(field) {
        return Evaluation::known(instance.field(field));
    }
    match env.derivation(instance, field) {
        Some((definition, key)) => eval(definition, &env.computing(instance, key)),
        None => Evaluation::known(instance.field(field)),
    }
}

/// A bare name: a binding, an entity type, or a state.
fn ident(ident: &Ident, env: &Env<'_>) -> Evaluation {
    let name = &ident.name;
    if let Some(bound) = env.bindings.get(name) {
        return Evaluation::known(bound.clone());
    }
    if env.world.count_of(name) > 0 {
        return Evaluation::known(collection_of(name, env));
    }
    // `for m in Members` names the collection, which is the entity pluralised.
    // Every invariant a real spec writes is quantified this way, so without
    // this they all range over nothing and hold vacuously — a checker that
    // always passes, which is worse than one that admits it cannot check.
    if let Some(entity) = singular(name)
        && env.world.count_of(&entity) > 0
    {
        return Evaluation::known(collection_of(&entity, env));
    }
    // A computed field of whatever `this` is, named bare. `is_stranded:
    // active_devices.count = 0` reads `active_devices` with nothing between,
    // and the definition it needs is one level further down the same chain.
    if let Some(Value::Ref(id)) = env.bindings.get("this")
        && let Some(instance) = env.world.instance(id)
        && let Some((definition, key)) = env.derivation(instance, name)
    {
        return eval(definition, &env.computing(instance, key));
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
            expression: Some(name.clone()),
            span: Some(Span::new(ident.span.start, ident.span.end)),
        }],
    }
}

/// `membership/Membership`: a type in another module.
fn qualified(name: &QualifiedName, env: &Env<'_>) -> Evaluation {
    Evaluation::known(collection_of(&name.name, env))
}

/// The entity a plural collection name refers to.
///
/// `Loans` -> `Loan`, `Copies` -> `Copy`. Only proposed: the caller uses it only
/// when the world holds instances of that name, so an over-eager reduction
/// costs nothing and `Staff` — already plural — falls through unchanged.
fn singular(collection: &str) -> Option<String> {
    if let Some(stem) = collection.strip_suffix("ies") {
        return Some(format!("{stem}y"));
    }
    collection.strip_suffix('s').filter(|stem| !stem.is_empty()).map(ToOwned::to_owned)
}

/// Every instance of `entity`, as references.
fn collection_of(entity: &str, env: &Env<'_>) -> Value {
    Value::Set(
        env.world.instances_of(entity).map(|instance| Value::Ref(instance.id.clone())).collect(),
    )
}

/// `object.field`, including `config.x` and `collection.count`.
fn member(whole: &Expr, object: &Expr, field: &Ident, env: &Env<'_>) -> Evaluation {
    let field = &field.name;

    // `config.loan_limit` is not a field of anything: `config` is a namespace.
    if is_ident_named(object, "config") {
        return Evaluation::known(env.world.config(env.module, field));
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
                format!("`{field}` is {}, which has no count", base.value.described()),
                whole,
                env.source,
            )
            .carrying(unresolved),
        };
    }

    match &base.value {
        Value::Ref(id) => match env.world.instance(id) {
            Some(instance) => {
                let read = field_of(instance, field, env);
                unresolved.extend(read.unresolved);
                // A field nobody set and nobody computes is undecided, and
                // saying *which* field on which instance is the difference
                // between a panel a reader can act on and one that says only
                // "unknown".
                if read.value.is_unknown() && !instance.fields.contains_key(field) {
                    unresolved.push(Unresolved {
                        reason: format!("`{id}` has no `{field}` set"),
                        expression: span_of(whole)
                            .and_then(|span| span.slice(env.source))
                            .map(str::to_owned),
                        span: span_of(whole),
                    });
                }
                Evaluation { value: read.value, unresolved }
            }
            None => Evaluation::unknown(format!("`{id}` is not in this world"), whole, env.source)
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
                        .map_or(Value::Unknown, |instance| instance.field(field)),
                    _ => Value::Unknown,
                })
                .collect();
            Evaluation { value: Value::Set(projected), unresolved }
        }
        Value::Unknown => Evaluation { value: Value::Unknown, unresolved },
        other => Evaluation::unknown(
            format!("`{field}` was read from {}, which has no fields", other.described()),
            whole,
            env.source,
        )
        .carrying(unresolved),
    }
}

use collections::{existence, filtered, membership, quantified};
use ops::{arithmetic, compare, logical};

/// `if condition: a else if condition: b else: c`.
///
/// A list of branches, which is what the language has. The JSON this used to
/// read had no such structure and the code guessed between `then`/`consequent`
/// and `otherwise`/`alternative` — two pairs of key names, at most one of which
/// was ever right.
fn conditional(branches: &[CondBranch], otherwise: Option<&Expr>, env: &Env<'_>) -> Evaluation {
    let mut unresolved = Vec::new();
    for branch in branches {
        let verdict = eval(&branch.condition, env);
        unresolved.extend(verdict.unresolved);
        match verdict.value.truth() {
            Truth::True => return eval(&branch.body, env).carrying(unresolved),
            Truth::False => {}
            // Undecided: which branch runs is not known, so neither is the
            // result. Reading on to the next condition would be answering a
            // question that was never reached.
            //
            // A condition that came back undecided has already said why. One
            // that came back as a known non-boolean has not, and that is the
            // gap: `apply.rs` reports exactly this case through `skipped()`,
            // and this arm used to return the unknown bare.
            Truth::Unknown => {
                if unresolved.is_empty() {
                    unresolved.push(unresolved_at(
                        format!("{} is not a condition", verdict.value.described()),
                        &branch.condition,
                        env.source,
                    ));
                }
                return Evaluation { value: Value::Unknown, unresolved };
            }
        }
    }
    match otherwise {
        Some(body) => eval(body, env).carrying(unresolved),
        // A conditional with no else branch and every condition false yields
        // nothing, which is what the language means.
        None => Evaluation { value: Value::Null, unresolved },
    }
}
