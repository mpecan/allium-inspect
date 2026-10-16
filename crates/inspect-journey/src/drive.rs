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

use std::collections::BTreeSet;

use inspect_model::NodeKind;
use inspect_sim::{StepOutcome, Truth, Value, enabled, step::step, world::Event};

use crate::{
    check,
    journey::Journey,
    run::{Ground, Settled, Undecided, Walked, Walker, already_ran},
};

/// How many events one step may fire, its own included, before what it set
/// off is reported as never having settled.
const FIRINGS: usize = 512;

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

        let mut seen = BTreeSet::from([key_of(event)]);
        let mut pending = emissions(&acted);
        for round in 0..=ROUNDS {
            let next: Vec<Event> =
                pending.into_iter().filter(|event| !seen.contains(&key_of(event))).collect();
            if next.is_empty() || self.cut_off.is_some() {
                break;
            }
            // Reaching the bound is said, as `settle` says it. The memory stops
            // a loop that repeats itself; it cannot stop one that makes
            // something new every round — `Divide` reacting to a cell being
            // made by making another — and a world cut off there is not one
            // the act leads to.
            if round == ROUNDS {
                self.cut(&next, &format!("after {ROUNDS} rounds"));
                break;
            }
            pending = Vec::new();
            for (at, event) in next.iter().enumerate() {
                if !seen.insert(key_of(event)) {
                    continue;
                }
                // Rounds bound how deep a cascade goes, not how wide. A
                // reaction that makes two of what it reacts to doubles every
                // round, and thirty-two rounds of that is a walk that never
                // comes back to say it was cut off. So the firings are counted
                // too, across the whole step, and running out is said the
                // same way.
                if self.firings >= FIRINGS {
                    self.cut(&next[at..], &format!("after {FIRINGS} firings"));
                    break;
                }
                pending.extend(emissions(&self.once(event)));
            }
        }

        acted
    }

    /// Stop following a cascade, saying which triggers were still waiting and
    /// when it stopped: what the act leads to never settled, and the world it
    /// left is not one the specification describes.
    fn cut(&mut self, waiting: &[Event], when: &str) {
        let triggers: BTreeSet<&str> = waiting.iter().map(|it| it.trigger.as_str()).collect();
        let triggers: Vec<&str> = triggers.into_iter().collect();
        self.cut_off = Some(format!(
            "`{}` was still being set off {when}, so what this led to never settled",
            triggers.join("`, `")
        ));
    }

    /// Forget what ran, before a step that will say what it ran.
    pub(crate) fn forget(&mut self) {
        self.fired.clear();
        self.undecided.clear();
        self.unapplied.clear();
        self.refused.clear();
        self.created.clear();
        self.cut_off = None;
        self.firings = 0;
    }

    /// One turn of the engine, remembering what ran.
    fn once(&mut self, event: &Event) -> StepOutcome {
        use inspect_sim::Disposition;
        self.firings += 1;
        let outcome = step(self.spec, self.program, self.sources, &self.world, event);
        for rule in &outcome.rules {
            match rule.disposition {
                Disposition::Fired => {
                    self.fired.push(rule.name.clone());
                    // Fired, and part of what it ensures could not be applied:
                    // a member of its `for` the filter could not decide about,
                    // a branch nobody could choose. The world may be missing a
                    // change — or a chain the missing emission would have set
                    // off — so the step is undecided, and says what was not
                    // applied. Kept apart from `undecided`, because it ran.
                    if let Some(why) = crate::outcome::unapplied(rule) {
                        self.unapplied.push(Undecided { rule: rule.name.clone(), why });
                    }
                }
                Disposition::Undecided => self.undecided.push(Undecided {
                    rule: rule.name.clone(),
                    why: crate::outcome::undecided(rule),
                }),
                Disposition::Refused => self.refused.push(Undecided {
                    rule: rule.name.clone(),
                    why: crate::outcome::refused(rule),
                }),
                Disposition::Unsimulatable => {}
            }
            for effect in &rule.effects {
                if let inspect_sim::Effect::Created { id, .. } = effect {
                    self.created.push(id.clone());
                }
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
        // By *rule* and instance. A state rule's trigger is its entity, so two
        // rules on `Hold` share one; remembering the trigger took the second
        // rule on a hold for the first one running again, and it never ran.
        let mut ran: Vec<(String, Value)> = Vec::new();
        for _ in 0..ROUNDS {
            // A cascade cut off partway is already said, by `unsettled`, and
            // nothing run on the world it left would be the spec's either.
            if self.cut_off.is_some() {
                return Settled::Yes;
            }
            // Everything the world makes true, not everything this step made
            // newly true: a rule enabled before the clock moved and never run
            // is still waiting, and a journey that skipped it would report a
            // world the spec does not describe.
            // The event to fire, the instance it is about, and the rules it
            // was fired for.
            let mut waiting: Vec<(Event, Value, Vec<String>)> = Vec::new();
            for rule in enabled(self.spec, self.program, self.sources, &self.world) {
                for over in rule.over {
                    // A rule already run for that same instance is where the
                    // fixpoint comes from. Without it a rule whose effect keeps
                    // its own condition true — `status = lost` stays lost —
                    // runs thirty-two times and then reports never settling.
                    if already_ran(&ran, &rule.rule, &over) {
                        continue;
                    }
                    // One firing per instance: the event reaches every rule
                    // waiting on its entity, and each reads its own `when`.
                    // Firing it once per rule would run the first rule again
                    // for every other rule that was true beside it. The
                    // instance alone is the key, because its id already says
                    // which entity it is.
                    let at = match waiting.iter().position(|(_, instance, _)| *instance == over) {
                        Some(at) => at,
                        None => {
                            waiting.push((
                                Event::new(&rule.trigger, &rule.module),
                                over.clone(),
                                Vec::new(),
                            ));
                            waiting.len() - 1
                        }
                    };
                    let (event, _, rules) = &mut waiting[at];
                    // Under the name each `when` clause gave it. A state rule's
                    // clauses are written about `copy`, and firing without that
                    // binding evaluates every one of them against nothing — and
                    // two rules on one entity need not use the same name.
                    event.arguments.insert(rule.binding.clone(), over.clone());
                    rules.push(rule.rule.clone());
                }
            }
            if waiting.is_empty() {
                return Settled::Yes;
            }
            for (event, over, woken) in waiting {
                let outcome = self.fire(&event);
                // What woke it, and whatever else the same firing ran: a rule
                // that became true between the listing and the firing has run
                // now, and must not run again next round.
                let also = outcome
                    .rules
                    .iter()
                    .filter(|rule| rule.disposition == inspect_sim::Disposition::Fired)
                    .map(|rule| rule.rule.clone());
                for rule in woken.into_iter().chain(also) {
                    ran.push((rule, over.clone()));
                }
            }
        }
        Settled::No { rounds: ROUNDS }
    }
}

impl Walker<'_> {
    /// `then <Rule> fires`, or `does not fire`, against what the last step ran.
    pub(crate) fn fires(&self, rule: &str, negated: bool) -> (Truth, Option<String>) {
        let ran = self.fired.iter().any(|name| name == rule);
        // "did not run" and "could not be told whether it should" are
        // different answers, and only one of them is about the spec.
        if !ran && let Some(it) = self.undecided.iter().find(|it| it.rule == rule) {
            return (Truth::Unknown, Some(format!("`{rule}` could not be decided: {}", it.why)));
        }
        // Woken and refused: the spec's own reason, in its own words, rather
        // than a bare "did not run" a reader has to go and explain.
        //
        // Unless the step is unsettled. Refused on the firings that happened
        // says nothing about one that did not: a `for` member nobody could
        // decide about, or a cascade cut off, may have woken it again with
        // something it would have taken.
        if !ran && let Some(it) = self.refused.iter().find(|it| it.rule == rule) {
            let refused = format!("`{rule}` refused: {}", it.why);
            return match self.unsettled() {
                Some(why) => (
                    Truth::Unknown,
                    Some(format!(
                        "{refused} — but whether it would have run on what was not settled is \
                         not known: {why}"
                    )),
                ),
                None => (Truth::from_bool(negated), Some(refused)),
            };
        }
        if !ran && self.waits_on_the_world(rule) {
            // Nobody fires a state-condition rule; it becomes true or it does
            // not. The simulator lists the ones that became true and says
            // nothing about the rest, so a rule whose condition is *false* and
            // one whose condition could not be *decided* look identical from
            // here — and reporting the second as a flat no is the failure this
            // whole design refuses.
            return (
                Truth::Unknown,
                Some(format!(
                    "`{rule}` never became true, and whether its condition is false or could \
                     not be decided is not visible from here"
                )),
            );
        }
        // Not run, while something this step ran left part of what it ensures
        // unapplied or could not be decided at all. Whatever it did not emit
        // or write is exactly what might have woken this rule, so "did not
        // run" is a fact about this world and not about the spec — and `does
        // not fire` holding on it would be the simulator guessing.
        if !ran && let Some(why) = self.unsettled() {
            return (
                Truth::Unknown,
                Some(format!(
                    "`{rule}` did not run, but whether it would have is not settled — {why}"
                )),
            );
        }
        // Said either way. A refused `does not fire` with nothing under it
        // reads, in a report, like a line that held.
        (
            Truth::from_bool(ran != negated),
            Some(if ran { format!("`{rule}` ran") } else { format!("`{rule}` did not run") }),
        )
    }

    /// Whether a rule waits on the world rather than on somebody acting.
    ///
    /// Not a creation rule, although the model files one as a state rule: a
    /// creation is followed whenever a step makes one, so one that did not run
    /// was either never woken — nothing of its kind was made — or refused, and
    /// both of those are visible from here.
    fn waits_on_the_world(&self, rule: &str) -> bool {
        use inspect_model::graph::TriggerSource;
        self.spec.nodes_of(NodeKind::Rule).find(|node| node.name == rule).is_some_and(|node| {
            let creation = self
                .program
                .rule(node.id.as_str())
                .is_some_and(|ast| inspect_sim::creation::creation_of(ast.when.as_ref()).is_some());
            !creation
                && node.detail.as_rule().is_some_and(|detail| {
                    matches!(detail.source, TriggerSource::State | TriggerSource::Temporal)
                })
        })
    }
}

