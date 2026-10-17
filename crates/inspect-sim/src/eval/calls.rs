//! Calls: the ones a journey stipulated, and the ones the spec defines.
//!
//! Two kinds of function appear in a specification and they are answered from
//! opposite ends. `may_invite(group, issuer)` is named and never defined — its
//! policy is still being decided — so no simulator can work it out, and only a
//! journey saying what it comes back as answers it. `is_used_by(who): who in
//! filed_on_by` is defined on the entity it is called on, and is computed the
//! way a derived field is, with its parameters read as its arguments.

use allium_parser::ast::{CallArg, Expr};

use super::{Env, Evaluation, ast::bare_name, eval, lambdas, unsupported};
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
    // `items.all(i => …)` and `items.any(i => …)` are quantifiers rather than
    // calls, and must be caught before the arguments are evaluated: a lambda
    // evaluated as a value is only ever "not simulated".
    if let Expr::MemberAccess { object, field, .. } = function
        && let Some(answer) = lambdas::quantified(whole, object, &field.name, args, env)
    {
        return answer;
    }

    let mut nodes = Vec::with_capacity(args.len());
    let mut evaluated = Vec::with_capacity(args.len());
    for argument in args {
        let CallArg::Positional(value) = argument else {
            return unsupported("a function call with named arguments", whole, env);
        };
        evaluated.push(eval(value, env));
        nodes.push(value);
    }
    let given: Vec<Value> = evaluated.iter().map(|argument| argument.value.clone()).collect();
    let mut unresolved: Vec<_> =
        evaluated.iter().flat_map(|argument| argument.unresolved.iter().cloned()).collect();

    match function {
        // `may_invite(group, issuer)`, or `is_used_by(m)` inside the entity
        // that defines it — where `this` is the instance it is asked of.
        Expr::Ident(called) => {
            if let Some(answer) = env.world.answer_in(env.module, &called.name, &given) {
                return told(&called.name, answer, unresolved, whole, env);
            }
            if let Some(answer) = answered_by_spelling(&called.name, whole, &nodes, &evaluated, env)
            {
                return answer;
            }
            // Inside a filter, the element's function before `this`'s: a bare
            // name there is the element's, as its fields are.
            let this = match env.bindings.get("this") {
                Some(Value::Ref(id)) => Some(id),
                _ => None,
            };
            let owner = [env.element.as_ref(), this]
                .into_iter()
                .flatten()
                .find(|id| defines(id, &called.name, env));
            match owner {
                Some(id) => defined(whole, id, &called.name, given, env),
                // Named and never defined: a black box. What would answer it
                // is a journey saying so, and that is the sentence worth
                // reading — "not simulated" sent a reader looking for a
                // missing feature when what was missing was a stipulation.
                _ => Evaluation::unknown(unanswered(&called.name, env), whole, env.source),
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

/// Why a black box came back unanswered.
///
/// A journey may have answered a call of the same name in another module:
/// `where b/watchers(tag)` read by `a`'s `watchers` answers nothing, and the
/// ledger lists it. Saying no more than "no stipulation answers this call"
/// beside that line reads as the tool contradicting itself (issue #32), so the
/// reason names which module asked and which the answer was about.
///
/// A call asked inside a definition another module wrote says whose it is
/// whether or not anything answered elsewhere yet: a qualified answer that
/// comes too late for it is otherwise read as one naming the wrong module.
fn unanswered(name: &str, env: &Env<'_>) -> String {
    let nothing = format!("nothing defines `{name}`, and no stipulation answers this call");
    let elsewhere = env.world.answered_elsewhere(env.module, name);
    if elsewhere.is_empty() {
        if env.module == env.asking {
            return nothing;
        }
        return format!("{nothing}; {}, read in its definition", asked_in(env.module));
    }
    let answers: Vec<String> = elsewhere.iter().map(|call| answers_only(call)).collect();
    format!("{nothing}; {}, and {}", answers.join("; "), asked_in(env.module))
}

/// "this one is `b`'s": whose an unanswered call is, in the words a reason
/// uses for it — see [`whose_call`], which reads it back.
#[must_use]
pub fn asked_in(module: &str) -> String {
    format!("this one is `{module}`'s")
}

/// The module a reason says its call is, when it says: the inverse of
/// [`asked_in`], so the journey walk never copies the wording.
#[must_use]
pub fn whose_call(reason: &str) -> Option<&str> {
    let (_, after) = reason.split_once("this one is `")?;
    let (module, _) = after.split_once("`'s")?;
    Some(module)
}

/// "`b/watchers` answers `b`'s `watchers`": what a qualified answer is about,
/// in the words a reason uses for it.
///
/// One spelling, shared, because the journey walk reads it back to tell a
/// call answered in the wrong module from one answered too late, and a copy
/// of the wording there would drift from this one without a sound.
#[must_use]
pub fn answers_only(call: &str) -> String {
    let (module, bare) = split_module(call);
    format!("`{call}` answers `{}`'s `{bare}`", module.unwrap_or_default())
}

/// The module a name is qualified by and the name in it: `tally/watchers` is
/// `tally`'s `watchers`, and `watchers` names no module. One reading of the
/// `module/name` convention — for a call, an answer and a type alike — so no
/// two places can split it differently.
#[must_use]
pub fn split_module(name: &str) -> (Option<&str>, &str) {
    name.rsplit_once('/').map_or((None, name), |(module, bare)| (Some(module), bare))
}

/// `can(repo, code)` answered by a stipulation that names the same word.
///
/// `code` is bound to nothing, so it evaluates to undecided — and the journey
/// that wrote `stipulate can(r, code) = true` read its own `code` as the state
/// it spells, because nothing in the journey is called that either. The two are
/// the same call, and matching on values alone could never see it.
///
/// So an argument nothing binds is retried as the state its name spells, and
/// kept only if a stipulation names exactly that state in exactly that place.
/// It is the judgement `compare` makes for `status = available`: one side is
/// a bare name and the other side says it is a state. Here the other side is
/// what the journey wrote, and without it nothing is read: a word nobody
/// stipulated stays undecided, with the reason it already had.
///
/// Only a name that could mean nothing else — see [`could_be_a_word`].
///
/// Looser than the emission path, `apply::Application::carried`, which reads
/// a bare word as a state only when the spec declares it. There nobody else
/// has spoken; here the journey's own stipulation is the other side, and it
/// is kept only on an exact match.
fn answered_by_spelling(
    name: &str,
    whole: &Expr,
    nodes: &[&Expr],
    evaluated: &[Evaluation],
    env: &Env<'_>,
) -> Option<Evaluation> {
    let mut spelled = Vec::with_capacity(nodes.len());
    let mut unresolved = Vec::new();
    let mut read_any = false;
    for (node, argument) in nodes.iter().zip(evaluated) {
        match bare_name(node) {
            Some(word) if argument.value.is_unknown() && could_be_a_word(word, env) => {
                spelled.push(Value::Enum(word.to_owned()));
                read_any = true;
            }
            _ => {
                spelled.push(argument.value.clone());
                unresolved.extend(argument.unresolved.iter().cloned());
            }
        }
    }
    if !read_any {
        return None;
    }
    let answer = env.world.answer_in(env.module, name, &spelled)?;
    Some(told(name, answer, unresolved, whole, env))
}

/// What a stipulation said the call comes back as.
///
/// An answer that is itself unknown — a journey stipulating `can(r) = x.f`
/// with `f` unset — settles nothing, and says so: passed through bare, the
/// rule asking was undecided with no reason, which stipulation 1 calls
/// indistinguishable from a bug.
fn told(
    name: &str,
    answer: &Value,
    unresolved: Vec<super::Unresolved>,
    whole: &Expr,
    env: &Env<'_>,
) -> Evaluation {
    if answer.holds_unknown() {
        return Evaluation::unknown(
            format!("the answer stipulated for `{name}` has no value"),
            whole,
            env.source,
        )
        .carrying(unresolved);
    }
    Evaluation { value: answer.clone(), unresolved }
}

/// Whether nothing in scope could give `word` a meaning of its own.
///
/// Three things read exactly like a word nothing binds, and each is a question
/// about a value rather than about a state with that spelling:
///
/// - a name bound to a value nobody knows — `can(repo, how)` with `how`
///   undecided must not be answered by what was said about a state `how`;
/// - a parameter of the rule that nobody passed, which is unbound;
/// - inside an entity's own definition, one of its fields nobody set. Its
///   stored fields are bound only when they hold something, and nothing here
///   knows the entity's declared field list — so inside an entity (`this`
///   bound) no name is read as a word at all. `allowed: can(this, level)` with
///   `level` unset is a question about an unknown number.
fn could_be_a_word(word: &str, env: &Env<'_>) -> bool {
    !env.bindings.contains_key(word)
        && !env.parameters.contains(word)
        && !env.bindings.contains_key("this")
}

/// Whether the entity `id` is an instance of defines a function `name` — or
/// `id` is in no world, which [`defined`] is the one to say.
fn defines(id: &crate::value::EntityId, name: &str, env: &Env<'_>) -> bool {
    env.world.instance(id).is_none_or(|instance| {
        let key = inspect_model::derived_key(&instance.module, &instance.entity, name);
        env.derived.functions.contains_key(key.as_str())
    })
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
    let instance = match env.instance(id) {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            return Evaluation::unknown(format!("`{id}` is not in this world"), whole, env.source);
        }
        Err(removal) => return super::removed::removed_here(id, removal, whole, env),
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
        scope.set(param.clone(), value);
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
