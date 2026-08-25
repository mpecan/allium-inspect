//! What a spec asks of a *set* rather than of one value.
//!
//! `exists`, `where`, `for … in` and `in` share one problem: the thing being
//! ranged over may not be a collection at all, and when it is not the answer is
//! undecided rather than empty. They are together because that is the decision
//! they have in common, and out of the dispatcher because the dispatcher should
//! read as a table of what the language has.
//!
//! Each of these used to begin by looking for its own operands under two or
//! three possible key names — `collection` or `source`, `value` or `left` —
//! because the JSON gave no way to know which the parser used. Typed, the
//! operands are fields and the guessing is gone with them.

use std::ops::Not;

use allium_parser::ast::{BinaryOp, Expr, ForBinding, JoinField};

use super::{
    Env, Evaluation, Unresolved, ast::truth_value, bare_name, eval, span_of, unresolved_at,
};
use crate::{truth::Truth, value::Value};

/// `exists X` and `not exists X`.
pub(super) fn existence(operand: &Expr, env: &Env<'_>, negated: bool) -> Evaluation {
    let evaluated = eval(operand, env);
    let present = match &evaluated.value {
        Value::Set(items) => Truth::from_bool(!items.is_empty()),
        Value::Null => Truth::False,
        Value::Unknown => Truth::Unknown,
        _ => Truth::True,
    };
    let truth = if negated { present.not() } else { present };
    Evaluation { value: truth_value(truth), unresolved: evaluated.unresolved }
}

/// What one field of a join lookup is being matched against.
///
/// The two are not interchangeable, and the difference is the whole of why a
/// bare name is kept as one. `Receipt{message: m, reporter: r, kind: read}` is
/// how a spec asks whether a read receipt is already recorded, and `read` is a
/// state of the field it sits beside — not a binding, not a typo, and nothing
/// in this expression says which. Reading it as an unbound name made every
/// such lookup undecided, and `MarkRead` with it.
///
/// So the decision is deferred to the candidate: a name is read as a state
/// only against a field that *holds* one, which is the rule `compare` already
/// applies to `status = applied`. Against anything else it stays undecided,
/// because there it really is a name nothing bound.
enum Wanted<'a> {
    Settled(Value),
    Named(&'a str),
}

impl Wanted<'_> {
    /// Whether a candidate's field is what was asked for.
    fn against(&self, held: &Value) -> Truth {
        match self {
            Wanted::Settled(value) => Truth::from_bool(held == value),
            Wanted::Named(name) => match held {
                Value::Enum(state) => Truth::from_bool(state == name),
                _ => Truth::Unknown,
            },
        }
    }
}

/// `Membership{group: g, member: m}` — the instance whose fields all match.
///
/// A join lookup resolves to **one instance or null**, which is what makes
/// `exists Membership{group: g, member: m}` the ordinary way a spec asks
/// whether a relationship is already recorded. Without it every such rule was
/// undecided, and in a real spec set that is most of the interesting ones.
///
/// Three-valued, and that is the whole care in here. A candidate whose field is
/// `Unknown` has not been ruled *out* — nobody looked — so a lookup that finds
/// no definite match but had an undecided candidate answers undecided rather
/// than null. Treating unknown as "does not match" would report a membership
/// absent because nothing had checked, which is exactly the failure the third
/// truth value exists to prevent.
pub(super) fn join_lookup(
    whole: &Expr,
    entity: &Expr,
    fields: &[JoinField],
    env: &Env<'_>,
) -> Evaluation {
    let Some(name) = entity_named(entity) else {
        return Evaluation::unknown("a join lookup over something unnamed", whole, env.source);
    };

    // Every field first: one unknown among them decides the whole lookup, and
    // evaluating them per candidate would repeat the work and the reasons.
    let mut unresolved = Vec::new();
    let mut deferred = Vec::new();
    let mut wanted: Vec<(&str, Wanted)> = Vec::new();
    for field in fields {
        // `Membership{group}` with no value means a binding of the same name.
        let (evaluated, written) = match &field.value {
            Some(value) => (eval(value, env), bare_name(value)),
            None => {
                (eval_ident_named(&field.field.name, field, env), Some(field.field.name.as_str()))
            }
        };
        // A bare name nothing bound is held back rather than reported: what it
        // means depends on the field it is matched against, and the candidates
        // have not been looked at yet. Its reason is held back with it, and
        // put back the moment the candidates fail to answer the question —
        // "nothing is bound to `m`" is the sentence a reader can act on, and
        // losing it to reach a state reading would be a worse answer given
        // more confidently.
        match (evaluated.value, written) {
            (Value::Unknown, Some(name)) => {
                deferred.extend(evaluated.unresolved);
                wanted.push((&field.field.name, Wanted::Named(name)));
            }
            (value, _) => {
                unresolved.extend(evaluated.unresolved);
                wanted.push((&field.field.name, Wanted::Settled(value)));
            }
        }
    }

    if wanted.iter().any(|(_, wanted)| matches!(wanted, Wanted::Settled(Value::Unknown))) {
        return Evaluation::unknown(
            format!("`{name}` cannot be looked up on a value nothing settled"),
            whole,
            env.source,
        )
        .carrying(unresolved);
    }

    let mut undecided = false;
    for instance in env.world.instances_of(name) {
        let mut matches = Truth::True;
        for (field, value) in &wanted {
            match instance.fields.get(*field) {
                Some(Value::Unknown) | None => matches = matches.and(Truth::Unknown),
                Some(held) => matches = matches.and(value.against(held)),
            }
        }
        match matches {
            Truth::True => {
                return Evaluation { value: Value::Ref(instance.id.clone()), unresolved };
            }
            Truth::Unknown => undecided = true,
            Truth::False => {}
        }
    }

    if undecided {
        unresolved.extend(deferred);
        return Evaluation::unknown(
            format!("a `{name}` might match, and one of its fields is unknown"),
            whole,
            env.source,
        )
        .carrying(unresolved);
    }

    // Nothing matched, and nothing was left open — including any name that was
    // held back. A world with no `Receipt` in it has none whatever `read`
    // meant, so the deferred reasons are dropped rather than reported against
    // an answer they had no part in.
    Evaluation { value: Value::Null, unresolved }
}

