//! Driving the world forward: what an act sets off, and what time settles.
//!
//! Split from the walk itself because the two ask different questions. The walk
//! asks what the journey said next; this asks what the *specification* says
//! happens once it has been said — and in a real spec set most of that is one
//! hop further on than the rule the act names.
//!
//! Two loops, and they are the same loop pointed at different things. `settle`
//! runs what the world has made true; `fire` runs what the rules have emitted.
//! The simulator does neither: its step is one step, and it hands both back to
//! the reader because in the browser a person picks which to follow. A journey
//! has already picked, twice over — it said time passed, and it said he acted.
//!
//! Both are bounded and both remember what they have already run, because a
//! spec whose rules feed each other is ordinary rather than exceptional.

use inspect_sim::{StepOutcome, Value, enabled, step::step, world::Event};

use crate::run::{Settled, Walker, already_ran};

impl Walker<'_> {
    /// Fire an event, and everything it sets off.
    ///
    /// The simulator's step is one step: it collects what the rules emitted and
    /// offers those triggers to the reader, because in the browser a person
    /// picks which to follow. A journey has already picked. `he writes into the
    /// room` is not a claim about one rule — it is a claim about what the
    /// specification says happens when he does, and in a real spec set most of
    /// that is one hop further on. `SendMessage` creates the message and emits
    /// `MessageSent`; `QueueOnSend` waits on that and files the outbox entry,
    /// with no preconditions at all. Stopping at the first rule reported the
    /// second as never having run, so a journey could not say the message was
    /// filed, and `creating entry: delivery/OutboxEntry` caught nothing —
    /// because nothing had been created.
    ///
    /// The same reasoning as [`Self::settle`] one door along, and the same two
    /// guards: a bound, and a memory of what has already run. Here the memory
    /// is the event itself — trigger *and* arguments — because a spec emitting
    /// one trigger per recipient is fan-out and must not be cut short, while
    /// the same trigger with the same arguments twice is a loop.
    ///
    /// The returned outcome is the act's own. The verdict of the step is about
    /// the act; what followed is asserted with `then <Rule> fires`, and the
    /// rules that ran are accumulated across the whole cascade so it can be.
    pub(crate) fn fire(&mut self, event: &Event) -> StepOutcome {
        const ROUNDS: usize = 32;
        let acted = self.once(event);

        let mut seen = vec![key_of(event)];
        let mut pending = emissions(&acted);
        for _ in 0..ROUNDS {
            let next: Vec<Event> =
                pending.into_iter().filter(|event| !seen.contains(&key_of(event))).collect();
            if next.is_empty() {
                break;
            }
            pending = Vec::new();
            for event in next {
                seen.push(key_of(&event));
                pending.extend(emissions(&self.once(&event)));
            }
        }

        acted
    }

    /// One turn of the engine, remembering what ran.
    fn once(&mut self, event: &Event) -> StepOutcome {
        use inspect_sim::Disposition;
        let outcome = step(self.spec, self.program, self.sources, &self.world, event);
        for rule in &outcome.rules {
            match rule.disposition {
                Disposition::Fired => self.fired.push(rule.name.clone()),
                Disposition::Undecided => self.undecided.push(rule.name.clone()),
                Disposition::Refused | Disposition::Unsimulatable => {}
            }
        }
        self.world = outcome.world.clone();
        outcome
    }

    /// Fire everything the world now makes true, until nothing else does.
    ///
    /// A state-condition rule is not fired by anybody — the clock passing a due
    /// date is what makes it hold — and the simulator reports those as *newly
    /// enabled* rather than running them, because in the browser a person picks
    /// which to follow. A journey has already said: time passed, so whatever
    /// became true happened.
    ///
    /// To a fixpoint, because one rule firing can make the next one true, and
    /// bounded because a spec with two rules that re-enable each other would
    /// otherwise run forever. Reaching the bound is reported rather than
    /// silently truncated.
    pub(crate) fn settle(&mut self) -> Settled {
        const ROUNDS: usize = 32;
        let mut ran: Vec<(String, Value)> = Vec::new();
        for _ in 0..ROUNDS {
            // Everything the world makes true, not everything this step made
            // newly true: a rule enabled before the clock moved and never run
            // is still waiting, and a journey that skipped it would report a
            // world the spec does not describe.
            let waiting: Vec<(String, String, String, Value)> =
                enabled(self.spec, self.program, self.sources, &self.world)
                    .into_iter()
                    .flat_map(|rule| {
                        let (trigger, module, binding) = (rule.trigger, rule.module, rule.binding);
                        rule.over.into_iter().map(move |over| {
                            (trigger.clone(), module.clone(), binding.clone(), over)
                        })
                    })
                    // A rule already run for that same instance is where the
                    // fixpoint comes from. Without it a rule whose effect keeps
                    // its own condition true — `status = lost` stays lost —
                    // runs thirty-two times and then reports never settling.
                    .filter(|(trigger, _, _, over)| !already_ran(&ran, trigger, over))
                    .collect();
            if waiting.is_empty() {
                return Settled::Yes;
            }
            for (trigger, module, binding, over) in waiting {
                let mut event = Event::new(&trigger, &module);
                // Under the name the `when` clause gave it. A state rule's
                // clauses are written about `copy`, and firing without that
                // binding evaluates every one of them against nothing.
                event.arguments.insert(binding, over.clone());
                self.fire(&event);
                ran.push((trigger, over));
            }
        }
        Settled::No { rounds: ROUNDS }
    }
}

