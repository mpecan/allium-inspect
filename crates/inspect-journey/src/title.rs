//! A journey's name, made readable.
//!
//! `SheReadsWhatArrivedAndHeIsToldSheDid` is an identifier and has to be: an
//! evidence marker says `// journey: SheReadsWhatArrivedAndHeIsToldSheDid.3`,
//! and the panel finds a walk by name alone. It is also a sentence, written in
//! the one style a name can be written in, and reading a column of them is
//! work nobody should be doing.
//!
//! So the name stays the name and this is what a heading shows.
//!
//! **Spaces only. Nothing is re-cased**, and that is a decision rather than an
//! omission. Sentence case reads better — "she reads what arrived and he is
//! told she did" — right up until a journey is about Ada, and lowercasing it
//! would need this to know that `Ada` is a person and `And` is a word. It
//! cannot know, the specification never said, and inventing an answer for the
//! cases it gets wrong is not worth the cases it gets right. A capital the
//! author typed is a capital the author meant.

/// `SheStandsUpAPiAndPointsARoomAtIt` → `She Stands Up A Pi And Points A Room At It`.
#[must_use]
pub fn readable(name: &str) -> String {
    let letters: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + name.len() / 4);

    for (at, letter) in letters.iter().enumerate() {
        if at > 0 && breaks_before(&letters, at) {
            out.push(' ');
        }
        out.push(*letter);
    }
    out
}

/// Whether a word begins at `at`.
///
/// Two boundaries, and the second is the one a naive split gets wrong.
///
/// A capital after a small letter or a digit opens a word: `ReadsWhat`,
/// `Arc2Begins`. And a capital that is the *last* of a run, with a small
/// letter after it, opens one too: `APIKey` is `API Key` and not `A P I Key`,
/// because a run of capitals is an initialism and belongs together.
fn breaks_before(letters: &[char], at: usize) -> bool {
    let here = letters[at];
    if !here.is_uppercase() {
        return false;
    }
    let before = letters[at - 1];
    if before.is_lowercase() || before.is_numeric() {
        return true;
    }
    // Inside a run of capitals: only the last of them starts a word.
    before.is_uppercase() && letters.get(at + 1).is_some_and(|after| after.is_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_the_sentence_it_was_written_as() {
        assert_eq!(
            readable("SheReadsWhatArrivedAndHeIsToldSheDid"),
            "She Reads What Arrived And He Is Told She Did"
        );
        assert_eq!(readable("OnePersonTwoMachines"), "One Person Two Machines");
    }

    /// Single letters are words here — `A Pi`, `A Room` — and that is right:
    /// the author wrote them as words.
    #[test]
    fn a_single_letter_word_is_a_word() {
        assert_eq!(
            readable("SheStandsUpAPiAndPointsARoomAtIt"),
            "She Stands Up A Pi And Points A Room At It"
        );
    }

    /// A run of capitals is an initialism, and splitting it letter by letter
    /// would turn `APIKeyRotates` into `A P I Key Rotates`.
    #[test]
    fn a_run_of_capitals_stays_together() {
        assert_eq!(readable("APIKeyRotates"), "API Key Rotates");
        assert_eq!(readable("SheReadsTheURL"), "She Reads The URL");
        assert_eq!(readable("URLIsShown"), "URL Is Shown");
    }

    #[test]
    fn a_digit_ends_a_word() {
        assert_eq!(readable("Arc2Begins"), "Arc2 Begins");
    }

    /// Nothing is re-cased. Lowercasing would have to decide that `Ada` is a
    /// person and `And` is a word, and the specification never said.
    #[test]
    fn a_capital_the_author_typed_is_kept() {
        assert_eq!(readable("BrunoAsksAda"), "Bruno Asks Ada");
    }

    #[test]
    fn a_name_with_nowhere_to_break_is_itself() {
        assert_eq!(readable("Arriving"), "Arriving");
        assert_eq!(readable(""), "");
        assert_eq!(readable("A"), "A");
    }

    /// A name already carrying spaces is left as it is rather than doubled.
    #[test]
    fn a_space_the_author_wrote_is_not_joined_by_another() {
        assert_eq!(readable("She Reads"), "She Reads");
    }
}
