//! What a spec asks of a *set* rather than of one value.
//!
//! `exists`, `where`, `for … in` and `in` share one problem: the thing being
//! ranged over may not be a collection at all, and when it is not the answer is
//! undecided rather than empty. They are together because that is the decision
//! they have in common, and out of the dispatcher because the dispatcher should
//! read as a table of what the language has.

use std::ops::Not;

use serde_json::Value as Json;

use super::{
    Env, Evaluation, Unresolved,
    ast::{name_of, tagged, truth_value},
    eval, operand_of, span_of,
};
use crate::{truth::Truth, value::Value};

/// `exists X` and `not exists X`.
pub(super) fn existence(inner: &Json, env: &Env<'_>, negated: bool) -> Evaluation {
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
pub(super) fn filtered(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(source_node) = inner.get("source") else {
        return Evaluation::unknown("a filter with nothing to filter", inner, env.source);
    };
    let base = eval(source_node, env);
    let Some(condition) = inner.get("condition").or_else(|| inner.get("predicate")) else {
        return base;
    };
    let Value::Set(items) = &base.value else {
        return Evaluation::unknown(
            format!("{} cannot be filtered", base.value.described()),
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

/// `for x in Collection: condition` — universal quantification.
///
/// This is what every invariant in a real spec is made of. Read as *all*
/// elements satisfy the body, so an empty collection is vacuously true: a spec
/// with no loans does not violate a rule about loans.
///
/// The same node appears in an `ensures` clause, where it means iteration
/// rather than a claim. Which it is depends on the context, not the syntax:
/// `apply` handles the postcondition case and this one handles the assertion.
pub(super) fn quantified(inner: &Json, env: &Env<'_>) -> Evaluation {
    let Some(collection) = inner.get("collection").or_else(|| inner.get("source")) else {
        return Evaluation::unknown("an iteration with nothing to iterate", inner, env.source);
    };
    let Some(body) = inner.get("body") else {
        return Evaluation::unknown("an iteration with no body", inner, env.source);
    };

    let name = inner
        .get("binding")
        .and_then(|binding| match tagged(binding) {
            // `{"Single": {"name": "m"}}` — the one-variable form, which is
            // every quantifier a real spec writes.
            Some((_, single)) => name_of(single),
            None => name_of(binding),
        })
        .unwrap_or_else(|| "it".to_owned());

    let over = eval(collection, env);
    let mut unresolved = over.unresolved;
    let Value::Set(items) = over.value else {
        return Evaluation {
            value: Value::Unknown,
            unresolved: {
                unresolved.push(Unresolved {
                    reason: format!(
                        "`{name}` ranges over {}, which has no elements",
                        over.value.described()
                    ),
                    expression: span_of(collection)
                        .and_then(|span| span.slice(env.source))
                        .map(str::to_owned),
                    span: span_of(inner),
                });
                unresolved
            },
        };
    };

    let mut verdicts = Vec::new();
    for item in &items {
        let mut scope = element_scope(env, item);
        scope.bindings.insert(name.clone(), item.clone());

        // `for l in Loans where status = open:` narrows what is claimed about.
        if let Some(filter) = inner.get("filter").filter(|filter| !filter.is_null()) {
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

/// `x in collection`.
pub(super) fn membership(inner: &Json, env: &Env<'_>) -> Evaluation {
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
        // Anything else — a scalar, or a collection nobody could resolve — is a
        // membership test with no collection to test against.
        _ => Truth::Unknown,
    };
    Evaluation { value: truth_value(truth), unresolved }
}
