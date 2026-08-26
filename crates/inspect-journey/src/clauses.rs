//! One line under a step, and what it says.
//!
//! `parse.rs` reads the shape of a file — blocks, steps, which keyword opens
//! what. This reads a clause, and a clause is recognised by a keyword it
//! *contains* rather than one it starts with: `ada does …`, `ada sees …`,
//! `then …`. That is deliberate — a person writing one writes a sentence, and
//! a sentence puts the actor first.
//!
//! Order matters in one place and it is worth knowing about. `cannot do` is
//! looked for before `does` and `cannot see` before `sees`, because the
//! negative forms contain no `does` or `sees` of their own but the reverse
//! order would still be a trap worth not setting.

use inspect_sim::{Value, seed};

use crate::{
    journey::{Assertion, Clause, Comparison, Subject, Term},
    parse::{ParseError, cast, fail},
    terms::{path, term},
};

/// One line under a step.
pub(crate) fn clause(text: &str, line: usize) -> Result<Clause, ParseError> {
    if let Some(rest) = text.strip_prefix("after ") {
        let written = rest.trim();
        let Some(duration @ Value::Duration(_)) = seed::literal(written) else {
            return fail(line, format!("`{written}` is not a duration"));
        };
        return Ok(Clause::After { duration, text: written.to_owned(), line });
    }
    if let Some(rest) = text.strip_prefix("then ") {
        return Ok(Clause::Then { assertion: assertion(rest.trim(), line)?, line });
    }
    if let Some(rest) = text.strip_prefix("stipulate ") {
        let Some((left, right)) = split_once_operator(rest, "=") else {
            return fail(
                line,
                "expected `stipulate <path> = <value>` or `stipulate <call> = <value>`",
            );
        };
        return Ok(Clause::Stipulate {
            subject: subject(left.trim(), line)?,
            value: term(right, line)?,
            line,
        });
    }
    // Before `does`, because `cannot do` contains no `does` but the reverse
    // order would still be a trap worth not setting.
    if let Some((actor, rest)) = text.split_once(" cannot do ") {
        return does(actor.trim(), rest.trim(), true, line);
    }
    if let Some((actor, rest)) = text.split_once(" does ") {
        return does(actor.trim(), rest.trim(), false, line);
    }
    if let Some((actor, rest)) = text.split_once(" cannot see ") {
        return sees(actor.trim(), rest.trim(), true, line);
    }
    if let Some((actor, rest)) = text.split_once(" sees ") {
        return sees(actor.trim(), rest.trim(), false, line);
    }
    fail(line, format!("no clause here: `{text}`"))
}

/// `MemberBorrows(ada, copy) on MemberShelf creating loan: Loan`
fn does(actor: &str, rest: &str, negated: bool, line: usize) -> Result<Clause, ParseError> {
    let (rest, creating) = match rest.split_once(" creating ") {
        Some((head, caught)) => (head.trim(), Some(cast(caught.trim(), line)?)),
        None => (rest, None),
    };
    // An act the spec refuses makes nothing, so there is nothing to catch. A
    // name bound to whatever happened to be created *anyway* would be the
    // worst of both.
    if negated && creating.is_some() {
        return fail(line, "an act that cannot happen creates nothing to name");
    }
    let Some((call, surface)) = rest.rsplit_once(" on ") else {
        return fail(line, format!("expected `… on <Surface>`, found `{rest}`"));
    };
    let (trigger, arguments) = call_parts(call.trim(), line)?;
    let surface = surface.trim();
    if surface.is_empty() {
        return fail(line, "an act happens on a surface, and this one names none");
    }
    // Otherwise this parses as a surface with a space in its name, and the
    // reader is told there is no surface called `GroupMembers in room`.
    if let Some((surface, _)) = surface.split_once(" in ") {
        return fail(
            line,
            format!(
                "an act names the surface it happens on, and `{surface}` is it — `in` says which \
                 one somebody is *looking* at, and an act says that with its arguments"
            ),
        );
    }
    Ok(Clause::Does {
        actor: actor.to_owned(),
        trigger,
        arguments,
        surface: surface.to_owned(),
        creating,
        negated,
        line,
    })
}

/// `loan.status on MemberShelf`, or `proposal.decision on GroupMembers in room`
fn sees(actor: &str, rest: &str, negated: bool, line: usize) -> Result<Clause, ParseError> {
    let Some((seen, surface)) = rest.rsplit_once(" on ") else {
        return fail(line, format!("expected `… on <Surface>`, found `{rest}`"));
    };
    // Split off the tail rather than the whole line: `in` is a word a subject
    // may contain and a surface name is one word, so the only ` in ` that can
    // be this one is the one after the surface.
    let tail = surface.trim();
    let (surface, context) = match tail.split_once(" in ") {
        Some((surface, context)) => (surface.trim(), Some(context.trim())),
        // The line was trimmed before it got here, so a trailing `in` has
        // nothing after it to split on — and left alone it becomes a surface
        // with a space in its name.
        None => match tail.strip_suffix(" in") {
            Some(surface) => (surface.trim(), Some("")),
            None => (tail, None),
        },
    };
    if surface.is_empty() {
        return fail(line, "somebody looking is looking at a surface, and this one names none");
    }
    if context.is_some_and(str::is_empty) {
        return fail(line, "`in` says which one they are looking at, and this one names nothing");
    }
    Ok(Clause::Sees {
        actor: actor.to_owned(),
        subject: subject(seen.trim(), line)?,
        surface: surface.to_owned(),
        context: context.map(ToOwned::to_owned),
        negated,
        line,
    })
}