impl Walker<'_> {
    /// Walk the journey this one continues from, and stay in the world it left.
    ///
    /// Its steps are *run* and not reported: a reader looking at this journey
    /// asked about this journey, and pasting another one's steps above it would
    /// bury the one they came for. What comes back instead is the fact that
    /// decides whether any of this means anything — the verdict of the ground
    /// it is standing on.
    ///
    /// Recursive, because a life is longer than two mornings, and guarded
    /// because a chain that comes back to where it started would otherwise run
    /// until the stack gave out. A cycle is reported and not followed: the
    /// journeys in it are real and their steps are still worth walking, and
    /// only the loop is refused.
    pub(crate) fn stand_on(
        &mut self,
        named: &str,
        everything: &[Journey],
        chain: &mut Vec<String>,
    ) {
        if chain.iter().any(|earlier| earlier == named) {
            chain.push(named.to_owned());
            self.notes.push(crate::Outcome {
                line: 0,
                verdict: check::Verdict::Unspecified,
                about: format!("after {named}"),
                detail: Some(format!(
                    "these journeys continue from each other in a circle: {}",
                    chain.join(" → ")
                )),
            });
            return;
        }

        let Some(earlier) = everything.iter().find(|candidate| candidate.name == named) else {
            self.notes.push(crate::Outcome {
                line: 0,
                verdict: check::Verdict::Unspecified,
                about: format!("after {named}"),
                detail: Some(format!("no journey called `{named}` was read")),
            });
            return;
        };

        chain.push(named.to_owned());
        if let Some(before) = &earlier.after {
            self.stand_on(before, everything, chain);
        }

        // Whatever it says it was told is said in its name from here on, so a
        // reader can tell what this journey was told from what it walked in on.
        let was = self.standing_on.replace(named.to_owned());
        self.lay_out(earlier);
        let checked = check::check(earlier, everything, self.spec, self.program);
        let steps: Vec<Walked> =
            earlier.steps.iter().map(|step| self.walk_step(step, &checked)).collect();
        self.standing_on = was;

        let held = steps.iter().filter(|step| step.verdict().holds()).count();
        let verdict = crate::run::worst(steps.iter().map(Walked::verdict));
        // The nearest ground wins the line, and a break anywhere below it is
        // already in that verdict — a journey whose predecessor stood on
        // something broken did not hold either.
        self.after = Some(Ground {
            journey: named.to_owned(),
            title: crate::title::readable(named),
            verdict,
            held,
            of: steps.len(),
        });
    }
}

