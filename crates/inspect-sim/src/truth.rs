//! Three-valued logic, and why the third value is the point.
//!
//! A specification language is not a programming language. Allium expressions
//! describe intent, they refer to things a simulator has no model of — a black
//! box function, a value the outside world supplies, a collection nobody has
//! populated — and they are written to be read by people as much as machines.
//! Any honest evaluator will meet expressions it cannot decide.
//!
//! There are two ways to handle that and only one of them is defensible.
//!
//! The tempting one is to pick a default: treat what you cannot evaluate as
//! `true` and rules fire that should not have; treat it as `false` and rules are
//! reported as blocked by a precondition nobody checked. Either way the
//! simulator states a conclusion it did not reach, and the reader has no way to
//! tell those apart from the ones it did.
//!
//! The other is to say so. [`Truth::Unknown`] is a real answer, it propagates
//! through the connectives by Kleene's rules, and it survives all the way to the
//! interface — where it is the loudest thing on the panel rather than the
//! quietest, because it is the only verdict that needs a person.
//!
//! Kleene's tables have one property worth stating, because it is what makes
//! them the right choice rather than merely a choice: they preserve the cases
//! where the answer *is* determined by one operand. `false and unknown` is
//! `false`, because a conjunction with a false operand is false whatever the
//! rest turns out to be. So partial knowledge still decides what it can, and
//! only genuinely undetermined results come back undetermined.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether something holds, does not hold, or could not be decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub enum Truth {
    True,
    False,
    /// The simulator could not decide. Never a synonym for false.
    Unknown,
}

impl Truth {
    /// A definite answer.
    #[must_use]
    pub fn from_bool(value: bool) -> Self {
        if value { Truth::True } else { Truth::False }
    }

    /// Conjunction.
    ///
    /// `false and unknown` is `false`: one false operand settles it, whatever
    /// the other turns out to be. This is what keeps partial knowledge useful
    /// instead of contagious.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Truth::False, _) | (_, Truth::False) => Truth::False,
            (Truth::True, Truth::True) => Truth::True,
            _ => Truth::Unknown,
        }
    }

    /// Disjunction. `true or unknown` is `true`, for the mirror reason.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        match (self, other) {
            (Truth::True, _) | (_, Truth::True) => Truth::True,
            (Truth::False, Truth::False) => Truth::False,
            _ => Truth::Unknown,
        }
    }

    /// Implication, as Allium writes it: `a implies b` is `not a or b`.
    #[must_use]
    pub fn implies(self, other: Self) -> Self {
        (!self).or(other)
    }

    /// Whether this is a definite answer either way.
    #[must_use]
    pub fn is_known(self) -> bool {
        self != Truth::Unknown
    }

    /// Whether this blocks a rule.
    ///
    /// Only `false` does. An undecided precondition does not stop the rule —
    /// it means the simulator cannot say whether the rule would fire, which the
    /// step reports as indeterminate rather than as a refusal.
    #[must_use]
    pub fn blocks(self) -> bool {
        self == Truth::False
    }

    /// The conjunction of everything in `parts`.
    ///
    /// An empty sequence is `true`: a rule with no preconditions fires whenever
    /// its trigger does, which is what the language means by omitting them.
    pub fn all(parts: impl IntoIterator<Item = Truth>) -> Self {
        parts.into_iter().fold(Truth::True, Truth::and)
    }

    /// The disjunction of everything in `parts`. Empty is `false`.
    pub fn any(parts: impl IntoIterator<Item = Truth>) -> Self {
        parts.into_iter().fold(Truth::False, Truth::or)
    }
}

/// Negation. `unknown` negates to `unknown`.
///
/// The real trait rather than an inherent method of the same name, so `!truth`
/// works and `truth.not()` keeps reading the way the language does.
impl std::ops::Not for Truth {
    type Output = Self;

    fn not(self) -> Self {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Truth; 3] = [Truth::True, Truth::False, Truth::Unknown];

    #[test]
    fn a_bool_becomes_a_definite_answer() {
        assert_eq!(Truth::from_bool(true), Truth::True);
        assert_eq!(Truth::from_bool(false), Truth::False);
    }

