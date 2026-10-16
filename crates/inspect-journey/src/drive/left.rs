//! What the clock left undecided, said once the world has stopped changing.

use inspect_model::NodeKind;
use inspect_sim::{Disposition, Enabled, EntityId, step::step, world::Event};

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
                    let why = crate::outcome::undecided(woken);
                    self.undecided.push(crate::answers::noted(woken, why, self.firings));
                    break;
                }
            }
        }
    }

    /// Rules a round listed as due on `instance`, which another firing in the
    /// same round removed before their own event came round.
    ///
    /// Undecided, each naming the rule that removed it. The round fires its
    /// events in a fixed order, so had theirs come first they would have run:
    /// a `then … does not fire` about them holding here would be that order's
    /// answer, flipped by renaming a rule, and not the specification's.
    pub(super) fn raced(&mut self, woken: &[String], instance: &EntityId) {
        let by = self.removed.get(instance).map_or("another rule", String::as_str);
        let mut said = Vec::new();
        for id in woken {
            let Some(node) = self.spec.nodes_of(NodeKind::Rule).find(|it| it.id.as_str() == id)
            else {
                continue;
            };
            said.push(Undecided {
                rule: node.name.clone(),
                why: format!(
                    "`{}` was due on `{instance}` in the same round, and `{by}` removed it \
                     before it ran; the specification does not say which of the two runs first",
                    node.name
                ),
                // No `turn`: it orders the calls a rule asked against a `where`
                // that landed, and a rule that never ran asked nothing.
                ..Undecided::default()
            });
        }
        self.undecided.extend(said);
    }
}