/// Every trigger this step emitted, as the events they would be.
fn emissions(outcome: &StepOutcome) -> Vec<Event> {
    let mut found = Vec::new();
    for rule in &outcome.rules {
        for effect in &rule.effects {
            if let inspect_sim::Effect::Emitted { trigger, module, arguments } = effect {
                found.push(Event {
                    trigger: trigger.clone(),
                    module: module.clone(),
                    arguments: arguments.clone(),
                });
            }
        }
    }
    found
}

/// What makes one firing the same as another: the trigger, and what it carries.
///
/// Rendered rather than compared as values so this can live in a `Vec` without
/// asking `Value` for `Ord` — the lists here are a handful long, and a spec
/// that emitted enough for that to matter would hit the round bound first.
fn key_of(event: &Event) -> String {
    let carried: Vec<String> =
        event.arguments.iter().map(|(name, value)| format!("{name}={}", value.render())).collect();
    format!("{}/{}({})", event.module, event.trigger, carried.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What makes one firing the same as another.
    ///
    /// The whole of the loop guard. Two rules that emit each other would run
    /// until the round bound without it, and a spec emitting one trigger per
    /// recipient would be cut short *with* the wrong version of it — so the
    /// arguments count, and this is where that is said.
    mod sameness {
        use inspect_sim::value::EntityId;

        use super::*;

        fn about(trigger: &str, who: u64) -> Event {
            Event::new(trigger, "messaging")
                .with("message", Value::Ref(EntityId::new("Message", 1)))
                .with("to", Value::Ref(EntityId::new("Device", who)))
        }

        #[test]
        fn the_same_trigger_carrying_the_same_things_is_the_same_firing() {
            assert_eq!(key_of(&about("MessageSent", 1)), key_of(&about("MessageSent", 1)));
        }

        /// Fan-out, not a loop: one emission per recipient is what a delivery
        /// rule does, and cutting it short after the first would lose every
        /// device but one.
        #[test]
        fn the_same_trigger_carrying_different_things_is_not() {
            assert_ne!(key_of(&about("MessageSent", 1)), key_of(&about("MessageSent", 2)));
        }

        #[test]
        fn two_triggers_are_never_the_same_firing() {
            assert_ne!(key_of(&about("MessageSent", 1)), key_of(&about("MessageRead", 1)));
        }

        /// The module is part of it: two specs may each declare a trigger of
        /// the same name, and they are not the same event.
        #[test]
        fn the_same_name_in_two_modules_is_two_triggers() {
            assert_ne!(
                key_of(&Event::new("Sent", "messaging")),
                key_of(&Event::new("Sent", "delivery"))
            );
        }
    }
}
