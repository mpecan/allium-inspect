//! Reading a journey file.
//!
//! Line-oriented and deliberately small. The grammar is shaped like Allium's —
//! blocks, keyword clauses, `--` comments — so that a reader who has one in
//! their head has both, and so that graduating a journey into the language
//! later is a rename rather than a rewrite.
//!
//! Two rules make it unambiguous without a lexer:
//!
//! - a **step** is a line beginning `<number>.`, and everything after it until
//!   the next such line is that step's clauses. The number is required and
//!   never renumbered: other documents cite journey steps by number, and a
//!   number that shifts when somebody inserts a step is a citation that quietly
//!   starts pointing at the wrong thing.
//! - a **clause** is recognised by a keyword it contains — `does`, `sees`,
//!   `after`, `then`, `stipulate` — rather than by its position, so that a file
//!   somebody reformatted still reads the same. Indentation does one job only:
//!   a line indented deeper than the clause above it *continues* that clause,
//!   which is how an act with four arguments and a `creating` gets to be three
//!   readable lines instead of one long one.
//!
//! Every error carries the line, because the first thing anybody does with one
//! is go back to it.

use crate::{
    clauses::{clause, split_once_operator},
    journey::{Axis, Cast, Given, Journey, Shared, Step, Term},
    terms::{path, term},
};

/// A journey file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn fail<T>(line: usize, message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { line, message: message.into() })
}

