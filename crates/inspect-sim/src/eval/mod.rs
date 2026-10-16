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
mod calls;
mod collections;
mod lambdas;
mod literals;
mod member;
mod ops;
mod told;
mod words;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use allium_parser::ast::{CondBranch, Expr, Ident, QualifiedName};
// Re-exported for `apply`, which walks the same tree to decide what a
// postcondition changes.
pub use ast::{bare_name, span_of};
use ast::{literal_text, truth_value};
pub use calls::answers_only;
pub(crate) use collections::element_scope;
use inspect_model::{Derivations, Function, Span};
use serde::{Deserialize, Serialize};
use std::ops::Not;
pub(crate) use told::remember;
pub use words::unset_field;

use ts_rs::TS;

use crate::{
    truth::Truth,
    value::{EntityId, Instance, Value},
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

/// Why each of some bound names holds an unknown: see [`Env::told`].
pub type Told = BTreeMap<String, Vec<Unresolved>>;

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
    /// Why a name in `bindings` holds an unknown, when whatever bound it said.
    ///
    /// Beside the binding rather than in it, because a binding is a bare
    /// [`Value`]; without this a name bound to something undecided reached its
    /// reader saying nothing (issue #27). See the `told` module.
    pub told: Told,
    /// The module's spec text, so an undecided note can quote the source.
    pub source: &'a str,
    /// Every module's spec text, by module name.
    ///
    /// What a definition is quoted from when it is computed: `quiet:
    /// watchers(this).count = 0` is written in the module that declares the
    /// entity, and its spans are offsets into *that* file, not into the one
    /// whose rule happened to read it. Empty by default, and then a definition
    /// from another module is quoted as nothing rather than as a slice of the
    /// wrong file.
    sources: &'a BTreeMap<String, String>,
    /// What the spec computes rather than stores, by `inspect_model::derived_key`.
    ///
    /// Empty by default, which is the honest default: an environment nobody
    /// gave the definitions to answers "nothing set it" rather than inventing
    /// a value, the same as before any of this existed.
    pub derived: &'a Derivations,
    /// The computed fields being computed right now.
    ///
    /// `is_stranded: active_devices.count = 0` reaches `active_devices`, which
    /// reaches `devices`, and a spec is free to write a definition that comes
    /// back to itself. Without this that is a stack overflow — a crash rather
    /// than an answer, over a specification that is merely wrong.
    resolving: BTreeSet<String>,
    /// The parameters of the rule being evaluated, whether passed or not.
    ///
    /// A parameter nobody passed is not in `bindings`, and reads exactly like a
    /// word nothing binds. Only a call's spelling match asks the difference —
    /// see `calls::answered_by_spelling` — and there it is the whole question.
    parameters: BTreeSet<String>,
    /// The element a `where` or `with` filter is looking at, when there is one.
    ///
    /// Inside a definition `this` stays the instance being defined, so a bare
    /// name in `docs where status = active` is a field of the *element*, not of
    /// `this` — and when that element never set it, it is the element that has
    /// to say so. Read off `this` alone, an unset `status` passed for a word,
    /// and `status = active` came out a definite `false`.
    element: Option<EntityId>,
    /// The elements of the filters this one is inside, outermost first.
    ///
    /// In `docs where (tags where name = draft).count > 0` an unset field of
    /// the outer doc is still a field of something in scope, not a word: read
    /// off the innermost element alone, it passed for the state it spells.
    enclosing: Vec<EntityId>,
}

/// The empty set of definitions, for an environment given none.
static NOTHING_DERIVED: LazyLock<Derivations> = LazyLock::new(Derivations::default);

/// No module's text, for an environment given none.
static NO_SOURCES: BTreeMap<String, String> = BTreeMap::new();

