//! How a *value* is written in a journey, as against how a clause is.
//!
//! `parse.rs` reads the shape of a file — blocks, steps, the keyword a clause
//! is recognised by. This reads what sits inside one: a path, a literal, a set,
//! a moment. The split is where the two stop needing each other, and it is
//! where the file stopped fitting under the size budget.
//!
//! The one rule worth restating here is the one `clock` exists for. Every other
//! literal a journey can write is absolute, and a timestamp written absolutely
//! is unreadable and wrong the day after it is written. `now + 1.day` is what a
//! journey means, and it is the only way it can say a moment at all.

use inspect_sim::{Value, seed};

use crate::{
    journey::{Path, Term},
    parse::{ParseError, fail, split_arguments},
};

/// `loan.copy.status`
pub(crate) fn path(text: &str, line: usize) -> Result<Path, ParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return fail(line, "expected a name");
    }
    let mut parts = trimmed.split('.').map(str::trim);
    let Some(root) = parts.next().filter(|root| !root.is_empty()) else {
        return fail(line, format!("`{trimmed}` is not a name"));
    };
    let segments: Vec<String> = parts.map(ToOwned::to_owned).collect();
    if segments.iter().any(String::is_empty) {
        return fail(line, format!("`{trimmed}` has an empty field"));
    }
    Ok(Path { root: root.to_owned(), segments })
}

/// A literal, a set of terms, or somewhere to read a value from.
pub(crate) fn term(text: &str, line: usize) -> Result<Term, ParseError> {
    let trimmed = text.trim();
    if let Some(inside) = trimmed.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) {
        let mut items = Vec::new();
        for part in split_arguments(inside) {
            let part = part.trim();
            if !part.is_empty() {
                items.push(term(part, line)?);
            }
        }
        return Ok(Term::Set(items));
    }
    // `seed::literal` is the same reader that gives a config parameter its
    // default, so a journey and a spec agree on what `21.days` and `"Ada"` are.
    if let Some(value) = seed::literal(trimmed) {
        return Ok(Term::Literal(value));
    }
    if let Some(clock) = clock(trimmed, line)? {
        return Ok(clock);
    }
    // Everything else is a path — including a bare word, which is the shape of
    // both a state a spec declares (`available`) and somebody the journey cast
    // (`copy`). Nothing here can tell those apart, and a parser that guessed
    // would pass the *name* `copy` to a rule expecting the copy. The walker
    // knows what the journey has bound, so it decides: a bound name is what it
    // is bound to, and an unbound one is the state it spells.
    Ok(Term::Path(path(trimmed, line)?))
}

/// `now`, `now + 1.day`, `now - 2.hours`, or nothing.
///
/// The one arithmetic the grammar has, and it stops here on purpose. A journey
/// says *when* relative to the clock it can already move with `after`; anything
/// more is an expression language, and this file has spent its whole life not
/// being one — see the note on `Assertion`.
///
/// # Errors
///
/// Returns an error for `now` followed by something that is not a signed
/// duration. Silence there would leave `now + 1.fortnight` reading as the bare
/// name `now`, which resolves to nothing and reports as an unbound cast member
/// — an error about the journey when the fault is in the line.
fn clock(text: &str, line: usize) -> Result<Option<Term>, ParseError> {
    let Some(rest) = text.strip_prefix("now") else { return Ok(None) };

    // `nowhere` opens with the same three letters and is an ordinary word. What
    // follows `now` has to be nothing or an operator, or this claims every name
    // that happens to start that way — and then refuses it, which is an error
    // about the line when there is nothing wrong with it.
    if !rest.is_empty() && !rest.starts_with([' ', '\t', '+', '-']) {
        return Ok(None);
    }

    let rest = rest.trim();

    if rest.is_empty() {
        return Ok(Some(Term::Clock { offset: 0, written: "now".to_owned() }));
    }

    let (sign, amount) = match rest.split_at_checked(1) {
        Some(("+", amount)) => (1, amount.trim()),
        Some(("-", amount)) => (-1, amount.trim()),
        _ => return fail(line, format!("expected `now`, `now + …` or `now - …`, found `{text}`")),
    };

    let Some(Value::Duration(millis)) = seed::literal(amount) else {
        return fail(
            line,
            format!("`{amount}` is not a duration — expected something like `1.day`"),
        );
    };

    Ok(Some(Term::Clock {
        offset: sign * millis,
        written: format!("now {} {amount}", if sign > 0 { "+" } else { "-" }),
    }))
}
