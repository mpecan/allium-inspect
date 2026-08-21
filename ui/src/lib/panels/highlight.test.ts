import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { type TokenKind, tokens } from "./highlight";

/** The kinds a line produces, in order, ignoring whitespace. */
function kinds(line: string): TokenKind[] {
  return tokens(line)
    .filter((token) => token.text.trim() !== "")
    .map((token) => token.kind);
}

/** The text of every token of `kind` on `line`. */
function of(line: string, kind: TokenKind): string[] {
  return tokens(line)
    .filter((token) => token.kind === kind)
    .map((token) => token.text);
}

describe("the text is never altered", () => {
  it("puts a line back together exactly as it came", () => {
    // The promise this whole panel rests on. A highlighter is the easiest place
    // to break it by accident — a swallowed character, a normalised run of
    // spaces — and the reader would have no way to tell, because what they are
    // checking against is the thing on screen.
    const lines = [
      "entity Reservation {",
      "    book: catalogue/Book",
      "    status: waiting | fulfilled | cancelled",
      "        waiting -> fulfilled",
      "    loan_period: Duration = 21.days",
      "-- A state-condition trigger: no external actor fires this.",
      "------------------------------------------------------------",
      '    name: String = "Ada"',
      "    @guarantee AWithdrawnBookTakesNoNewCopies",
      "",
      "   ",
      "\tindented with a tab",
      "trailing space ",
      "a — dash and an ünlaut, and a 🙂",
    ];
    for (const line of lines) {
      expect(tokens(line).map((token) => token.text).join("")).toBe(line);
    }
  });

  it("puts every line of the real fixture specs back together too", () => {
    // Hand-written lines are the ones somebody thought of. These are the ones
    // nobody did — including the separator rules, the prose comments with
    // punctuation in them, and the blank lines between blocks.
    const specs = join(
      import.meta.dirname,
      "../../../../crates/inspect-model/tests/fixtures/specs",
    );
    for (const name of ["catalogue.allium", "lending.allium"]) {
      const text = readFileSync(join(specs, name), "utf8");
      for (const line of text.split("\n")) {
        expect(tokens(line).map((token) => token.text).join(""), `${name}: ${line}`).toBe(line);
      }
    }
  });

  it("has nothing to say about an empty line", () => {
    expect(tokens("")).toEqual([]);
  });
});

describe("comments", () => {
  it("runs a comment to the end of its line", () => {
    const line = "    status: open  -- and everything after this is prose";
    expect(of(line, "comment")).toEqual(["-- and everything after this is prose"]);
  });

  it("colours a separator rule as the comment it is", () => {
    // Six of these in a real spec, and they are the loudest thing on screen if
    // they are not quietened.
    expect(kinds("--------------------------------")).toEqual(["comment"]);
  });

  it("does not mistake a transition arrow for a comment", () => {
    // `->` is a hyphen and a greater-than; `--` is two hyphens. Getting this
    // wrong greys out every lifecycle in the file.
    expect(of("        waiting -> fulfilled", "comment")).toEqual([]);
    expect(kinds("        waiting -> fulfilled")).toEqual([
      "text", "punctuation", "punctuation", "text",
    ]);
  });

  it("keeps a comment a comment even when it contains code", () => {
    const line = '-- entity Book { name: String = "x" }';
    expect(of(line, "comment")).toEqual([line]);
  });
});

describe("keywords and types", () => {
  it("knows allium's keywords", () => {
    expect(of("entity Reservation {", "keyword")).toEqual(["entity"]);
    expect(of("    when: MemberBorrows(member, copy)", "keyword")).toEqual(["when"]);
    expect(of("    transitions status {", "keyword")).toEqual(["transitions"]);
    expect(of("        terminal: fulfilled", "keyword")).toEqual(["terminal"]);
    expect(of("    requires: not member.is_at_limit", "keyword")).toEqual(["requires", "not"]);
  });

  it("does not colour a field that merely starts like a keyword", () => {
    // `information` begins with `in`, and `configured` with `config`. A
    // prefix match would colour half the field names in a real spec.
    expect(of("    information: String", "keyword")).toEqual([]);
    expect(of("    configured: Boolean", "keyword")).toEqual([]);
  });

  it("colours a capitalised name as a type", () => {
    expect(of("    book: catalogue/Book", "type")).toEqual(["catalogue/Book"]);
    expect(of("    placed_at: Timestamp", "type")).toEqual(["Timestamp"]);
  });

  it("reads the capital on the last segment of a qualified name", () => {
    // `catalogue/Book` is a type; `catalogue` on its own is a module.
    expect(of("catalogue/Book", "type")).toEqual(["catalogue/Book"]);
    expect(of("membership/group_of", "type")).toEqual([]);
  });

  it("leaves a lower-case name alone", () => {
    expect(of("    member: Member", "type")).toEqual(["Member"]);
    expect(of("    member: Member", "text").filter((t) => t.trim())).toEqual(["member"]);
  });
});

describe("literals", () => {
  it("colours a string, quotes and all", () => {
    expect(of('    name: String = "Ada Lovelace"', "string")).toEqual(['"Ada Lovelace"']);
  });

  it("colours a backtick literal, which is a name with awkward characters", () => {
    expect(of("    locale: `de-CH-1996`", "string")).toEqual(["`de-CH-1996`"]);
  });

  it("colours an unterminated string to the end of the line", () => {
    // The file is being typed. Pretending the quote is not there is worse than
    // showing the reader where it runs to.
    expect(of('    name: "Ada', "string")).toEqual(['"Ada']);
  });

  it("keeps a duration in one piece", () => {
    // `21.days` split on the dot reads as a number, a full stop and a field
    // called `days`, which is three wrong colours for one value.
    expect(of("    loan_period: Duration = 21.days", "number")).toEqual(["21.days"]);
  });

  it("keeps digit separators in the number they belong to", () => {
    expect(of("    cap: Integer = 2_000_000_000", "number")).toEqual(["2_000_000_000"]);
  });
});

describe("annotations", () => {
  it("colours the whole annotation rather than the at-sign", () => {
    // `@` alone is not what a reader scans for.
    expect(of("    @guarantee ALoanIsVisibleToItsHolderOnly", "annotation")).toEqual([
      "@guarantee",
    ]);
  });

  it("colours the three the language has", () => {
    for (const word of ["@guarantee", "@guidance", "@invariant"]) {
      expect(of(`    ${word} Name`, "annotation")).toEqual([word]);
    }
  });
});
