// Colouring a line of Allium.
//
// A tokeniser rather than a parser. The strip shows one line at a time and has
// to render a line that is mid-declaration, mid-block, or not valid Allium at
// all — a spec is edited with this tool open, so half the time what is on
// screen does not parse. A parser would have nothing to say about those lines;
// a tokeniser says the same thing about every one of them.
//
// **The text is never altered.** Stipulation three of this project is that the
// panel shows what the author wrote, and a highlighter is the easiest place in
// a codebase to break that promise by accident — swallowing a stray character,
// normalising whitespace, dropping a trailing space. So the contract here is
// that the token texts concatenate back to exactly the input, and a property
// test asserts it over every line of the fixture specs.
//
// The keywords are allium's own, taken from `crates/allium-parser/src/lexer.rs`
// rather than from reading specs and guessing. A word this list is missing
// renders as an identifier, which is wrong but harmless; a word on it that
// allium does not have would colour a field name as a keyword, which is a
// small lie about the language.

/** What a run of characters is, for the purpose of colouring it. */
export type TokenKind =
  | "comment"
  | "keyword"
  | "type"
  | "string"
  | "number"
  | "annotation"
  | "punctuation"
  | "text";

export interface Token {
  text: string;
  kind: TokenKind;
}

/** Allium's keywords, as its lexer spells them. */
const KEYWORDS = new Set([
  "rule", "entity", "external", "value", "enum", "given", "config", "surface",
  "actor", "default", "variant", "deferred", "open", "question", "use", "as",
  "when", "requires", "ensures", "let", "for", "in", "if", "else", "where",
  "with", "not", "and", "or", "exists", "implies", "contract", "invariant",
  "transitions_to", "becomes", "transitions", "produces", "consumes",
  "terminal", "true", "false", "null", "now", "this", "within",
]);

const PUNCTUATION = new Set([..."{}()[]:,|=<>+-*/?@."]);

/**
 * `line` split into runs worth colouring differently.
 *
 * One line at a time, which is sound because nothing in Allium's lexis crosses
 * a newline: a comment runs to the end of its line and a string literal cannot
 * contain one. That is what lets the strip colour a window onto a file without
 * reading the rest of it.
 */
export function tokens(line: string): Token[] {
  const found: Token[] = [];
  let at = 0;

  const take = (kind: TokenKind, to: number) => {
    found.push({ kind, text: line.slice(at, to) });
    at = to;
  };

  while (at < line.length) {
    const character = line[at] ?? "";

    // `--` to the end of the line. Checked first: a comment can contain
    // anything, including something that looks like every other rule here.
    if (character === "-" && line[at + 1] === "-") {
      take("comment", line.length);
      continue;
    }

    if (character === " " || character === "\t") {
      take("text", runOf(line, at, (c) => c === " " || c === "\t"));
      continue;
    }

    if (character === '"' || character === "`") {
      take("string", closingAfter(line, at, character));
      continue;
    }

    // `@guarantee`, `@guidance`, `@invariant` — the whole thing, because the
    // `@` alone is not what a reader is looking for.
    if (character === "@") {
      take("annotation", runOf(line, at + 1, isWordCharacter));
      continue;
    }

    if (isDigit(character)) {
      // `21.days` is one token, and so is `2_000_000`. Split on the dot and
      // the duration reads as a number, a full stop and a field called `days`.
      take("number", runOf(line, at, (c) => isWordCharacter(c) || c === "."));
      continue;
    }

    if (isWordStart(character)) {
      const end = runOf(line, at, (c) => isWordCharacter(c) || c === "/");
      const word = line.slice(at, end);
      take(kindOfWord(word), end);
      continue;
    }

    if (PUNCTUATION.has(character)) {
      take("punctuation", at + 1);
      continue;
    }

    // Anything else at all — a character this tokeniser has no rule for. Kept
    // rather than skipped, because dropping it would change the line.
    take("text", at + 1);
  }

  return found;
}

/**
 * Whether a word is a keyword, a type name, or neither.
 *
 * Capitalisation is the language's own convention for a type, and it is checked
 * on the last segment: `catalogue/Book` is a type and `catalogue` is not.
 */
function kindOfWord(word: string): TokenKind {
  if (KEYWORDS.has(word)) {
    return "keyword";
  }
  const last = word.slice(word.lastIndexOf("/") + 1);
  return /^[A-Z]/.test(last) ? "type" : "text";
}

/** The end of the run from `from` for which every character satisfies `holds`. */
function runOf(line: string, from: number, holds: (character: string) => boolean): number {
  let at = from;
  while (at < line.length && holds(line[at] ?? "")) {
    at += 1;
  }
  return at;
}

/**
 * The index just past the closing `quote`, or the end of the line.
 *
 * An unterminated string is an ordinary thing to see: the file is being typed.
 * Colouring the rest of the line as a string is what an editor does, and it is
 * a better signal than pretending the quote was not there.
 */
function closingAfter(line: string, from: number, quote: string): number {
  let at = from + 1;
  while (at < line.length) {
    if (line[at] === "\\") {
      at += 2;
      continue;
    }
    if (line[at] === quote) {
      return at + 1;
    }
    at += 1;
  }
  return line.length;
}

function isDigit(character: string): boolean {
  return character >= "0" && character <= "9";
}

function isWordStart(character: string): boolean {
  return /[A-Za-z_]/.test(character);
}

function isWordCharacter(character: string): boolean {
  return /[A-Za-z0-9_]/.test(character);
}
