import { describe, expect, it } from "vitest";

import { sliceLines } from "./source";

const SPEC = [
  "-- catalogue.allium", // 1
  "", // 2
  "enum Medium { print | audio }", // 3
  "", // 4
  "entity Book {", // 5
  "    title: String", // 6
  "    status: listed | withdrawn", // 7
  "}", // 8
  "", // 9
].join("\n");

const encoder = new TextEncoder();

/** The *byte* span of `needle` in `text`, as the parser would report it. */
function spanIn(text: string, needle: string) {
  const at = text.indexOf(needle);
  if (at < 0) {
    throw new Error(`the fixture does not contain ${needle}`);
  }
  const start = encoder.encode(text.slice(0, at)).length;
  return { start, end: start + encoder.encode(needle).length };
}

function spanOf(needle: string) {
  return spanIn(SPEC, needle);
}

describe("sliceLines", () => {
  it("highlights every line the span covers", () => {
    const view = sliceLines(SPEC, spanOf("entity Book {\n    title"), 40);
    const highlit = view.lines.filter((line) => line.highlit).map((l) => l.number);
    expect(highlit).toEqual([5, 6]);
  });

  it("highlights only the line a single-line span is on", () => {
    const view = sliceLines(SPEC, spanOf("enum Medium { print | audio }"), 40);
    expect(view.lines.filter((line) => line.highlit).map((l) => l.number)).toEqual([3]);
  });

  it("numbers lines from one, as an editor does", () => {
    const view = sliceLines(SPEC, spanOf("-- catalogue.allium"), 40);
    expect(view.lines[0]?.number).toBe(1);
    expect(view.firstLine).toBe(1);
  });

  it("keeps context above the declaration when there is room", () => {
    const view = sliceLines(SPEC, spanOf("entity Book {"), 40);
    expect(view.lines[0]?.number).toBe(3);
    // The address names the declaration, not the first line shown: the strip's
    // header says where the construct is, and the lead is only context.
    expect(view.firstLine).toBe(5);
  });

  it("drops the context rather than the declaration when space is tight", () => {
    // A one-line strip should show the thing itself, not the blank line above
    // it. Keeping the lead here would peek at a comment and hide the entity.
    const view = sliceLines(SPEC, spanOf("entity Book {"), 1);
    expect(view.lines).toHaveLength(1);
    expect(view.lines[0]?.text).toBe("entity Book {");
    expect(view.lines[0]?.highlit).toBe(true);
  });

  it("shows at most the budget it was given", () => {
    expect(sliceLines(SPEC, spanOf("entity Book {"), 3).lines).toHaveLength(3);
  });

  it("does not run past the end of the file", () => {
    const view = sliceLines(SPEC, spanOf("}"), 40);
    expect(view.lines.at(-1)?.number).toBeLessThanOrEqual(9);
  });

  it("marks the line the declaration opens on, for the eye to land on", () => {
    // A forty-line rule flooded with highlighter is no easier to read than one
    // without; the opening line is where the reader actually starts.
    const view = sliceLines(SPEC, spanOf("entity Book {\n    title"), 40);
    const opens = view.lines.filter((line) => line.opens).map((line) => line.number);
    expect(opens).toEqual([5]);
  });

  it("marks no opening line when nothing is highlighted", () => {
    expect(sliceLines(SPEC, null, 40).lines.some((line) => line.opens)).toBe(false);
  });

  it("shows nothing when nothing is selected", () => {
    // Line 1 of a file whose selection is elsewhere is a quiet lie about
    // where you are.
    expect(sliceLines(SPEC, null, 40)).toEqual({ lines: [], firstLine: 0 });
  });

  it("shows nothing for an empty file", () => {
    expect(sliceLines("", { start: 0, end: 0 }, 40).lines).toEqual([]);
  });

  it("shows nothing when there is no room to show anything", () => {
    expect(sliceLines(SPEC, spanOf("entity Book {"), 0).lines).toEqual([]);
  });

  it("clamps a span from a file that has since been edited", () => {
    // The tool watches files while they are being written, so a span from the
    // previous read pointing past the current end is ordinary, not exceptional.
    const view = sliceLines(SPEC, { start: 5000, end: 6000 }, 40);
    expect(view.lines.length).toBeGreaterThan(0);
    expect(view.firstLine).toBeLessThanOrEqual(9);
  });

  it("handles a reversed span without looping or throwing", () => {
    const view = sliceLines(SPEC, { start: 60, end: 10 }, 40);
    expect(view.lines.length).toBeGreaterThan(0);
  });

  it("handles an empty span at a position", () => {
    const at = SPEC.indexOf("entity Book {");
    const view = sliceLines(SPEC, { start: at, end: at }, 40);
    expect(view.lines.filter((line) => line.highlit).map((l) => l.number)).toEqual([5]);
  });

  it("resolves a span by bytes, not by string index", () => {
    // The bug this function exists to avoid. The parser counts bytes;
    // JavaScript strings are indexed in UTF-16 units. They agree for ASCII —
    // which is why it looks fine until a spec has an em-dash in a comment, and
    // then every highlight below it silently shifts.
    const text = [
      "-- a comment — with an em-dash", // 3 bytes for the dash, 1 unit
      "-- another — one",
      "entity Book {",
      "    title: String",
      "}",
      "",
    ].join("\n");

    const view = sliceLines(text, spanIn(text, "entity Book {"), 40);
    const opens = view.lines.filter((line) => line.opens);
    expect(opens).toHaveLength(1);
    expect(opens[0]?.text).toBe("entity Book {");
    expect(view.firstLine).toBe(3);
  });

  it("drifts by exactly the byte surplus when given a string index instead", () => {
    // Stated as a test so the failure mode is documented rather than merely
    // avoided: two em-dashes ahead of the declaration put a naive lookup four
    // bytes early, which is a whole line in a dense file.
    const text = "-- — —\nentity Book {\n";
    const byteSpan = spanIn(text, "entity Book {");
    const stringIndex = text.indexOf("entity Book {");
    expect(byteSpan.start - stringIndex).toBe(4);
    expect(sliceLines(text, byteSpan, 40).firstLine).toBe(2);
  });

  it("counts lines correctly past multi-byte characters", () => {
    const text = "-- naïve\nentity A {}\n";
    const view = sliceLines(text, spanIn(text, "entity A {}"), 40);
    expect(view.firstLine).toBe(2);
  });
});
