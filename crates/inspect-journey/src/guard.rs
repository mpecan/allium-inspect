//! `when` guards on an `exposes` item.
//!
//! `doc.secret when doc.is_private = false` is the language's way of saying a
//! field is conditionally visible: the item is shown while the guard holds and
//! hidden while it does not. The walk once read a guarded item as never shown
//! at all, so `sees` came back unexposed and `cannot see` held with nothing
//! checked — the privacy claim that passed because nobody looked.
//!
//! The guard is read against the world from where the item is read, which is
//! the same scope a `for … where` filter gets: the surface's contexts, whoever
//! it faces, its `let`s, and inside a `for` body the element being visited.

use std::collections::BTreeMap;

use allium_parser::ast::Expr;
use inspect_sim::{Value, eval::span_of};

use crate::{assert::Admission, run::Walker};

impl Walker<'_> {
    /// Whether `action when condition` shows what is being asked about, given
    /// whether `action` alone would.
    ///
    /// An item about something else is not shown whatever its guard says;
    /// `combine` is the one place that decides so, and the guard read for it
    /// is read for nothing.
    pub(crate) fn guarded(
        &self,
        (action, condition): (&Expr, &Expr),
        shown: Admission,
        context: &BTreeMap<String, Value>,
        module: &str,
    ) -> Admission {
        let source = self.sources.get(module).map_or("", String::as_str);
        let written = quoted(source, action, condition)
            .map_or_else(|| "its `when` guard".to_owned(), |text| format!("`when {text}`"));
        combine(shown, &written, self.guard_holds(condition, context, module))
    }

    /// Whether the guard holds here, or why that could not be settled.
    fn guard_holds(
        &self,
        condition: &Expr,
        context: &BTreeMap<String, Value>,
        module: &str,
    ) -> Result<bool, String> {
        match self.evaluate(condition, context, module) {
            // Decided is decided. A value computed over a collection that
            // dropped members nothing could decide — `(docs where is_private =
            // true).count > 0` with one `is_private` unset — comes back
            // undecided from the evaluator itself, so what reaches here as a
            // boolean is one no dropped member could overturn.
            Err(why) => Err(why),
            Ok((Value::Bool(holds), _)) => Ok(holds),
            Ok((other, _)) => Err(format!("it came back {}, not true or false", other.described())),
        }
    }
}

/// The guard as the author wrote it, sliced from the source by byte span.
///
/// From the end of the item rather than the start of the condition, because
/// the parser leaves parentheses out of an expression's span: the condition of
/// `shelf.name when (shelf.docs where …).count > 0` starts after its `(`, and
/// quoting from there shows the reader a line they did not write. A `)` the
/// span stops short of is taken back for the same reason.
fn quoted(source: &str, action: &Expr, condition: &Expr) -> Option<String> {
    quoted_between(source, span_of(action)?.end, span_of(condition)?.end)
}

/// [`quoted`], from the byte where the item ends to the byte where the
/// condition's span ends.
fn quoted_between(source: &str, from: usize, to: usize) -> Option<String> {
    let between = source.get(from..to)?;
    let written = between.trim_start_matches(|c: char| c == ')' || c.is_whitespace());
    let mut written = written.strip_prefix("when")?.trim_start().to_owned();
    let mut unclosed = written.matches('(').count().saturating_sub(written.matches(')').count());
    for c in source.get(to..)?.chars() {
        match c {
            _ if unclosed == 0 => break,
            ')' => unclosed -= 1,
            c if c.is_whitespace() => {}
            _ => break,
        }
        written.push(c);
    }
    Some(written.trim_end().to_owned())
}

/// What a guarded item shows, from what the item would show unguarded and
/// whether its guard — quoted as `written` — holds.
///
/// A false guard hides the item whatever else is unsettled about it — hidden
/// is hidden — and says which guard, because "nothing shows it" sends an
/// author to the surface when the answer is in their `given:`. A guard nothing
/// could settle leaves a *yes* undecided, and that is the case this exists
/// for: answering either way would be stating a conclusion the walk did not
/// reach.
fn combine(shown: Admission, written: &str, guard: Result<bool, String>) -> Admission {
    match (shown, guard) {
        (hidden @ (Admission::No | Admission::Hidden(_)), _) => hidden,
        (_, Ok(false)) => Admission::Hidden(format!("only {written}, which does not hold here")),
        (shown, Ok(true)) => shown,
        // The item's own reason first: it was unsettled before the guard was.
        (Admission::Undecided(why), Err(_)) => Admission::Undecided(why),
        (Admission::Yes, Err(why)) => {
            Admission::Undecided(format!("{written} could not be settled: {why}"))
        }
    }
}