/// A qualified join lookup, which the parser hands over as a division.
///
/// `exists membership/Membership{group: g}` should be an `Exists` over a
/// `JoinLookup` whose entity is a `QualifiedName`. `allium-parser` 3.5.3 reads
/// the `/` as division instead and produces
///
/// ```text
/// BinaryOp {
///     left:  Exists(Ident "membership"),
///     op:    Div,
///     right: JoinLookup { entity: Ident "Membership", … },
/// }
/// ```
///
/// which this crate then reported, faithfully and uselessly, as "nothing is
/// bound to `membership`" — sending a reader to look for a cast member that was
/// never missing. The same qualified name in *type* position parses correctly,
/// so it is this path alone.
///
/// Recovered rather than reported, because the shape is unambiguous: dividing
/// an `exists` — a boolean — by a set is not an expression any specification
/// means, and the source text at that span says exactly what was intended.
///
/// `tests/upstream.rs` asserts the misparse still happens, so the day it is
/// fixed this becomes dead code loudly rather than quietly.
pub(super) fn misparsed_qualified_lookup<'a>(
    left: &'a Expr,
    op: BinaryOp,
    right: &'a Expr,
) -> Option<&'a Expr> {
    if op != BinaryOp::Div || !matches!(right, Expr::JoinLookup { .. }) {
        return None;
    }
    let Expr::Exists { operand, .. } = left else { return None };
    // A module name, which is lower case in every spec the language admits.
    match operand.as_ref() {
        Expr::Ident(ident) if ident.name.starts_with(char::is_lowercase) => Some(right),
        _ => None,
    }
}

/// The entity a join lookup is over: `Membership`, or `membership/Membership`.
fn entity_named(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident(ident) => Some(&ident.name),
        // The module is dropped, the same as everywhere else here: a world
        // holds instances by entity name, and two modules declaring one name is
        // a collision the *spec* has rather than one this can resolve.
        Expr::QualifiedName(name) => Some(&name.name),
        _ => None,
    }
}

/// `Membership{group}` — the field's value is a binding of the same name.
fn eval_ident_named(name: &str, at: &JoinField, env: &Env<'_>) -> Evaluation {
    env.bindings.get(name).map_or_else(
        || {
            Evaluation::unknown(
                format!("nothing is bound to `{name}`"),
                // The field carries the only span there is: the value it would
                // have had is the thing that is not written.
                &Expr::Ident(at.field.clone()),
                env.source,
            )
        },
        |bound| Evaluation::known(bound.clone()),
    )
}