/// Every journey in one file.
///
/// # Errors
///
/// Returns the first line that could not be read, and why.
pub fn parse(source: &str) -> Result<Vec<Journey>, ParseError> {
    let mut journeys: Vec<Journey> = Vec::new();
    let mut shared: Option<Shared> = None;
    fn numbered((at, text): (usize, &str)) -> (usize, &str) {
        (at + 1, text)
    }
    let mut lines: Lines<'_> = source.lines().enumerate().map(numbered as _).peekable();

    while let Some((line, text)) = lines.next() {
        let trimmed = strip_comment(text);
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "world {" {
            // One per file. Two would leave a reader working out which one a
            // journey took by counting lines.
            if shared.is_some() {
                return fail(line, "a file lays out one world, and this is the second");
            }
            // Before the journeys that take it, because a world declared below
            // them would change what they mean from further down the page —
            // and reading order is the only order there is here.
            if !journeys.is_empty() {
                return fail(
                    line,
                    "a world is laid out before the journeys that take it, and this file \
                     already has one above",
                );
            }
            shared = Some(world(line, &mut lines)?);
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("journey ") else {
            return fail(line, format!("expected `journey <Name> {{`, found `{trimmed}`"));
        };
        let Some(name) = rest.strip_suffix('{').map(str::trim) else {
            return fail(line, "a journey opens with `journey <Name> {`");
        };
        if name.is_empty() {
            return fail(line, "a journey needs a name");
        }
        let mut journey = body(name, line, &mut lines)?;
        // `inherits` is `Some` only where there is something to inherit *and*
        // the journey did not decline it. `body` sets it to a marker world with
        // line 0 to mean "did not decline"; here is where it becomes real.
        journey.inherits = journey.inherits.and(shared.clone());
        journeys.push(journey);
    }
    Ok(journeys)
}

/// Everything between the braces of a file's `world`.
///
/// `cast:` and `given:` and nothing else. A world says who is there and how
/// things stand; a step is something somebody *does*, and a world that could
/// act would be a journey nobody named.
fn world(opened: usize, lines: &mut Lines<'_>) -> Result<Shared, ParseError> {
    let mut shared = Shared { cast: Vec::new(), given: Vec::new(), line: opened };
    let mut block = Block::None;

    for (line, text) in lines.by_ref() {
        let trimmed = strip_comment(text);
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "}" {
            return Ok(shared);
        }
        if trimmed == "cast:" {
            block = Block::Cast;
            continue;
        }
        if trimmed == "given:" {
            block = Block::Given;
            continue;
        }
        match block {
            Block::Cast => shared.cast.push(cast(trimmed, line)?),
            Block::Given => shared.given.push(given(trimmed, line)?),
            _ => {
                return fail(
                    line,
                    format!("a world holds `cast:` and `given:`, and found `{trimmed}`"),
                );
            }
        }
    }

    fail(opened, "the world is never closed")
}

type Numbering<'a> = fn((usize, &'a str)) -> (usize, &'a str);
type Lines<'a> =
    std::iter::Peekable<std::iter::Map<std::iter::Enumerate<std::str::Lines<'a>>, Numbering<'a>>>;

/// Everything between the braces of one journey.
fn body(name: &str, opened: usize, lines: &mut Lines<'_>) -> Result<Journey, ParseError> {
    let mut journey = Journey {
        name: name.to_owned(),
        goal: Vec::new(),
        cast: Vec::new(),
        shows: Vec::new(),
        given: Vec::new(),
        // A marker, replaced in `parse` by the file's world if there is one.
        // `Some` here means "did not decline"; `world: none` clears it, and
        // that is the only way to tell a journey that opted out from one whose
        // file simply has no world — which matters, because the second is
        // silence and the first is a decision.
        inherits: Some(Shared { cast: Vec::new(), given: Vec::new(), line: 0 }),
        after: None,
        steps: Vec::new(),
        ends: Vec::new(),
        line: opened,
    };
    // The clause being read, which may still gain wrapped lines.
    let mut pending: Option<Pending> = None;
    // Which block's continuation lines we are inside. `goal:` and `ends:` run
    // on across lines because they are prose; `cast:` and `given:` because they
    // are lists.
    let mut block = Block::None;

    for (line, text) in lines.by_ref() {
        let trimmed = strip_comment(text);
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "}" {
            flush(&mut pending, &mut journey)?;
            return Ok(journey);
        }

        if let Some(rest) = trimmed.strip_prefix("goal:") {
            block = Block::Goal;
            push_prose(&mut journey.goal, rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("ends:") {
            block = Block::Ends;
            push_prose(&mut journey.ends, rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("after:") {
            // `after` is also a clause — `after 1.hour` — and the two are told
            // apart the way every block key is: a colon, and a place above the
            // steps rather than under one. Worth knowing about rather than
            // renaming around, because `after` is the word for both.
            let named = rest.trim();
            if named.is_empty() {
                return fail(line, "a journey follows one named journey, and this names none");
            }
            if journey.after.is_some() {
                return fail(line, "a journey continues from one journey, and this is the second");
            }
            journey.after = Some(named.to_owned());
            block = Block::None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("world:") {
            // The one thing a journey can say about the file's world, because
            // it is the only thing it needs to: take it, or start from nothing.
            // Anything else here would be a second grammar for laying a world
            // out, in the place a journey declines one.
            if rest.trim() != "none" {
                return fail(
                    line,
                    "a journey takes the file's world or says `world: none`, and nothing else",
                );
            }
            journey.inherits = None;
            block = Block::None;
            continue;
        }
        if trimmed == "cast:" {
            block = Block::Cast;
            continue;
        }
        if trimmed == "shows:" {
            block = Block::Shows;
            continue;
        }
        if trimmed == "given:" {
            block = Block::Given;
            continue;
        }
        if let Some((number, title)) = step_header(trimmed) {
            flush(&mut pending, &mut journey)?;
            block = Block::Steps;
            journey.steps.push(Step { number, title, clauses: Vec::new(), line });
            continue;
        }

        match block {
            Block::Goal | Block::Ends => {
                // Prose runs on across lines, which means a clause written
                // before any step would be quietly absorbed into it — and a
                // journey that runs with fewer assertions than somebody wrote
                // is the false green this whole design exists to refuse. The
                // three words below open a clause and open nothing else.
                if let Some(word) = trimmed.split_whitespace().next()
                    && matches!(word, "then" | "after" | "stipulate")
                {
                    return fail(
                        line,
                        format!("`{word}` opens a clause, and a clause belongs under a step"),
                    );
                }
                let into = if matches!(block, Block::Goal) {
                    &mut journey.goal
                } else {
                    &mut journey.ends
                };
                push_prose(into, trimmed);
            }
            Block::Cast => journey.cast.push(cast(trimmed, line)?),
            Block::Shows => journey.shows.push(axis(trimmed, line)?),
            Block::Given => journey.given.push(given(trimmed, line)?),
            Block::Steps => {
                if journey.steps.is_empty() {
                    return fail(line, "a clause outside any step");
                }
                match &mut pending {
                    // Deeper than the clause above it: the same clause, wrapped.
                    Some(open) if indent_of(text) > open.indent => {
                        open.text.push(' ');
                        open.text.push_str(trimmed);
                    }
                    _ => {
                        flush(&mut pending, &mut journey)?;
                        pending = Some(Pending {
                            line,
                            indent: indent_of(text),
                            text: trimmed.to_owned(),
                        });
                    }
                }
            }
            Block::None => {
                return fail(line, format!("expected a clause or a step, found `{trimmed}`"));
            }
        }
    }

    fail(opened, format!("journey `{name}` is never closed"))
}

/// A clause read so far, which a deeper line may still continue.
struct Pending {
    line: usize,
    indent: usize,
    text: String,
}

/// Turn the clause that was being read into one on the last step.
fn flush(pending: &mut Option<Pending>, journey: &mut Journey) -> Result<(), ParseError> {
    let Some(open) = pending.take() else { return Ok(()) };
    let parsed = clause(&open.text, open.line)?;
    match journey.steps.last_mut() {
        Some(step) => step.clauses.push(parsed),
        None => return fail(open.line, "a clause outside any step"),
    }
    Ok(())
}

/// How far a line is indented, in characters.
///
/// Characters rather than bytes, because this is compared against the
/// indentation of another line and a tab or a non-ASCII space would otherwise
/// count for more than it looks. Stipulation 4 in miniature: the two agree for
/// ASCII, which is why counting bytes survived every test written against an
/// ASCII fixture.
pub(crate) fn indent_of(text: &str) -> usize {
    text.chars().count() - text.trim_start().chars().count()
}

enum Block {
    None,
    Goal,
    Ends,
    Cast,
    Shows,
    Given,
    Steps,
}

/// `3. two hours later she takes it back`
fn step_header(text: &str) -> Option<(u32, String)> {
    let (number, rest) = text.split_once('.')?;
    let number = number.trim().parse::<u32>().ok()?;
    let title = rest.trim();
    if title.is_empty() { None } else { Some((number, title.to_owned())) }
}

fn push_prose(into: &mut Vec<String>, text: &str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        into.push(trimmed.to_owned());
    }
}

/// `ada: identity/Identity`
pub(crate) fn cast(text: &str, line: usize) -> Result<Cast, ParseError> {
    let Some((name, type_expr)) = text.split_once(':') else {
        return fail(line, format!("expected `<name>: <Type>`, found `{text}`"));
    };
    let (name, type_expr) = (name.trim(), type_expr.trim());
    if name.is_empty() || type_expr.is_empty() {
        return fail(line, "a cast member needs a name and a type");
    }
    Ok(Cast { name: name.to_owned(), type_expr: type_expr.to_owned(), line })
}

/// `theme: dark, light`
fn axis(text: &str, line: usize) -> Result<Axis, ParseError> {
    let Some((key, values)) = text.split_once(':') else {
        return fail(line, format!("expected `<name>: <value>, <value>`, found `{text}`"));
    };
    let key = key.trim();
    if key.is_empty() {
        return fail(line, "a way of showing a journey needs a name");
    }

    let values: Vec<String> = values
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();

    // One value is not a question, and the control would offer nothing to
    // choose. Saying so beats a dropdown with a single entry that does nothing.
    if values.len() < 2 {
        return fail(
            line,
            format!("`{key}` needs at least two values to be worth choosing between"),
        );
    }

    let mut seen = values.clone();
    seen.sort();
    seen.dedup();
    if seen.len() != values.len() {
        return fail(line, format!("`{key}` lists the same value twice"));
    }

    Ok(Axis { key: key.to_owned(), values, line })
}

/// `note: messaging/Message { … }`, or `ada.status = active`.
fn given(text: &str, line: usize) -> Result<Given, ParseError> {
    if let Some((head, rest)) = text.split_once('{') {
        let Some(fields) = rest.strip_suffix('}') else {
            return fail(line, "an instance's fields are closed with `}` on the same line");
        };
        let Cast { name, type_expr, .. } = cast(head.trim(), line)?;
        return Ok(Given::Instance { name, type_expr, fields: named_fields(fields, line)?, line });
    }

    let Some((left, right)) = split_once_operator(text, "=") else {
        return fail(
            line,
            format!("expected `<path> = <value>` or `<name>: <Type> {{…}}`, found `{text}`"),
        );
    };
    Ok(Given::Assign { path: path(left, line)?, value: term(right, line)?, line })
}

/// `author: ada, body: "…"`
fn named_fields(text: &str, line: usize) -> Result<Vec<(String, Term)>, ParseError> {
    let mut fields = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((name, value)) = part.split_once(':') else {
            return fail(line, format!("expected `<field>: <value>`, found `{part}`"));
        };
        fields.push((name.trim().to_owned(), term(value, line)?));
    }
    Ok(fields)
}

/// Everything before a `--` comment, trimmed.
///
/// A `--` inside a string literal is text, not a comment. Cutting at the first
/// one regardless turned `{ title: "Ada -- a life" }` into an unterminated
/// field list and reported it as a missing `}` — an error about the line's
/// shape when the problem was a dash inside a quote.
fn strip_comment(text: &str) -> &str {
    // Over pairs rather than over indices with arithmetic: `windows(2)` is what
    // "a dash followed by a dash" means, and doing it by hand needed an `at + 1`
    // and an `at += 1` that no assertion could distinguish from `* 1`.
    //
    // The final byte is never `pair[0]`, which costs nothing: a `-` at the very
    // end cannot begin a `--`, and a quote there closes a literal that is about
    // to end anyway.
    let mut quoted = false;
    for (at, pair) in text.as_bytes().windows(2).enumerate() {
        match pair[0] {
            b'"' => quoted = !quoted,
            // No escape handling, because the grammar has none: a literal has
            // no way to contain a quote, so the next one always closes it.
            b'-' if !quoted && pair[1] == b'-' => return text[..at].trim(),
            _ => {}
        }
    }
    text.trim()
}