impl<'a> Env<'a> {
    /// An environment over `world` with nothing bound.
    #[must_use]
    pub fn new(world: &'a World, module: &'a str, source: &'a str) -> Self {
        Self {
            world,
            module,
            bindings: BTreeMap::new(),
            told: BTreeMap::new(),
            source,
            sources: &NO_SOURCES,
            derived: &NOTHING_DERIVED,
            resolving: BTreeSet::new(),
            parameters: BTreeSet::new(),
            element: None,
            enclosing: Vec::new(),
        }
    }

    /// The same environment, knowing the rule declares `names` as parameters.
    #[must_use]
    pub fn declaring(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.parameters.extend(names);
        self
    }

    /// The same environment, able to compute what the spec computes.
    #[must_use]
    pub fn deriving(mut self, derived: &'a Derivations) -> Self {
        self.derived = derived;
        self
    }

    /// The same environment, able to quote every module's text, so a
    /// definition another module wrote is quoted from that module's file.
    #[must_use]
    pub fn quoting(mut self, sources: &'a BTreeMap<String, String>) -> Self {
        self.sources = sources;
        self
    }

    /// How `field` on `instance` is computed, when it is and we are not already
    /// in the middle of computing it.
    fn derivation(&self, instance: &Instance, field: &str) -> Option<(&'a Expr, String)> {
        let key = inspect_model::derived_key(&instance.module, &instance.entity, field);
        if self.resolving.contains(&key) {
            return None;
        }
        self.derived.fields.get(key.as_str()).map(|expr| (expr, key))
    }

    /// The function `name` defined on `instance`, under the same guard.
    fn function(&self, instance: &Instance, name: &str) -> Option<(&'a Function, String)> {
        let key = inspect_model::derived_key(&instance.module, &instance.entity, name);
        if self.resolving.contains(&key) {
            return None;
        }
        self.derived.functions.get(key.as_str()).map(|function| (function, key))
    }

    /// The same environment with `name` bound.
    #[must_use]
    pub fn bind(mut self, name: impl Into<String>, value: Value) -> Self {
        self.set(name, value);
        self
    }

    /// A child scope with `name` bound, for an iteration or a filter.
    fn scoped(&self, name: &str, value: Value) -> Env<'_> {
        let mut bindings = self.bindings.clone();
        bindings.insert(name.to_owned(), value);
        let mut told = self.told.clone();
        told.remove(name);
        Env {
            world: self.world,
            module: self.module,
            bindings,
            told,
            source: self.source,
            sources: self.sources,
            derived: self.derived,
            resolving: self.resolving.clone(),
            parameters: self.parameters.clone(),
            element: self.element.clone(),
            enclosing: self.enclosing.clone(),
        }
    }

    /// The scope a computed field is computed in.
    ///
    /// `this` is the instance and its stored fields are visible bare, which is
    /// what lets `devices where status = active` mean what it reads as. The key
    /// is remembered so a definition that comes back to itself stops.
    ///
    /// **Nothing else** is in scope: not the rule's parameters, not a loop
    /// variable, not whatever the caller happened to bind. A definition belongs
    /// to its entity and means the same wherever it is read from. Inheriting
    /// the caller's names let `items: Item with box = this`, read from a rule
    /// whose parameter is also called `box`, take that parameter for an item's
    /// unset `box` — so an item nobody placed was counted in the box, and the
    /// verdict turned on what the rule's parameter was called.
    ///
    /// For the same reason it is read **in the module that wrote it**, not
    /// the one reading it: `quiet: watchers(this).count = 0` declared in `a`
    /// asks `a`'s `watchers`, so `where a/watchers(tag)` answers it and `where
    /// b/watchers(tag)` does not, whichever module's rule reads `tag.quiet`
    /// (issue #32). Its `config` is `a`'s, and its undecided notes quote `a`'s
    /// text, at offsets that are `a`'s.
    fn computing<'s>(&'s self, instance: &'s Instance, key: String) -> Env<'s> {
        let mut bindings = BTreeMap::new();
        bindings.insert("this".to_owned(), Value::Ref(instance.id.clone()));
        let mut resolving = self.resolving.clone();
        resolving.insert(key);
        let module = instance.module.as_str();
        let source = if module == self.module {
            self.source
        } else {
            self.sources.get(module).map_or("", String::as_str)
        };
        let mut scope = Env {
            world: self.world,
            module,
            bindings,
            told: BTreeMap::new(),
            source,
            sources: self.sources,
            derived: self.derived,
            resolving,
            // A definition belongs to its entity, not to the rule reading it.
            parameters: BTreeSet::new(),
            element: None,
            enclosing: Vec::new(),
        };
        // Bare, and each undecided one saying why: `shown: out + "!"` over an
        // `out` a rule could not settle names what it could not settle.
        scope.bind_stored(instance);
        scope
    }
}