    #[test]
    fn negation_leaves_the_undecided_undecided() {
        assert_eq!(!Truth::True, Truth::False);
        assert_eq!(!Truth::False, Truth::True);
        assert_eq!(!Truth::Unknown, Truth::Unknown);
    }

    #[test]
    fn conjunction_follows_kleenes_table_exactly() {
        use Truth::{False, True, Unknown};
        let table = [
            ((True, True), True),
            ((True, False), False),
            ((True, Unknown), Unknown),
            ((False, True), False),
            ((False, False), False),
            // The case that matters: one false operand settles a conjunction
            // regardless of what the other turns out to be.
            ((False, Unknown), False),
            ((Unknown, True), Unknown),
            ((Unknown, False), False),
            ((Unknown, Unknown), Unknown),
        ];
        for ((left, right), expected) in table {
            assert_eq!(left.and(right), expected, "{left:?} and {right:?}");
        }
    }

    #[test]
    fn disjunction_follows_kleenes_table_exactly() {
        use Truth::{False, True, Unknown};
        let table = [
            ((True, True), True),
            ((True, False), True),
            ((True, Unknown), True),
            ((False, True), True),
            ((False, False), False),
            ((False, Unknown), Unknown),
            ((Unknown, True), True),
            ((Unknown, False), Unknown),
            ((Unknown, Unknown), Unknown),
        ];
        for ((left, right), expected) in table {
            assert_eq!(left.or(right), expected, "{left:?} or {right:?}");
        }
    }

    #[test]
    fn both_connectives_are_commutative() {
        for left in ALL {
            for right in ALL {
                assert_eq!(left.and(right), right.and(left));
                assert_eq!(left.or(right), right.or(left));
            }
        }
    }

    #[test]
    fn implication_is_not_a_or_b() {
        use Truth::{False, True, Unknown};
        assert_eq!(True.implies(True), True);
        assert_eq!(True.implies(False), False);
        assert_eq!(False.implies(False), True, "a false premise implies anything");
        assert_eq!(False.implies(Unknown), True, "and does so even undecided");
        assert_eq!(Unknown.implies(True), True, "a true conclusion needs no premise");
        assert_eq!(Unknown.implies(Unknown), Unknown);
    }

    #[test]
    fn de_morgans_laws_hold() {
        // Not a decoration: they are what says these tables are a coherent
        // logic rather than nine hand-picked answers.
        for left in ALL {
            for right in ALL {
                assert_eq!(!left.and(right), (!left).or(!right));
                assert_eq!(!left.or(right), (!left).and(!right));
            }
        }
    }

    #[test]
    fn only_a_definite_false_blocks_a_rule() {
        // An undecided precondition does not refuse the rule; it means the
        // simulator cannot say. Treating it as a refusal would report a rule as
        // blocked by a condition nothing checked.
        assert!(Truth::False.blocks());
        assert!(!Truth::True.blocks());
        assert!(!Truth::Unknown.blocks());
    }

    #[test]
    fn knownness_distinguishes_the_third_value() {
        assert!(Truth::True.is_known());
        assert!(Truth::False.is_known());
        assert!(!Truth::Unknown.is_known());
    }

    #[test]
    fn a_rule_with_no_preconditions_fires() {
        // Omitting `requires` means "whenever the trigger happens", so the
        // empty conjunction has to be true.
        assert_eq!(Truth::all([]), Truth::True);
    }

    #[test]
    fn an_empty_disjunction_is_false() {
        assert_eq!(Truth::any([]), Truth::False);
    }

    #[test]
    fn all_and_any_fold_the_tables() {
        use Truth::{False, True, Unknown};
        assert_eq!(Truth::all([True, True, True]), True);
        assert_eq!(Truth::all([True, Unknown]), Unknown);
        assert_eq!(Truth::all([True, Unknown, False]), False, "a false anywhere settles it");
        assert_eq!(Truth::any([False, Unknown]), Unknown);
        assert_eq!(Truth::any([False, Unknown, True]), True);
    }

    #[test]
    fn truth_serialises_in_lower_case_for_the_wire() {
        let json = serde_json::to_string(&Truth::Unknown).expect("serialises");
        assert_eq!(json, "\"unknown\"");
        assert_eq!(serde_json::from_str::<Truth>("\"false\"").expect("parses"), Truth::False);
    }
}