/// What an act set off that could not be decided, as the reason the act was
/// not decided either — or `None` when everything it set off was.
///
/// Each rule once, in the order it first ran, with the first reason it gave: a
/// rule woken twice is still one rule nobody could decide, and the reason is
/// what tells a reader whether the fix is in their journey or in the spec.
pub(crate) fn set_off(undecided: &[Undecided]) -> Option<String> {
    let said: Vec<String> = first_per_rule(undecided)
        .map(|it| format!("`{}` could not be decided: {}", it.rule, it.why))
        .collect();
    (!said.is_empty()).then(|| format!("what it set off: {}", said.join("; ")))
}

/// What fired and left part of what it ensures unapplied, one line per rule —
/// or nothing when everything that fired was applied in full.
///
/// Worded as having fired, because it did: "could not be decided" beside a
/// `then Fan fires` that holds is two answers to one question.
pub(crate) fn partly(unapplied: &[Undecided]) -> Option<String> {
    let said: Vec<String> = first_per_rule(unapplied)
        .map(|it| {
            format!("`{}` fired, but part of what it ensures was not applied: {}", it.rule, it.why)
        })
        .collect();
    (!said.is_empty()).then(|| said.join("; "))
}

/// Each rule once, in the order it first appears, with its first reason.
fn first_per_rule(all: &[Undecided]) -> impl Iterator<Item = &Undecided> {
    let mut named = BTreeSet::new();
    all.iter().filter(move |it| named.insert(it.rule.as_str()))
}

