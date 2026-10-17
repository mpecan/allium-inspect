//! Which instances a rule's `ensures` removes, found before any of them is
//! applied for real (issue #33).
//!
//! Every clause reads the state the rule leaves, and which instances that
//! state has lost is not known until the last removal has run. So the clauses
//! are applied to copies that are then thrown away, each pass told what the
//! one before it removed, until two passes agree.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

use crate::{
    apply::{Against, Application},
    eval::{Removal, Told, Unresolved, span_of},
    value::{EntityId, Value},
    world::World,
};

/// What `ensures` removes from `world`, each by the clause that removes it
/// and as `world` held it, and why that is undecided when it never settled.
///
/// A pass can remove what the one before it did not: `not exists upload`
/// written above `if upload.big: not exists upload.thumb` finds the upload as
/// the rule found it only once it is known to go. For any rule a person writes
/// the second or third pass agrees with the one before it.
///
/// A spec can be written so the answer never settles — a value the rule
/// leaves behind, read from what it removes, deciding whether it is removed:
/// removed, the value is undecided and nothing removes it; kept, the value
/// says to remove it. [`PASSES`] passes without agreement are not answered
/// by whichever pass came last. Everything any pass removed is
/// treated as removed — so every read of it is undecided and says why — and
/// the rule is told the question did not settle.
pub(crate) fn removed_by(
    (against, rule): (Against<'_>, &str),
    world: &World,
    (bindings, told): (&BTreeMap<String, Value>, &Told),
    ensures: &[Expr],
) -> (BTreeMap<EntityId, Removal>, Option<Unresolved>) {
    let mut removed = BTreeMap::new();
    if !ensures.iter().any(|clause| may_remove(clause, against.source)) {
        return (removed, None);
    }
    let mut seen: Vec<BTreeMap<EntityId, Removal>> = vec![removed.clone()];
    for _ in 0..PASSES {
        let mut scratch = world.clone();
        let mut application = Application::new(against, &mut scratch, bindings.clone())
            .telling(told.clone())
            .removing(removed);
        for clause in ensures {
            application.apply(clause);
        }
        let mut found = application.removed().clone();
        for (id, removal) in &mut found {
            removal.rule = rule.to_owned();
            if let Some(before) = world.instance(id) {
                removal.found = before.clone();
            }
        }
        if seen.last().is_some_and(|last| last.keys().eq(found.keys())) {
            return (found, None);
        }
        seen.push(found.clone());
        removed = found;
    }
    let mut every = BTreeMap::new();
    for pass in seen {
        for (id, removal) in pass {
            every.entry(id).or_insert(removal);
        }
    }
    let why = unsettled(rule, &every);
    (every, Some(why))
}

/// How many times [`removed_by`] reads a rule's `ensures` looking for two
/// readings that agree.
const PASSES: usize = 4;

/// Whether `clause` could remove anything: whether its text says `not
/// exists`. A rule that never says it skips the search; a clause whose text
/// cannot be found is searched, to be safe.
fn may_remove(clause: &Expr, source: &str) -> bool {
    span_of(clause).and_then(|span| span.slice(source)).is_none_or(|text| {
        let words: Vec<&str> = text.split_whitespace().collect();
        words.windows(2).any(|pair| pair == ["not", "exists"])
    })
}

/// Why whether `rule` removes `every` is undecided.
fn unsettled(rule: &str, every: &BTreeMap<EntityId, Removal>) -> Unresolved {
    let which: Vec<String> =
        every.iter().map(|(id, removal)| format!("`{id}` (`{}`)", removal.clause)).collect();
    Unresolved {
        reason: format!(
            "whether rule `{rule}` removes {} is undecided: what decides it is read in the \
             state the rule leaves, and removing it unsettles that; bind what decides it \
             beside the `when`, where a `let` is read before the rule changes anything",
            which.join(", ")
        ),
        expression: None,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use allium_parser::{
        Span,
        ast::{Expr, Ident},
    };

    use super::may_remove;

    /// A clause spanning all of `source`.
    fn clause(source: &str) -> Expr {
        Expr::Ident(Ident { span: Span { start: 0, end: source.len() }, name: String::new() })
    }

    /// Only a clause whose text says `not exists` — however it is laid out —
    /// sends a rule looking for what it removes; one whose text cannot be
    /// found does too, to be safe.
    #[test]
    fn only_a_clause_saying_not_exists_could_remove_anything() {
        for (source, could) in [
            ("not exists upload", true),
            ("if upload.big:\n    not\n  exists upload", true),
            ("Blob.created(name: upload.name)", false),
            ("exists upload", false),
            ("upload.notexists = 1", false),
        ] {
            assert_eq!(may_remove(&clause(source), source), could, "{source}");
        }
        let lost = Expr::Ident(Ident { span: Span { start: 5, end: 99 }, name: String::new() });
        assert!(may_remove(&lost, "short"));
    }
}
