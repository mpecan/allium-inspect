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

use inspect_sim::{Value, seed};

use crate::journey::{Assertion, Axis, Cast, Clause, Comparison, Given, Journey, Path, Step, Term};

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

fn fail<T>(line: usize, message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { line, message: message.into() })
}

/// Every journey in one file.
///
/// # Errors
///
/// Returns the first line that could not be read, and why.
pub fn parse(source: &str) -> Result<Vec<Journey>, ParseError> {
    let mut journeys = Vec::new();
    fn numbered((at, text): (usize, &str)) -> (usize, &str) {
        (at + 1, text)
    }
    let mut lines: Lines<'_> = source.lines().enumerate().map(numbered as _).peekable();

    while let Some((line, text)) = lines.next() {
        let trimmed = strip_comment(text);
        if trimmed.is_empty() {
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
        journeys.push(body(name, line, &mut lines)?);
    }
    Ok(journeys)
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
fn cast(text: &str, line: usize) -> Result<Cast, ParseError> {
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

/// One line under a step.
fn clause(text: &str, line: usize) -> Result<Clause, ParseError> {
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
            return fail(line, "expected `stipulate <path> = <value>`");
        };
        return Ok(Clause::Stipulate { path: path(left, line)?, value: term(right, line)?, line });
    }
    if let Some((actor, rest)) = text.split_once(" does ") {
        return does(actor.trim(), rest.trim(), line);
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
fn does(actor: &str, rest: &str, line: usize) -> Result<Clause, ParseError> {
    let (rest, creating) = match rest.split_once(" creating ") {
        Some((head, caught)) => (head.trim(), Some(cast(caught.trim(), line)?)),
        None => (rest, None),
    };
    let Some((call, surface)) = rest.rsplit_once(" on ") else {
        return fail(line, format!("expected `… on <Surface>`, found `{rest}`"));
    };
    let (trigger, arguments) = call_parts(call.trim(), line)?;
    let surface = surface.trim();
    if surface.is_empty() {
        return fail(line, "an act happens on a surface, and this one names none");
    }
    Ok(Clause::Does {
        actor: actor.to_owned(),
        trigger,
        arguments,
        surface: surface.to_owned(),
        creating,
        line,
    })
}

/// `loan.status on MemberShelf`
fn sees(actor: &str, rest: &str, negated: bool, line: usize) -> Result<Clause, ParseError> {
    let Some((seen, surface)) = rest.rsplit_once(" on ") else {
        return fail(line, format!("expected `… on <Surface>`, found `{rest}`"));
    };
    Ok(Clause::Sees {
        actor: actor.to_owned(),
        path: path(seen, line)?,
        surface: surface.trim().to_owned(),
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
fn split_arguments(text: &str) -> Vec<String> {
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
fn split_once_operator<'a>(text: &'a str, symbol: &str) -> Option<(&'a str, &'a str)> {
    let at = text.find(symbol)?;
    let before = text[..at].chars().next_back();
    let after = text[at + symbol.len()..].chars().next();
    // `!=`, `<=` and `>=` all end in `=`, so a bare `=` must not split them.
    if symbol == "=" && (matches!(before, Some('!' | '<' | '>')) || after == Some('=')) {
        return None;
    }
    Some((&text[..at], &text[at + symbol.len()..]))
}

/// `loan.copy.status`
fn path(text: &str, line: usize) -> Result<Path, ParseError> {
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
fn term(text: &str, line: usize) -> Result<Term, ParseError> {
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