impl Walker<'_> {
    /// Everything this step ran that leaves the world unsettled — what it set
    /// off that could not be decided, and what fired and was not applied in
    /// full — or `None` when there is nothing.
    pub(crate) fn unsettled(&self) -> Option<String> {
        let said: Vec<String> = set_off(&self.undecided)
            .into_iter()
            .chain(partly(&self.unapplied))
            .chain(self.cut_off.clone())
            .collect();
        (!said.is_empty()).then(|| said.join("; "))
    }
}

/// Every trigger this step emitted, and every creation it made that a rule
/// reacts to, as the events they would be.
///
/// A creation is followed for the same reason an emission is: `when: o:
/// Order.created` is a rule the specification says runs when an order is made,
/// and a journey that made one has said it was. Left out, the order was there
/// and the rule that reacts to it never ran.
fn emissions(outcome: &StepOutcome) -> Vec<Event> {
    let mut found = Vec::new();
    for rule in &outcome.rules {
        for effect in &rule.effects {
            if let inspect_sim::Effect::Emitted { trigger, module, arguments, positional } = effect
            {
                found.push(Event {
                    trigger: trigger.clone(),
                    module: module.clone(),
                    arguments: arguments.clone(),
                    positional: positional.clone(),
                    emitted: true,
                });
            }
        }
    }
    found.extend(inspect_sim::reactions(outcome));
    found
}

/// What makes one firing the same as another: the trigger, and what it carries.
///
/// Rendered rather than compared as values so this can live in a `Vec` without
/// asking `Value` for `Ord` — the lists here are a handful long, and a spec
/// that emitted enough for that to matter would hit the round bound first.
fn key_of(event: &Event) -> String {
    let carried: Vec<String> = event
        .positional
        .iter()
        .map(Value::render)
        .chain(event.arguments.iter().map(|(name, value)| format!("{name}={}", value.render())))
        .collect();
    format!("{}/{}({})", event.module, event.trigger, carried.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod set_off {
        use super::*;

        fn rule(name: &str, why: &str) -> Undecided {
            Undecided { rule: name.to_owned(), why: why.to_owned() }
        }

        #[test]
        fn nothing_undecided_says_nothing() {
            assert_eq!(set_off(&[]), None);
        }

        /// Named, with its reason: the name says where, the reason says what.
        #[test]
        fn names_the_rule_and_why() {
            assert_eq!(
                set_off(&[rule("Halted", "nothing is bound to `abort`")]).as_deref(),
                Some("what it set off: `Halted` could not be decided: nothing is bound to `abort`")
            );
        }

        /// Fan-out wakes one rule once per recipient; it is still one rule.
        #[test]
        fn a_rule_woken_twice_is_named_once_in_the_order_it_first_ran() {
            let said = set_off(&[rule("B", "first"), rule("A", "a"), rule("B", "second")])
                .expect("something to say");
            assert_eq!(said.matches("`B`").count(), 1, "{said}");
            assert!(said.contains("`B` could not be decided: first"), "{said}");
            assert!(said.find("`B`") < said.find("`A`"), "{said}");
        }
    }

    mod partly {
        use super::*;

        fn rule(name: &str, why: &str) -> Undecided {
            Undecided { rule: name.to_owned(), why: why.to_owned() }
        }

        /// A fan-out that skipped two members left two notes from one rule;
        /// it is one rule that fired, said once, with its first reason.
        #[test]
        fn a_rule_that_left_two_things_unapplied_is_named_once() {
            let said = partly(&[rule("Fan", "Hook#1"), rule("Ring", "b"), rule("Fan", "Hook#2")])
                .expect("something to say");
            assert_eq!(said.matches("`Fan`").count(), 1, "{said}");
            assert_eq!(said.matches("`Ring`").count(), 1, "{said}");
            assert!(
                said.contains("`Fan` fired, but part of what it ensures was not applied: Hook#1")
            );
            assert!(!said.contains("Hook#2"), "{said}");
        }

        #[test]
        fn everything_applied_says_nothing() {
            assert_eq!(partly(&[]), None);
        }
    }

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

        /// Positional arguments count as well as named ones: `Announced(a)`
        /// and `Announced(b)` are two firings, not a loop.
        #[test]
        fn the_same_trigger_carrying_different_things_by_position_is_not() {
            let carrying = |who: u64| {
                Event::new("Announced", "chains").passing(Value::Ref(EntityId::new("Doc", who)))
            };
            assert_eq!(key_of(&carrying(1)), key_of(&carrying(1)));
            assert_ne!(key_of(&carrying(1)), key_of(&carrying(2)));
            assert_ne!(key_of(&carrying(1)), key_of(&Event::new("Announced", "chains")));
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