/// The name an [`Env`] binds the context an actor is identified within under.
///
/// `within` is a keyword, so no spec can declare a name that collides with it.
pub const WITHIN: &str = "within";

/// The reason given for a bare name nothing binds: "nothing is bound to `x`".
///
/// One spelling, shared, because a reader of the reason — the journey walk
/// turning it into "`bob` has no `is_admin` set" — has to recognise it, and a
/// copy of the wording there would drift from this one without a sound.
#[must_use]
pub fn nothing_bound(name: &str) -> String {
    format!("nothing is bound to `{name}`")
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
        Expr::SetLiteral { elements, .. } => literals::set_literal(elements, false, env),
        Expr::ListLiteral { elements, .. } => literals::set_literal(elements, true, env),
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
        // `items.all(i => …)` and `.any` are caught as calls; see `lambdas`.
        Expr::Lambda { .. } => unsupported("a lambda", expr, env),
        Expr::Call { function, args, .. } => calls::answered(expr, function, args, env),
        Expr::JoinLookup { entity, fields, .. } => {
            collections::join_lookup(expr, entity, fields, env)
        }
        Expr::ObjectLiteral { .. } => unsupported("an object literal", expr, env),
        Expr::GenericType { .. } | Expr::TypeOptional { .. } => {
            unsupported("a type annotation", expr, env)
        }
        Expr::NullCoalesce { left, right, .. } => coalesce(left, right, env),
        Expr::ProjectionMap { .. } => unsupported("a projection", expr, env),
        Expr::TransitionsTo { .. } => unsupported("a `transitions_to` assertion", expr, env),
        Expr::Becomes { .. } => unsupported("a `becomes` assertion", expr, env),
        Expr::WhenGuard { .. } => unsupported("a `when` guard", expr, env),
        // `identified_by: Seat where workspace = within` — the context an
        // actor is identified within. Only whoever reads an actor's condition
        // knows which instance that is, and it binds it; anywhere else it is
        // a name nothing bound, said the way every other one is.
        Expr::Within { .. } => match env.bindings.get(WITHIN) {
            Some(value) => Evaluation::known(value.clone()),
            None => Evaluation::unknown(nothing_bound(WITHIN), expr, env.source),
        },
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
    //
    // A value stored undecided says so, with what its write could not settle
    // (issue #27) — unanchored, because only the reader knows where it read.
    if instance.fields.contains_key(field) {
        let value = instance.field(field);
        let unresolved = if told::undecided(instance, field, &value) {
            vec![told::stored_unknown(instance, field)]
        } else {
            Vec::new()
        };
        return Evaluation { value, unresolved };
    }
    match env.derivation(instance, field) {
        Some((definition, key)) => eval(definition, &env.computing(instance, key)),
        None => match env.function(instance, field) {
            Some((function, _)) => calls::read_bare(field, function, env),
            None => Evaluation::known(instance.field(field)),
        },
    }
}

