//! What a rule removes, while its `ensures` are applied (issue #33).
//!
//! `ensures` clauses are a set of postconditions about the state the rule
//! leaves, not a sequence. So the rule's removals are known before any clause
//! is applied (`siblings::removed_by` finds them), and they are made up front:
//! every clause then reads the state the rule leaves in the same way,
//! whichever line it is written on. A value read from a removed instance is
//! undecided and says why ([`crate::eval::Removal`]); a relationship through
//! which the removed instances were reached has none of them left.
//!
//! What **decides** which postconditions apply — an `if`'s condition, a
//! `for`'s collection and filter, what a `not exists` or an assignment is
//! about — reads the world the rule found instead: the same world with what
//! the rule removes put back as it was before any clause ran. `if upload.big:
//! not exists upload` asks whether to remove it, and asked of the state the
//! removal itself produces the question has no answer.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;

use super::{Application, Applied, Effect};
use crate::{
    eval::{Env, Evaluation, Removal, Unresolved, eval},
    value::{EntityId, Instance, Value},
};

impl<'a> Application<'a> {
    /// The same application, knowing the rule removes each of `removed`, and
    /// with each already gone from the world it applies to.
    #[must_use]
    pub fn removing(mut self, removed: BTreeMap<EntityId, Removal>) -> Self {
        for id in removed.keys() {
            self.world.remove(id);
        }
        self.removed_here = removed;
        self
    }

    /// What the clauses applied so far removed, each by the clause that
    /// removed it as written — the map [`Application::removing`] takes.
    #[must_use]
    pub fn removed(&self) -> &BTreeMap<EntityId, Removal> {
        &self.removed
    }

    /// `ensures: not exists presence` — afterwards, it is gone.
    ///
    /// What the language says `not exists` means as an outcome: the entity is
    /// removed. One instance is removed, and `null` — a lookup that matched
    /// nothing — already holds. A collection is noted: `not exists Copy` names
    /// a type where an instance was meant, and removing every copy in the
    /// world on the strength of it would be a guess with side effects.
    ///
    /// An instance made gone up front by [`Application::removing`] is removed
    /// here all the same, once: this is the clause that removes it.
    pub(super) fn removal(&mut self, whole: &Expr, operand: &Expr) -> Applied {
        let found = self.deciding(operand);
        match found.value {
            Value::Ref(id) => {
                let taken = self.world.remove(&id).or_else(|| {
                    let ahead = self.removed_here.get(&id)?;
                    (!self.removed.contains_key(&id)).then(|| ahead.found.clone())
                });
                let Some(instance) = taken else {
                    // Already gone is the end state the clause asks for.
                    return Applied::default();
                };
                let entity = instance.entity.clone();
                let removal = Removal {
                    rule: String::new(),
                    subject: self.describe(operand).unwrap_or_else(|| id.as_str().to_owned()),
                    clause: self.describe(whole).unwrap_or_else(|| "not exists".to_owned()),
                    found: instance,
                };
                self.removed.entry(id.clone()).or_insert(removal);
                Applied::effect(Effect::Removed { id, entity })
            }
            Value::Null => Applied::default(),
            Value::Unknown => self.skipped(whole, found.unresolved),
            _ => self.asserted(whole),
        }
    }

    /// What decides which postconditions apply, and to what, read in the
    /// world the rule found.
    pub(super) fn deciding(&self, node: &Expr) -> Evaluation {
        self.decide(|scope| eval(node, &scope))
    }