/// `MemberBorrows(ada, copy)`
fn call_parts(text: &str, line: usize) -> Result<(String, Vec<Term>), ParseError> {
    let Some((name, rest)) = text.split_once('(') else {
        return fail(line, format!("expected `<Trigger>(…)`, found `{text}`"));
    };
    let Some(inside) = rest.strip_suffix(')') else {
        return fail(line, "an act's arguments are closed with `)`");
    };
    let name = name.trim();
    if name.is_empty() {
        return fail(line, "an act needs a trigger");
    }

    let mut arguments = Vec::new();
    for part in split_arguments(inside) {
        let part = part.trim();
        if !part.is_empty() {
            arguments.push(term(part, line)?);
        }
    }
    Ok((name.to_owned(), arguments))
}

/// Split on commas that are not inside a `{…}` set.
pub(crate) fn split_arguments(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for character in text.chars() {
        match character {
            '{' => {
                depth += 1;
                current.push(character);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    parts.push(current);
    parts
}

/// `loan.status = open`, `BorrowCopy fires`, `copy in entry.awaiting`.
fn assertion(text: &str, line: usize) -> Result<Assertion, ParseError> {
    if let Some(rule) = text.strip_suffix(" does not fire") {
        return Ok(Assertion::Fires { rule: rule.trim().to_owned(), negated: true });
    }
    if let Some(rule) = text.strip_suffix(" fires") {
        return Ok(Assertion::Fires { rule: rule.trim().to_owned(), negated: false });
    }
    if let Some(rest) = text.strip_suffix(" does not exist") {
        return Ok(Assertion::Exists { path: path(rest, line)?, negated: true });
    }
    if let Some(rest) = text.strip_suffix(" exists") {
        return Ok(Assertion::Exists { path: path(rest, line)?, negated: false });
    }
    if let Some((needle, haystack)) = text.split_once(" in ") {
        return Ok(Assertion::Within {
            needle: term(needle, line)?,
            haystack: path(haystack, line)?,
        });
    }
    // Longest first: `<=` must not be read as `<`.
    for (symbol, operator) in [
        ("!=", Comparison::NotEqual),
        ("<=", Comparison::LessOrEqual),
        (">=", Comparison::GreaterOrEqual),
        ("=", Comparison::Equal),
        ("<", Comparison::Less),
        (">", Comparison::Greater),
    ] {
        if let Some((left, right)) = split_once_operator(text, symbol) {
            return Ok(Assertion::Compare {
                left: path(left, line)?,
                operator,
                right: term(right, line)?,
            });
        }
    }
    fail(line, format!("this asserts nothing: `{text}`"))
}

/// Split on the first `symbol` that is not part of a longer operator.
pub(crate) fn split_once_operator<'a>(text: &'a str, symbol: &str) -> Option<(&'a str, &'a str)> {
    let at = text.find(symbol)?;
    let before = text[..at].chars().next_back();
    let after = text[at + symbol.len()..].chars().next();
    // `!=`, `<=` and `>=` all end in `=`, so a bare `=` must not split them.
    if symbol == "=" && (matches!(before, Some('!' | '<' | '>')) || after == Some('=')) {
        return None;
    }
    Some((&text[..at], &text[at + symbol.len()..]))
}

/// What a `stipulate` or `sees` line is about: a path, or a call.
///
/// A call is told apart by its shape rather than by looking it up, because the
/// whole reason to write one is that the specification names a function it
/// never defines — there is nothing to look it up in.
pub(crate) fn subject(text: &str, line: usize) -> Result<Subject, ParseError> {
    let Some((name, rest)) = text.split_once('(') else {
        return Ok(Subject::Path(path(text, line)?));
    };
    let Some(inside) = rest.strip_suffix(')') else {
        return fail(line, format!("`{text}` opens a call and does not close it"));
    };

    let name = name.trim();
    if name.is_empty() {
        return fail(line, "a call needs a name");
    }

    let mut arguments = Vec::new();
    for part in split_arguments(inside) {
        let part = part.trim();
        if !part.is_empty() {
            arguments.push(term(part, line)?);
        }
    }

    Ok(Subject::Call { name: name.to_owned(), arguments })
}