/// A bare name: a binding, an entity type, or a state.
fn ident(ident: &Ident, env: &Env<'_>) -> Evaluation {
    let name = &ident.name;
    if let Some(bound) = env.bindings.get(name) {
        let span = Span::new(ident.span.start, ident.span.end);
        let unresolved = if bound.holds_unknown() { env.told_of(name, span) } else { Vec::new() };
        return Evaluation { value: bound.clone(), unresolved };
    }
    if let Some(collection) = collection_named(name, env) {
        return Evaluation::known(collection);
    }
    // A field of the element a filter is looking at, which it never set.
    if let Some(read) = words::unset_element_field(ident, env) {
        return read;
    }
    // Or one it computes, in its own scope (issue #23).
    if let Some(read) = words::computed_element_field(ident, env) {
        return read;
    }
    // A computed field of whatever `this` is, named bare. `is_stranded:
    // active_devices.count = 0` reads `active_devices` with nothing between,
    // and the definition it needs is one level further down the same chain.
    if let Some(Value::Ref(id)) = env.bindings.get("this")
        && let Some(instance) = env.world.instance(id)
    {
        if let Some((definition, key)) = env.derivation(instance, name) {
            return eval(definition, &env.computing(instance, key));
        }
        if let Some((function, _)) = env.function(instance, name) {
            return calls::read_bare(name, function, env);
        }
        if let Some(read) = words::own_field_or_state(instance, ident, env) {
            return read;
        }
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
            reason: nothing_bound(name),
            expression: Some(name.clone()),
            span: Some(Span::new(ident.span.start, ident.span.end)),
        }],
    }
}

/// `membership/Membership`: a type in another module.
///
/// And `membership/Memberships`, its collection, read the way the bare plural
/// is. Only the bare one was: `for m in membership/Memberships` ranged over
/// instances of an entity called `Memberships`, of which there are none, so a
/// surface reached across a module boundary showed nobody anything.
fn qualified(name: &QualifiedName, env: &Env<'_>) -> Evaluation {
    Evaluation::known(collection_named(&name.name, env).unwrap_or(Value::Set(Vec::new())))
}

/// The instances a type name means, when the world holds any: `Loan`, or
/// `Loans`, its collection.
///
/// `for m in Members` names the collection, which is the entity pluralised.
/// Every invariant a real spec writes is quantified this way, so without this
/// they all range over nothing and hold vacuously — a checker that always
/// passes, which is worse than one that admits it cannot check. One reading for
/// a bare name and a qualified one, so the two cannot disagree about a plural.
fn collection_named(name: &str, env: &Env<'_>) -> Option<Value> {
    if env.world.count_of(name) > 0 {
        return Some(collection_of(name, env));
    }
    singular(name)
        .into_iter()
        .find(|entity| env.world.count_of(entity) > 0)
        .map(|entity| collection_of(&entity, env))
}

/// The entities a plural collection name may refer to, most likely first.
///
/// `Loans` -> `Loan`, `Copies` -> `Copy`, `Watches` -> `Watch`. Only proposed:
/// the caller takes the first the world holds instances of, so an over-eager
/// reduction costs nothing (`Houses` tries `House` before `Hous`) and `Staff`
/// — already plural — falls through unchanged.
fn singular(collection: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(stem) = collection.strip_suffix("ies") {
        candidates.push(format!("{stem}y"));
    }
    for suffix in ["s", "es"] {
        if let Some(stem) = collection.strip_suffix(suffix).filter(|stem| !stem.is_empty()) {
            candidates.push(stem.to_owned());
        }
    }
    candidates
}

/// Every instance of `entity`, as references.
fn collection_of(entity: &str, env: &Env<'_>) -> Value {
    Value::Set(
        env.world.instances_of(entity).map(|instance| Value::Ref(instance.id.clone())).collect(),
    )
}

use collections::{existence, filtered, membership, quantified};
use member::member;
use ops::{arithmetic, coalesce, compare, logical};

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
            // gap: `apply` reports exactly this case through `skipped()`,
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