/// `collection where condition`, and `Entity with predicate`.
pub(super) fn filtered(source: &Expr, condition: &Expr, env: &Env<'_>) -> Evaluation {
    let base = eval(source, env);
    let Value::Set(items) = &base.value else {
        return Evaluation::unknown(
            format!("{} cannot be filtered", base.value.described()),
            source,
            env.source,
        )
        .carrying(base.unresolved);
    };

    let mut unresolved = base.unresolved.clone();
    let mut kept = Vec::new();
    for item in items {
        // The element's fields are in scope bare, so `where status = active`
        // reads them without naming it. See `element_scope` for what `this`
        // means in here, which is not the element when something outside
        // already holds it.
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

/// `for x in Collection: condition` — universal quantification.
///
/// This is what every invariant in a real spec is made of. Read as *all*
/// elements satisfy the body, so an empty collection is vacuously true: a spec
/// with no loans does not violate a rule about loans.
///
/// The same node appears in an `ensures` clause, where it means iteration
/// rather than a claim. Which it is depends on the context, not the syntax:
/// `apply` handles the postcondition case and this one handles the assertion.
pub(super) fn quantified(
    binding: &ForBinding,
    collection: &Expr,
    filter: Option<&Expr>,
    body: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    // The one-variable form is every quantifier a real spec writes. A
    // destructured binding ranges over the same elements; naming it after its
    // first part keeps the body's references working rather than binding
    // nothing at all.
    let name = match binding {
        ForBinding::Single(ident) => ident.name.clone(),
        ForBinding::Destructured(parts, _) => {
            parts.first().map_or_else(|| "it".to_owned(), |ident| ident.name.clone())
        }
    };

    let over = eval(collection, env);
    let mut unresolved = over.unresolved;
    let Value::Set(items) = over.value else {
        unresolved.push(Unresolved {
            reason: format!(
                "`{name}` ranges over {}, which has no elements",
                over.value.described()
            ),
            expression: span_of(collection)
                .and_then(|span| span.slice(env.source))
                .map(str::to_owned),
            span: span_of(collection),
        });
        return Evaluation { value: Value::Unknown, unresolved };
    };

    let mut verdicts = Vec::new();
    for item in &items {
        let mut scope = element_scope(env, item);
        scope.bindings.insert(name.clone(), item.clone());

        // `for l in Loans where status = open:` narrows what is claimed about.
        if let Some(filter) = filter {
            let keep = eval(filter, &scope);
            let holds = keep.truth();
            unresolved.extend(keep.unresolved);
            if holds != Truth::True {
                continue;
            }
        }

        let held = eval(body, &scope);
        let verdict = held.truth();
        unresolved.extend(held.unresolved);
        verdicts.push(verdict);
    }

    Evaluation { value: truth_value(Truth::all(verdicts)), unresolved }
}

/// A scope in which `item`'s own fields are visible without naming it.
///
/// `this` is bound to the element **only when nothing else already holds it**,
/// and that exception is the whole of a real reading rather than a defensive
/// one. Inside an entity, `this` means the instance:
///
/// ```text
/// entity Member {
///     loans: Loan with member = this
/// }
/// ```
///
/// means *the loans whose member is this member*. Rebinding `this` to each
/// candidate loan turns it into `member = <that same loan>`, which is false for
/// every loan there could ever be — so the relationship came back empty, the
/// count came back zero, and `is_at_limit` was confidently and quietly wrong.
///
/// Where there is no enclosing `this` — a filter inside an invariant, say —
/// the element is the only thing it could mean, and it still means that.
fn element_scope<'a>(env: &'a Env<'a>, item: &Value) -> Env<'a> {
    let mut scope = match env.bindings.get("this") {
        Some(enclosing) => env.scoped("this", enclosing.clone()),
        None => env.scoped("this", item.clone()),
    };
    if let Value::Ref(id) = item
        && let Some(instance) = env.world.instance(id)
    {
        for (field, value) in &instance.fields {
            scope.bindings.insert(field.clone(), value.clone());
        }
    }
    scope
}

/// `x in collection`, and `x not in collection`.
pub(super) fn membership(
    element: &Expr,
    collection: &Expr,
    env: &Env<'_>,
    negated: bool,
) -> Evaluation {
    let needle = eval(element, env);
    let haystack = eval(collection, env);
    let mut unresolved = needle.unresolved;
    unresolved.extend(haystack.unresolved);

    let inside = match &haystack.value {
        Value::Set(items) => Truth::any(items.iter().map(|item| item.equals(&needle.value))),
        // Anything else — a scalar, or a collection nobody could resolve — is a
        // membership test with no collection to test against. If the haystack
        // came back undecided it has already said why; if it came back as a
        // known scalar, nothing has, and an unknown with no reason is
        // indistinguishable from a bug. `filtered` says exactly this for the
        // same situation, a hundred lines up.
        other => {
            if unresolved.is_empty() {
                unresolved.push(unresolved_at(
                    format!("{} is not a collection to test membership of", other.described()),
                    collection,
                    env.source,
                ));
            }
            Truth::Unknown
        }
    };
    let truth = if negated { inside.not() } else { inside };
    Evaluation { value: truth_value(truth), unresolved }
}
