//! Whose call an answer is about (issue #32).
//!
//! Two modules may each name a black box `watchers`, and `tally/watchers(t)`
//! is how a journey says which one it answers. An unqualified answer answers
//! every module's call; a qualified one only the module it names — and that is
//! the module that *wrote* the call, which for a computed field is the module
//! declaring the entity, not the one whose rule happened to read the field.
//!
//! The walk reads that from two places. A rule's own module covers the common
//! case. A call written in another module's definition is caught by the
//! simulator, which knows where it read the call and says so in its reason —
//! see [`inspect_sim::eval::answers_only`] — so the walk reads it back rather
//! than guessing.

use inspect_model::SpecGraph;
use inspect_sim::{Value, eval::answers_only, world::World};

use crate::run::Undecided;

/// The module an answer names and the call it answers there: `tally/watchers`
/// is `tally`'s `watchers`, and `watchers` is every module's.
pub(crate) fn qualified(call: &str) -> (Option<&str>, &str) {
    call.rsplit_once('/').map_or((None, call), |(module, bare)| (Some(module), bare))
}

/// Whether `expression` is a call to `call`: `reach(issue)` is one to `reach`,
/// and `outreach(issue)` is not.
fn calls(expression: &str, call: &str) -> bool {
    expression.split_once('(').is_some_and(|(name, _)| name.trim() == call)
}

/// The first call `rule` left unanswered that is a call of the name `call`
/// answers, in whichever module — see [`reaches`] for whether it does.
pub(crate) fn asked_of<'a>(rule: &'a Undecided, call: &str) -> Option<&'a str> {
    let (_, bare) = qualified(call);
    rule.asked.iter().find(|expression| calls(expression, bare)).map(String::as_str)
}

/// Whether an answer to `call` is one `rule` could read: an unqualified one
/// in every module, a qualified one only in the module that wrote the call.
///
/// The rule's module is that module unless the simulator said otherwise: a
/// call inside another module's definition, unanswered with this answer in
/// force, is reported as `` `b/watchers` answers `b`'s `watchers` ``.
pub(crate) fn reaches(call: &str, rule: &Undecided) -> bool {
    let module = qualified(call).0;
    module.is_none_or(|module| module == rule.module)
        && !(module.is_some()
            && rule.reasons.iter().any(|reason| reason.contains(&answers_only(call))))
}

/// The remark for an answer naming another module than the one asking.
pub(crate) fn elsewhere(rule: &Undecided, expression: &str, written: &str, call: &str) -> String {
    let (module, bare) = qualified(call);
    format!(
        "`{}` asked `{expression}`, which `where {written}` does not answer — it answers \
         `{}`'s `{bare}` and no other module's",
        rule.rule,
        module.unwrap_or_default()
    )
}

/// Why an answer to `call` reaches no call at all: it names a module the
/// specification does not have. `None` when it names none, or one it has.
///
/// Refused rather than told: the world would keep it, nothing would ever ask
/// it, and the ledger would list a line that changed nothing. A `use … as`
/// alias is the likeliest slip, and is named as one.
pub(crate) fn no_module(spec: &SpecGraph, call: &str) -> Option<String> {
    let (module, bare) = qualified(call);
    let module = module?;
    if spec.modules.iter().any(|known| known.name == module) {
        return None;
    }
    let alias = spec.modules.iter().find_map(|importing| {
        let import = importing.imports.iter().find(|import| import.alias == module)?;
        Some((importing.name.as_str(), import.target.as_deref()?))
    });
    Some(match alias {
        Some((importing, target)) => format!(
            "the specification has no module `{module}` — `{module}` is what `{importing}` \
             calls `{target}`, and a journey names it `{target}/{bare}`"
        ),
        None => format!("the specification has no module `{module}`"),
    })
}

/// What an unqualified answer leaves unanswered: every module a qualified
/// answer about the same arguments is still in force in.
///
/// The qualified one wins there, as `config` does, whichever was said later —
/// so a bare `stipulate watchers(x) = {ada}` after `given b/watchers(x) = {}`
/// changes nothing in `b`, and the ledger listing it should not leave the
/// reader assuming it applies everywhere.
pub(crate) fn shadowed(world: &World, call: &str, arguments: &[Value]) -> Option<String> {
    if qualified(call).0.is_some() {
        return None;
    }
    let standing: Vec<String> = world
        .answered_elsewhere("", call)
        .into_iter()
        .filter(|named| world.answer(named, arguments).is_some())
        .map(|named| format!("`{named}`"))
        .collect();
    (!standing.is_empty()).then(|| {
        format!(
            "{} still answers this call in its own module, so this answers `{call}` \
             everywhere else",
            standing.join(" and ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_before_the_name_is_whose_call_it_is() {
        assert_eq!(qualified("tally/watchers"), (Some("tally"), "watchers"));
        assert_eq!(qualified("watchers"), (None, "watchers"));
    }

    fn asked_in(module: &str) -> Undecided {
        Undecided {
            rule: "Count".to_owned(),
            module: module.to_owned(),
            asked: vec!["watchers(t)".to_owned()],
            ..Undecided::default()
        }
    }

    /// Found by its bare name in any module, and read only in its own.
    #[test]
    fn a_qualified_answer_is_read_only_by_the_module_it_names() {
        let tally = asked_in("tally");
        let parcels = asked_in("parcels");
        assert_eq!(asked_of(&tally, "tally/watchers"), Some("watchers(t)"));
        assert_eq!(asked_of(&parcels, "tally/watchers"), Some("watchers(t)"));
        assert_eq!(asked_of(&tally, "tally/viewers"), None);
        assert!(reaches("tally/watchers", &tally));
        assert!(!reaches("tally/watchers", &parcels));
        assert!(reaches("watchers", &parcels));
    }

    /// A rule in `tally` that read `parcels`' definition asked `parcels`'
    /// call, and the simulator said so: `tally/watchers` did not reach it.
    #[test]
    fn a_call_read_from_another_modules_definition_is_that_modules() {
        let mut tally = asked_in("tally");
        tally.reasons = vec![format!(
            "nothing defines `watchers`, and no stipulation answers this call; {}, and this \
             one is `parcels`'s",
            answers_only("tally/watchers")
        )];
        assert!(!reaches("tally/watchers", &tally));
        // An unqualified answer is not what the reason is about.
        assert!(reaches("watchers", &tally));
    }
}