    /// `read` in the world the rule found: this one, with every instance the
    /// rule removes as it was before any clause ran, and every `let` inside
    /// `ensures` naming what it reads there.
    pub(super) fn decide<R>(&self, read: impl FnOnce(Env<'_>) -> R) -> R {
        if self.removed_here.is_empty() && self.decided.is_empty() {
            return read(self.scope_in(self.world));
        }
        let mut found = self.world.clone();
        for (id, removal) in &self.removed_here {
            if found.instance(id).is_none() {
                found.insert(removal.found.clone());
            }
        }
        let mut scope = self.scope_in(&found);
        for (name, (value, why)) in &self.decided {
            scope.told.remove(name);
            crate::eval::remember(&mut scope.told, name, value, why.clone());
            scope.bindings.insert(name.clone(), value.clone());
        }
        read(scope)
    }

    /// `id` as the world holds it, or as the rule found it when the rule
    /// removes it: what an assignment to it is checked against.
    pub(super) fn held(&self, id: &EntityId) -> Option<&Instance> {
        self.world.instance(id).or_else(|| self.removed_here.get(id).map(|it| &it.found))
    }

    /// `let name = value` inside `ensures`, over something that is not a call.
    ///
    /// A name for a read, so it reads what the read would wherever the name
    /// is used: in the state the rule leaves for a value the rule leaves
    /// behind, and in the world the rule found for what decides. `let big =
    /// upload.big` above `if big: not exists upload` decides the removal the
    /// way `if upload.big:` does; read only in the state the rule leaves, the
    /// removal undid the answer that made it, and which pass of the search
    /// for what the rule removes came last decided whether it happened.
    pub(super) fn named(&mut self, name: &str, value: &Expr) -> Applied {
        let evaluated = self.evaluate(value);
        let decided = (!self.removed_here.is_empty()).then(|| self.deciding(value));
        self.bind(name, evaluated.value.clone(), evaluated.unresolved.clone());
        let Some(decided) = decided.filter(|it| it.value != evaluated.value) else {
            return Applied::note(evaluated.unresolved);
        };
        self.decided.insert(name.to_owned(), (decided.value, decided.unresolved));
        // Said where the name is read in the state the rule leaves, if it is:
        // read only to decide, there is nothing undecided to say.
        Applied::default()
    }

    /// Why a value the rule leaves behind is undecided, when the only reason
    /// is that the rule removes what it reads: read in the world the rule
    /// found, it is settled.
    ///
    /// Said as part of what the rule ensures that the world does not show,
    /// so the act that fired it says so. Otherwise an emission's argument
    /// reached its listener as a bare unknown with the reason left behind on
    /// the emitter, and a creation nobody asserted about held in silence.
    pub(super) fn lost(&self, node: &Expr, evaluated: &Evaluation) -> Option<Unresolved> {
        if self.removed_here.is_empty() || !evaluated.value.holds_unknown() {
            return None;
        }
        if self.deciding(node).value.holds_unknown() {
            return None;
        }
        evaluated.unresolved.first().cloned()
    }
}

impl Applied {
    /// `lost`, when there is one, as something promised and not applied.
    pub(super) fn lose(&mut self, lost: Option<Unresolved>) {
        let Some(why) = lost else { return };
        if !self.unresolved.contains(&why) {
            self.unresolved.push(why.clone());
        }
        if !self.unapplied.contains(&why) {
            self.unapplied.push(why);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Applied;
    use crate::eval::Unresolved;

    fn note(reason: &str) -> Unresolved {
        Unresolved { reason: reason.to_owned(), expression: None, span: None }
    }

    /// What was lost is said once among the notes and once as not applied,
    /// whether or not the value's own notes already carried it — which they
    /// do, when the value is undecided only because of the removal.
    #[test]
    fn what_is_lost_is_said_once_and_marked_once() {
        let why = note("removed");
        let mut carried = Applied { unresolved: vec![why.clone()], ..Applied::default() };
        carried.lose(Some(why.clone()));
        assert_eq!(carried.unresolved, vec![why.clone()]);
        assert_eq!(carried.unapplied, vec![why.clone()]);

        let mut fresh = Applied { unresolved: vec![note("other")], ..Applied::default() };
        fresh.lose(Some(why.clone()));
        assert_eq!(fresh.unresolved, vec![note("other"), why.clone()]);
        assert_eq!(fresh.unapplied, vec![why]);

        let mut nothing = Applied::default();
        nothing.lose(None);
        assert_eq!(nothing, Applied::default());
    }
}
