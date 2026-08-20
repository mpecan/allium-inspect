import { describe, expect, it } from "vitest";

import { inline, paragraphs } from "./prose";

describe("paragraphs", () => {
  it("joins the lines of one paragraph", () => {
    // The author wrapped at their editor's column. Rendered into a panel a
    // third that width, the original breaks read as a poem.
    expect(paragraphs(["Writing a message and", "transmitting it are", "separate events."])).toEqual(
      ["Writing a message and transmitting it are separate events."],
    );
  });

  it("breaks where the author left a blank line", () => {
    expect(paragraphs(["first one", "", "second one"])).toEqual(["first one", "second one"]);
  });

  it("treats several blank lines as one break", () => {
    expect(paragraphs(["a", "", "", "b"])).toEqual(["a", "b"]);
  });

  it("ignores blank lines at either end", () => {
    expect(paragraphs(["", "a", ""])).toEqual(["a"]);
  });

  it("has nothing to say about nothing", () => {
    expect(paragraphs([])).toEqual([]);
    expect(paragraphs(["", "  "])).toEqual([]);
  });

  it("keeps the words and drops only the wrapping", () => {
    // The one thing it must not do is lose or reorder what was written.
    const lines = ["No `expired`. An entry that lapses is", "deleted, so absence is what says"];
    expect(paragraphs(lines)[0]).toBe(
      "No `expired`. An entry that lapses is deleted, so absence is what says",
    );
  });
});

describe("inline", () => {
  it("leaves plain prose in one piece", () => {
    expect(inline("nothing marked up here")).toEqual([
      { kind: "text", text: "nothing marked up here" },
    ]);
  });

  it("reads a backticked span as code", () => {
    // `expired` is a state name. Showing the backticks shows the scaffolding.
    expect(inline("No `expired` here.")).toEqual([
      { kind: "text", text: "No " },
      { kind: "code", text: "expired" },
      { kind: "text", text: " here." },
    ]);
  });

  it("reads a double-starred span as emphasis", () => {
    expect(inline("ends up **staggered but whole**, not clipped")).toEqual([
      { kind: "text", text: "ends up " },
      { kind: "strong", text: "staggered but whole" },
      { kind: "text", text: ", not clipped" },
    ]);
  });

  it("reads several in one paragraph", () => {
    const pieces = inline("`a` and `b` and **c**");
    expect(pieces.map((piece) => piece.kind)).toEqual([
      "code",
      "text",
      "code",
      "text",
      "strong",
    ]);
  });

  it("leaves an unmatched marker alone", () => {
    // A stray backtick is a stray backtick. Pairing it with the next one across
    // half a paragraph would swallow the words between them.
    expect(inline("a lone ` backtick")).toEqual([{ kind: "text", text: "a lone ` backtick" }]);
    expect(inline("one ** star")).toEqual([{ kind: "text", text: "one ** star" }]);
  });

  it("leaves everything else as written", () => {
    // A link, a list, a heading: guessing at a markup language the spec never
    // claimed to be written in is how a tool starts putting words in a mouth.
    const written = "see [the note](x) and\n# not a heading";
    expect(inline(written)).toEqual([{ kind: "text", text: written }]);
  });

  it("marks a whole paragraph when the whole paragraph is marked", () => {
    expect(inline("**What is not in it is the design.**")).toEqual([
      { kind: "strong", text: "What is not in it is the design." },
    ]);
  });

  it("has nothing to say about an empty string", () => {
    expect(inline("")).toEqual([]);
  });
});
