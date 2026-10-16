//! A simulator reason, said about the names the journey cast.
//!
//! The evaluator knows instances by the world's ids — "`Run#2` has no
//! `archived` set" — and the person reading a `then` line wrote `s`. With two
//! runs in the cast, mapping one to the other is work the reader should not
//! have to do, and [`crate::facing`] already does it for the same reason on a
//! surface's lines. Without this the one fact read one way there and another
//! way on the `then` line below it.

use inspect_sim::Unresolved;

use crate::run::Walker;

impl Walker<'_> {
    /// `note`'s reason with every instance the journey named called by that
    /// name, and the sub-expression it is about when the reason does not
    /// already quote it.
    pub(crate) fn said_of_the_cast(&self, note: &Unresolved) -> String {
        let mut reason = note.reason.clone();
        for (name, id) in &self.bound {
            // Alone, and at the head of a path: "`Box#1.out` was set to …".
            for (world, said) in
                [(format!("`{id}`"), format!("`{name}`")), (format!("`{id}."), format!("`{name}."))]
            {
                if reason.contains(&world) {
                    reason = reason.replace(&world, &said);
                }
            }
        }
        match &note.expression {
            // "`r` has no `other` set — in `other`" says `other` twice.
            Some(expression) if !reason.contains(&format!("`{expression}`")) => {
                format!("{reason} — in `{expression}`")
            }
            _ => reason,
        }
    }
}
