//! `object.field`: a read of one field, and where its note is anchored.
//!
//! Split out of the evaluator's root when it reached the file-size limit; the
//! judgements are unchanged and documented beside each arm.

use allium_parser::ast::{Expr, Ident};

use super::{Env, Evaluation, Unresolved, ast::is_ident_named, eval, field_of, told, unset_field};
use crate::value::Value;

/// `object.field`, including `config.x` and `collection.count`.
pub(super) fn member(whole: &Expr, object: &Expr, field: &Ident, env: &Env<'_>) -> Evaluation {
    let field = &field.name;

    // `config.loan_limit` is not a field of anything: `config` is a namespace.
    if is_ident_named(object, "config") {
        // A parameter with no default this simulator can read, and that
        // nobody set, is undecided — and says which parameter. Bare, it read
        // as "something could not be evaluated" at the rule it guarded. One
        // that *was* set, to something itself undecided, says that instead:
        // it has a default and somebody wrote over it, so the first reason
        // would be false on both counts.
        return match env.world.config(env.module, field) {
            Value::Unknown => Evaluation::unknown(
                env.world.why_config_is_undecided(env.module, field, &format!("config.{field}")),
                whole,
                env.source,
            ),
            value => Evaluation::known(value),
        };
    }

    let base = eval(object, env);
    let mut unresolved = base.unresolved.clone();

    if field == "count" {
        // A collection that came back with reasons may be short of members it
        // could not decide on — `parcels: Parcel with crate = this` leaves out
        // a parcel whose `crate` nobody set — so its count is a lower bound,
        // not a count. Reported as one, `parcels.count = 0` would hold over a
        // parcel that may well be in the crate. See
        // `lambdas::undecided_by_omission`, the same judgement for `all`.
        if matches!(base.value, Value::Set(_)) && !unresolved.is_empty() {
            return Evaluation::unknown(
                "this collection may be missing members it could not decide on, so its \
                 `count` is not known",
                object,
                env.source,
            )
            .carrying(unresolved);
        }
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
                // One *stored* undecided — two rules woken by one trigger
                // disagreed about it, or a rule wrote something it could not
                // settle — comes back saying so (`field_of`), and the note is
                // anchored here: a `requires` reading `box.out` names
                // `box.out`, not a sub-expression of the rule that wrote it.
                if instance.fields.contains_key(field) {
                    unresolved.extend(
                        read.unresolved
                            .into_iter()
                            .map(|note| told::anchored_at(note, whole, env.source)),
                    );
                    return Evaluation { value: read.value, unresolved };
                }
                // A field nobody set and nobody computes is undecided, and
                // saying *which* field on which instance is the difference
                // between a panel a reader can act on and one that says only
                // "unknown".
                unresolved.extend(read.unresolved);
                if read.value.is_unknown() {
                    unresolved.push(told::anchored_at(
                        Unresolved { reason: unset_field(id, field), expression: None, span: None },
                        whole,
                        env.source,
                    ));
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
        // Undecided already, and usually saying why. When it said nothing —
        // a name bound to an argument an emission could not settle — the read
        // says it, or a chained rule reports only "something could not be
        // evaluated" (issue #6), which is indistinguishable from a bug.
        Value::Unknown if unresolved.is_empty() => Evaluation::unknown(
            format!("`{field}` was read from something nothing settled"),
            object,
            env.source,
        ),
        Value::Unknown => Evaluation { value: Value::Unknown, unresolved },
        other => Evaluation::unknown(
            format!("`{field}` was read from {}, which has no fields", other.described()),
            whole,
            env.source,
        )
        .carrying(unresolved),
    }
}
