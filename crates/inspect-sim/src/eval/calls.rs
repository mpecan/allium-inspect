//! Calls: the ones a journey stipulated, and the ones the spec defines.
//!
//! Two kinds of function appear in a specification and they are answered from
//! opposite ends. `may_invite(group, issuer)` is named and never defined — its
//! policy is still being decided — so no simulator can work it out, and only a
//! journey saying what it comes back as answers it. `is_used_by(who): who in
//! filed_on_by` is defined on the entity it is called on, and is computed the
//! way a derived field is, with its parameters read as its arguments.

use allium_parser::ast::{CallArg, Expr};

use super::{Env, Evaluation, eval, unsupported};
use inspect_model::Function;

use crate::value::Value;

/// A call, answered by a stipulation or by the definition it names.
///
/// Stipulation first, for a bare call: a journey that says what a function
/// comes back as has said so on purpose, the same as a stipulated field
/// overrules its definition.
///
/// Matched on argument *values*: the rule writes `may_invite(group, issuer)`
/// and the journey writes `may_invite(chat, she)`, which are the same call
/// about the same two things. An argument nothing settled matches nothing,
/// because a stipulation about a value nobody knows is not about anything.
pub(super) fn answered(
    whole: &Expr,
    function: &Expr,
    args: &[CallArg],
    env: &Env<'_>,
) -> Evaluation {
    let mut given = Vec::with_capacity(args.len());
    let mut unresolved = Vec::new();
    for argument in args {
        let CallArg::Positional(value) = argument else {
            return unsupported("a function call with named arguments", whole, env);
        };
        let evaluated = eval(value, env);
        unresolved.extend(evaluated.unresolved);
        given.push(evaluated.value);
    }

    match function {
        // `may_invite(group, issuer)`, or `is_used_by(m)` inside the entity
        // that defines it — where `this` is the instance it is asked of.
        Expr::Ident(called) => {
            if let Some(answer) = env.world.answer(&called.name, &given) {
                return Evaluation { value: answer.clone(), unresolved };
            }
            match env.bindings.get("this") {
                Some(Value::Ref(id)) => defined(whole, id, &called.name, given, env),
                _ => unsupported("a function call", whole, env),
            }
            .carrying(unresolved)
        }
        // `hub.is_used_by(member)`: the function of whatever `hub` is.
        Expr::MemberAccess { object, field, .. } => {
            let on = eval(object, env);
            unresolved.extend(on.unresolved);
            match on.value {
                Value::Ref(id) => defined(whole, &id, &field.name, given, env),
                // Undecided already, and it said why.
                Value::Unknown => Evaluation { value: Value::Unknown, unresolved: Vec::new() },
                other => Evaluation::unknown(
                    format!(
                        "`{}` was called on {}, which defines nothing",
                        field.name,
                        other.described()
                    ),
                    whole,
                    env.source,
                ),
            }
            .carrying(unresolved)
        }
        _ => unsupported("a function call", whole, env).carrying(unresolved),
    }
}

/// `name(given…)` computed from its definition on the instance `id`.
///
/// Each parameter is bound to its argument, in the scope a derived field of
/// that instance is computed in: `this` is the instance and its stored fields
/// are visible bare, which is what lets `who in filed_on_by` mean *this* hub's
/// list.
fn defined(
    whole: &Expr,
    id: &crate::value::EntityId,
    name: &str,
    given: Vec<Value>,
    env: &Env<'_>,
) -> Evaluation {
    let Some(instance) = env.world.instance(id) else {
        return Evaluation::unknown(format!("`{id}` is not in this world"), whole, env.source);
    };
    let Some((function, key)) = env.function(instance, name) else {
        // Neither stipulated nor defined, or defined in terms of itself and
        // already being computed — either way nothing here can answer it.
        return unsupported("a function call", whole, env);
    };
    if given.len() != function.params.len() {
        return Evaluation::unknown(
            format!(
                "`{name}` takes {} argument{}, and was given {}",
                function.params.len(),
                if function.params.len() == 1 { "" } else { "s" },
                given.len()
            ),
            whole,
            env.source,
        );
    }

    let mut scope = env.computing(instance, key);
    for (param, value) in function.params.iter().zip(given) {
        scope.bindings.insert(param.clone(), value);
    }
    eval(&function.body, &scope)
}

/// A function named where a value was wanted: `hub.is_used_by`, with no call.
///
/// Evaluating its body would read its parameters as names nothing bound, which
/// is true and useless; what is true and useful is that the spec asks for an
/// argument here and none was given.
pub(super) fn read_bare(name: &str, function: &Function, env: &Env<'_>) -> Evaluation {
    Evaluation::unknown(
        format!("`{name}` takes an argument, and was read without one"),
        &function.body,
        env.source,
    )
}
