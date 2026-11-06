//! Whether a relationship's name says it holds one entity or several.
//!
//! Allium has no syntax for it. `invitation: Invitation with candidacy = this`
//! and `slots: InterviewSlot with candidacy = this` are written the same way,
//! and the language reference settles the difference by the *name*: singular
//! is at most one entity, or `null`; plural is a collection. Nothing in the
//! recorded `model` output carries it either, so it is read here, once, at
//! ingestion.
//!
//! Read as a set, a singular relationship gave wrong answers rather than
//! undecided ones: `high in allowance.levels` became membership in a set of
//! sets, which is `false` (issue #39).

/// Plurals that do not end in `s`, and words used as a collection whatever
/// their ending. Read as plural, so they keep the collection reading.
const IRREGULAR: &[&str] = &[
    "alumni",
    "children",
    "criteria",
    "data",
    "feet",
    "geese",
    "media",
    "men",
    "mice",
    "people",
    "personnel",
    "staff",
    "teeth",
    "women",
];

/// Singular words that end in `s`: `status`, `address`, `analysis`.
const SINGULAR_ENDINGS: &[&str] = &["ss", "us", "is"];

/// Whether `field` names at most one entity rather than a collection.
///
/// The last word decides, so `billing_address` is one and `open_tickets` is
/// several. A plural this does not recognise is read as singular, and that is
/// the safe side to err on: a singular relationship that matches more than one
/// entity is reported as a specification error, not quietly answered.
#[must_use]
pub fn names_one(field: &str) -> bool {
    let last = field.rsplit('_').next().unwrap_or(field).to_ascii_lowercase();
    if IRREGULAR.contains(&last.as_str()) {
        return false;
    }
    if last.ends_with('s') {
        return SINGULAR_ENDINGS.iter().any(|ending| last.ends_with(ending));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_noun_is_one() {
        assert!(names_one("allowance"));
        assert!(names_one("invitation"));
        assert!(names_one("current_plan"));
    }

    #[test]
    fn a_regular_plural_is_several() {
        assert!(!names_one("slots"));
        assert!(!names_one("feedback_requests"));
        assert!(!names_one("replies"));
        assert!(!names_one("Boxes"));
    }

    #[test]
    fn a_singular_ending_in_s_is_one() {
        assert!(names_one("status"));
        assert!(names_one("billing_address"));
        assert!(names_one("analysis"));
    }

    #[test]
    fn an_irregular_plural_is_several() {
        assert!(!names_one("children"));
        assert!(!names_one("key_people"));
        assert!(!names_one("staff"));
    }
}
