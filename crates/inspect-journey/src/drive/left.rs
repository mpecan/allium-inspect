//! What the clock left undecided, said once the world has stopped changing.

use std::collections::BTreeMap;

use inspect_model::NodeKind;
use inspect_sim::{
    Disposition, Effect, Enabled, EntityId, Unresolved, Value, step::step, world::Event,
};

use crate::run::{Undecided, Walker};

impl Walker<'_> {
    /// The temporal rules the clock left undecided: each once, with the reason
    /// it gave when woken on an instance it could not settle.
    ///
    /// Read once the world has stopped changing, because that is the world the
    /// step claims. Nothing fires a rule whose `when` nobody could settle, so
    /// without this it went unsaid unless something else happened to wake an
    /// instance of its entity — and a step holding because nothing woke it is
    /// an unknown read as no (issue #20). Woken on a copy of the world rather
    /// than taken from the listing alone, because a `when` nobody could settle
    /// beside a `requires` that refuses is a rule that does not run, and says
    /// nothing a reader needs to hear.
    ///
    /// Only the instances the listing could not settle are woken: one it said
    /// no about cannot come back undecided, and waking it is a whole step for
    /// nothing. One reason per rule, the first instance that gives one — as
    /// `then … fires` gives one — and none for a rule this step has already
    /// said could not be decided.
    pub(super) fn undecided_over_what_is_left(&mut self, rules: Vec<Enabled>) {
        for rule in rules {
            if self.undecided.iter().any(|it| it.rule == rule.name) {
                continue;
            }
            for instance in rule.unsettled_over {
                let mut event = Event::new(&rule.trigger, &rule.module);
                event.arguments.insert(rule.binding.clone(), instance);
                let outcome = step(self.spec, self.program, self.sources, &self.world, &event);
                let woken = outcome
                    .rules
                    .iter()
                    .find(|it| it.rule == rule.rule && it.disposition == Disposition::Undecided);
                if let Some(woken) = woken {
                    let why = crate::outcome::undecided(woken, |note| self.said_of_the_cast(note));
                    self.undecided.push(crate::answers::noted(woken, why, self.firings));
                    break;
                }
            }
        }
    }

    /// Rules a round listed as due on `instance`, which another firing in the
    /// same round, by rule `by`, removes.
    ///
    /// Undecided, each naming the rule that removes it, whichever of the two
    /// firings the round happens to reach first: had theirs come first they
    /// would have run, and a `then … does not fire` holding here — or a `then
    /// … fires` holding when the order is the other way round — would be
    /// that order's answer, flipped by renaming a rule, and not the
    /// specification's. Said of the cast, as every reason the walk gives.
    pub(super) fn raced(&mut self, woken: &[String], instance: &EntityId, by: &str) {
        let mut said = Vec::new();
        for id in woken {
            let Some(node) = self.spec.nodes_of(NodeKind::Rule).find(|it| it.id.as_str() == id)
            else {
                continue;
            };
            let why = self.said_of_the_cast(&Unresolved {
                reason: format!(
                    "`{}` was due on `{instance}` in the same round, and `{by}` removed it; the \
                     specification does not say which of the two runs first",
                    node.name
                ),
                expression: None,
                span: None,
            });
            said.push(Undecided {
                rule: node.name.clone(),
                why,
                // No `turn`: it orders the calls a rule asked against a `where`
                // that landed, and a rule that never ran asked nothing.
                ..Undecided::default()
            });
        }
        self.undecided.extend(said);
    }

    /// What each event of a round removes, read on the world the round
    /// began in, by the index of the event and the rule that removes it.
    ///
    /// Only a round of two events or more can race, so a round of one is
    /// not read twice.
    pub(super) fn round_removes(
        &self,
        waiting: &[(Event, Value, Vec<String>)],
    ) -> BTreeMap<EntityId, Vec<(usize, String)>> {
        let mut removes: BTreeMap<EntityId, Vec<(usize, String)>> = BTreeMap::new();
        if waiting.len() < 2 {
            return removes;
        }
        for (at, (event, _, _)) in waiting.iter().enumerate() {
            let outcome = step(self.spec, self.program, self.sources, &self.world, event);
            for rule in &outcome.rules {
                for effect in &rule.effects {
                    if let Effect::Removed { id, .. } = effect {
                        removes.entry(id.clone()).or_default().push((at, rule.name.clone()));
                    }
                }
            }
        }
        removes
    }
}