impl Admission {
    /// The stronger of two answers about the same question from two places —
    /// two items of a clause, two lines of a `for` body, two elements.
    ///
    /// Shown anywhere is shown. Otherwise undecided anywhere is undecided: a
    /// *no* beside something nobody could read is a claim nothing checked.
    /// Then hidden, because a guard that hid it is the one thing a reader can
    /// act on; and the first reason of a kind is the one kept.
    pub(crate) fn or(self, other: Admission) -> Admission {
        fn rank(admission: &Admission) -> u8 {
            match admission {
                Admission::No => 0,
                Admission::Hidden(_) => 1,
                Admission::Undecided(_) => 2,
                Admission::Yes => 3,
            }
        }
        if rank(&other) > rank(&self) { other } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARD: &str = "`when doc.is_private = false`";

    /// `source` quoted as the parser spans it: from the end of `item` to the
    /// end of `condition`, which stops short of any closing parenthesis.
    fn quote(source: &str, item: &str, condition: &str) -> Option<String> {
        let from = source.find(item)? + item.len();
        let to = source.find(condition)? + condition.len();
        quoted_between(source, from, to)
    }

    #[test]
    fn a_closing_parenthesis_the_span_stopped_short_of_is_taken_back() {
        let source = "doc.secret when (doc.is_private = false)\n";
        assert_eq!(
            quote(source, "doc.secret", "doc.is_private = false").as_deref(),
            Some("(doc.is_private = false)")
        );
    }

    /// Only as many as were opened: a `)` past the last one closes something
    /// the guard is not part of.
    #[test]
    fn no_more_parentheses_are_taken_back_than_the_guard_opened() {
        let source = "(doc.secret when (doc.is_private = false))";
        assert_eq!(
            quote(source, "doc.secret", "doc.is_private = false").as_deref(),
            Some("(doc.is_private = false)")
        );
    }

    #[test]
    fn whitespace_before_a_closing_parenthesis_is_kept_as_written() {
        let source = "doc.secret when ( doc.is_private = false )";
        assert_eq!(
            quote(source, "doc.secret", "doc.is_private = false").as_deref(),
            Some("( doc.is_private = false )")
        );
    }

    /// Nothing but whitespace and closers is taken back: the next word is the
    /// next item, not part of the guard.
    #[test]
    fn taking_back_stops_at_anything_but_whitespace_and_a_closer() {
        let source = "doc.secret when (doc.is_private = false\n    doc.owner";
        assert_eq!(
            quote(source, "doc.secret", "doc.is_private = false").as_deref(),
            Some("(doc.is_private = false")
        );
    }

    #[test]
    fn a_balanced_guard_takes_nothing_back() {
        let source = "doc.secret when doc.is_private = false)";
        assert_eq!(
            quote(source, "doc.secret", "doc.is_private = false").as_deref(),
            Some("doc.is_private = false")
        );
    }

    fn unsettled(why: &str) -> Result<bool, String> {
        Err(why.to_owned())
    }

    fn reason(admission: &Admission) -> Option<&str> {
        match admission {
            Admission::Undecided(why) | Admission::Hidden(why) => Some(why),
            _ => None,
        }
    }

    #[test]
    fn a_true_guard_leaves_the_answer_as_it_was() {
        assert!(matches!(combine(Admission::Yes, GUARD, Ok(true)), Admission::Yes));
        assert!(matches!(combine(Admission::No, GUARD, Ok(true)), Admission::No));
        let still = combine(Admission::Undecided("the item".to_owned()), GUARD, Ok(true));
        assert!(matches!(still, Admission::Undecided(_)));
        assert_eq!(reason(&still), Some("the item"));
    }

    #[test]
    fn a_false_guard_hides_the_item_and_names_the_guard() {
        let hidden = combine(Admission::Yes, GUARD, Ok(false));
        assert!(matches!(hidden, Admission::Hidden(_)));
        assert_eq!(
            reason(&hidden),
            Some("only `when doc.is_private = false`, which does not hold here")
        );
        assert!(matches!(
            combine(Admission::Undecided("the item".to_owned()), GUARD, Ok(false)),
            Admission::Hidden(_)
        ));
    }

    /// The case the issue was about, pointed both ways: a shown item behind a
    /// guard nothing settled is neither shown nor hidden.
    #[test]
    fn an_unsettled_guard_turns_a_yes_into_undecided_naming_the_guard() {
        let answer = combine(Admission::Yes, GUARD, unsettled("the guard"));
        assert!(matches!(answer, Admission::Undecided(_)));
        assert_eq!(
            reason(&answer),
            Some("`when doc.is_private = false` could not be settled: the guard")
        );
    }

    #[test]
    fn an_unsettled_guard_keeps_the_items_own_reason() {
        let answer =
            combine(Admission::Undecided("the item".to_owned()), GUARD, unsettled("the guard"));
        assert_eq!(reason(&answer), Some("the item"));
    }

    /// An item about something else is not shown, and its guard is no reason
    /// to say otherwise; an item an inner guard hid stays hidden by it.
    #[test]
    fn an_item_that_is_not_shown_stays_not_shown() {
        assert!(matches!(combine(Admission::No, GUARD, unsettled("the guard")), Admission::No));
        assert!(matches!(combine(Admission::No, GUARD, Ok(false)), Admission::No));
        let inner = combine(Admission::Hidden("inner".to_owned()), GUARD, Ok(true));
        assert_eq!(reason(&inner), Some("inner"));
    }

    #[test]
    fn shown_beats_undecided_beats_hidden_beats_no() {
        let undecided = || Admission::Undecided("u".to_owned());
        let hidden = || Admission::Hidden("h".to_owned());
        assert!(matches!(Admission::No.or(hidden()), Admission::Hidden(_)));
        assert!(matches!(hidden().or(Admission::No), Admission::Hidden(_)));
        assert!(matches!(hidden().or(undecided()), Admission::Undecided(_)));
        assert!(matches!(undecided().or(hidden()), Admission::Undecided(_)));
        assert!(matches!(undecided().or(Admission::Yes), Admission::Yes));
        assert!(matches!(Admission::Yes.or(undecided()), Admission::Yes));
        let first = Admission::Hidden("first".to_owned()).or(Admission::Hidden("second".into()));
        assert_eq!(reason(&first), Some("first"));
    }
}
