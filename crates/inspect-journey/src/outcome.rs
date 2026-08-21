//! Reading a step outcome as a journey verdict.
//!
//! The simulator answers about rules; a journey asks about a line somebody
//! wrote. This is the translation, and the whole of what it adds is care about
//! which of the two undecided-looking answers it is giving: a rule that refused
//! and a rule nobody could evaluate are different things, and only one of them
//! is the specification saying no.

use inspect_sim::{Disposition, Truth, step::StepOutcome};

use crate::check::Verdict;

/// How a step outcome reads as a verdict.
pub(crate) fn verdict_of(outcome: &StepOutcome) -> Verdict {
    // Undecided first, and the order is the whole judgement here. Several
    // rules watch one trigger. One of them *refusing* means its own
    // precondition was not met, which is ordinary and does not undo an act
    // that happened — so `Fired` beats `Refused`. One of them being
    // undecided is different: the simulator could not tell whether it ran, so
    // the world may be missing changes and every assertion after this line is
    // answered against a world that might be wrong.
    //
    // `Fired` used to win outright, which made that case read as satisfied and
    // left `refusal` with nothing to report, because the step had not failed.
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Undecided) {
        return Verdict::Undecided;
    }
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Fired) {
        return Verdict::Specified;
    }
    if outcome.rules.iter().any(|rule| rule.disposition == Disposition::Refused) {
        return Verdict::Refused;
    }
    // Nothing waits for it. The trigger exists — the checker said so — and no
    // rule consumes it, which is a fact about the spec rather than about this
    // world.
    Verdict::Remark
}

/// Why a step did not go through, in the spec's own words.
pub(crate) fn refusal(outcome: &StepOutcome) -> Option<String> {
    for rule in &outcome.rules {
        match rule.disposition {
            Disposition::Refused => {
                let blocking: Vec<&str> = rule
                    .requires
                    .iter()
                    .filter(|clause| clause.truth == Truth::False)
                    .map(|clause| clause.text.as_str())
                    .collect();
                return Some(format!("`{}` refused: {}", rule.name, blocking.join("; ")));
            }
            Disposition::Undecided => {
                let reason = rule.unresolved.first().map_or_else(
                    || "something could not be evaluated".to_owned(),
                    // The sub-expression as well as the reason, when the note
                    // located one. "`Member#1` has no `is_at_limit` set" says
                    // what is missing; it does not say which clause asked, and
                    // a rule with four preconditions has four candidates. The
                    // quote comes from the spec text, which is why the walker
                    // is handed it.
                    |note| match &note.expression {
                        Some(expression) => format!("{} — in `{expression}`", note.reason),
                        None => note.reason.clone(),
                    },
                );
                return Some(format!("`{}` could not be decided: {reason}", rule.name));
            }
            Disposition::Unsimulatable => {
                return Some(format!("`{}`'s clauses did not parse", rule.name));
            }
            Disposition::Fired => {}
        }
    }
    if outcome.rules.is_empty() {
        return Some("no rule waits for this".to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use inspect_sim::{
        step::RuleOutcome,
        world::{Event, World},
    };

    fn rule(name: &str, disposition: Disposition) -> RuleOutcome {
        RuleOutcome {
            rule: format!("lending::rule::{name}"),
            name: name.to_owned(),
            module: "lending".to_owned(),
            disposition,
            requires: Vec::new(),
            effects: Vec::new(),
            unresolved: Vec::new(),
        }
    }

    fn outcome(rules: Vec<RuleOutcome>) -> StepOutcome {
        StepOutcome {
            world: World::new(),
            event: Event::new("MemberBorrows", "lending"),
            rules,
            invariants: Vec::new(),
            newly_enabled: Vec::new(),
            emitted: Vec::new(),
        }
    }

    #[test]
    fn one_rule_firing_does_not_settle_a_trigger_another_rule_left_undecided() {
        // Several rules can wait on one trigger. `Fired` used to win outright,
        // so a step where one rule ran and another could not be evaluated read
        // as *specified* — and `refusal` then had nothing to report, because
        // the step had not failed. The act did happen; part of what should
        // have happened alongside it is unknown, and a world that is partly
        // unknown is exactly what `undecided` is for.
        let both = outcome(vec![
            rule("BorrowCopy", Disposition::Fired),
            rule("NotifyReserver", Disposition::Undecided),
        ]);
        assert_eq!(verdict_of(&both), Verdict::Undecided);

        // Order must not matter: the same two rules the other way round.
        let reversed = outcome(vec![
            rule("NotifyReserver", Disposition::Undecided),
            rule("BorrowCopy", Disposition::Fired),
        ]);
        assert_eq!(verdict_of(&reversed), Verdict::Undecided);
    }

    #[test]
    fn a_trigger_whose_rules_all_fired_is_still_satisfied() {
        // The ordinary case, so the rule above cannot quietly become
        // "everything is undecided".
        let fired = outcome(vec![
            rule("BorrowCopy", Disposition::Fired),
            rule("MarkOnLoan", Disposition::Fired),
        ]);
        assert_eq!(verdict_of(&fired), Verdict::Specified);
    }

    #[test]
    fn another_rule_refusing_does_not_undo_an_act_that_happened() {
        // The asymmetry, which is the point of the order. Several rules watch
        // one trigger. One of them refusing means *its* precondition was not
        // met, which is ordinary and not a failure of the act the journey
        // named — the act happened, because something fired.
        //
        // Undecided is different, and that is why it outranks both: the
        // simulator could not tell whether that rule ran, so the world may be
        // missing changes, and every assertion after it is being answered
        // against a world that might be wrong.
        let mixed = outcome(vec![
            rule("BorrowCopy", Disposition::Fired),
            rule("AtLimit", Disposition::Refused),
        ]);
        assert_eq!(verdict_of(&mixed), Verdict::Specified);

        // With nothing fired, the refusal is the answer.
        let refused = outcome(vec![rule("AtLimit", Disposition::Refused)]);
        assert_eq!(verdict_of(&refused), Verdict::Refused);
    }
}
