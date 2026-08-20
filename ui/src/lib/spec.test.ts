import { describe, expect, it } from "vitest";

import type { Diagnostic } from "./api/Diagnostic";
import type { SpecGraph } from "./api/SpecGraph";
import { positionOf, worstByModule, worstByNode } from "./spec";

const encoder = new TextEncoder();

/** The byte offset of `needle`, as the parser reports offsets. */
function byteOffset(text: string, needle: string): number {
  return encoder.encode(text.slice(0, text.indexOf(needle))).length;
}

function diagnostic(partial: Partial<Diagnostic>): Diagnostic {
  return {
    severity: "warning",
    message: "m",
    code: null,
    location: null,
    module: "catalogue",
    node: null,
    ...partial,
  };
}

function graph(diagnostics: Diagnostic[]): SpecGraph {
  return {
    allium_version: "test",
    modules: [],
    nodes: [],
    edges: [],
    diagnostics,
    findings: [],
    obligations: [],
  };
}

describe("positionOf", () => {
  const SPEC = "entity Book {\n    title: String\n}\n";

  it("is one-based at the very start", () => {
    expect(positionOf(SPEC, { start: 0, end: 1 })).toEqual({ line: 1, column: 1 });
  });

  it("resolves a later line and column", () => {
    const at = byteOffset(SPEC, "title");
    expect(positionOf(SPEC, { start: at, end: at + 5 })).toEqual({ line: 2, column: 5 });
  });

  it("reads the offset as bytes, which is how the parser reports it", () => {
    // An em-dash is three bytes and one UTF-16 unit. Slicing the string by a
    // byte offset lands early, and the caret ends up on the wrong line once
    // enough prose has accumulated above the declaration.
    const text = "-- a — comment\n-- another — one\nentity Book {\n";
    const at = byteOffset(text, "entity Book {");
    expect(at).toBeGreaterThan(text.indexOf("entity Book {"));
    expect(positionOf(text, { start: at, end: at + 1 })).toEqual({ line: 3, column: 1 });
  });

  it("counts the column in characters, which is what an editor shows", () => {
    const text = "-- naïve — here\n";
    const at = byteOffset(text, "here");
    // "-- naïve — " is eleven characters, so `here` starts at column twelve.
    expect(positionOf(text, { start: at, end: at + 4 })).toEqual({ line: 1, column: 12 });
  });

  it("has no position when nothing is selected", () => {
    expect(positionOf(SPEC, null)).toBeNull();
  });

  it("clamps a span from a file edited since it was read", () => {
    // The end of the file, not an invented line beyond it. `SPEC` ends with a
    // newline, so the last position is the empty line after it — which is
    // where an editor puts the caret too.
    const clamped = positionOf(SPEC, { start: 9000, end: 9001 });
    const atEnd = positionOf(SPEC, { start: SPEC.length, end: SPEC.length });
    expect(clamped).toEqual(atEnd);
    expect(clamped).toEqual({ line: 4, column: 1 });
  });

  it("clamps a negative offset to the start rather than wrapping", () => {
    expect(positionOf(SPEC, { start: -5, end: 0 })).toEqual({ line: 1, column: 1 });
  });
});

describe("worstByNode", () => {
  it("keeps the worst severity reported against each construct", () => {
    const result = worstByNode(
      graph([
        diagnostic({ node: "catalogue::entity::Copy", severity: "info" }),
        diagnostic({ node: "catalogue::entity::Copy", severity: "error" }),
        diagnostic({ node: "catalogue::entity::Copy", severity: "warning" }),
        diagnostic({ node: "catalogue::entity::Book", severity: "info" }),
      ]),
    );
    expect(result.get("catalogue::entity::Copy")).toBe("error");
    expect(result.get("catalogue::entity::Book")).toBe("info");
  });

  it("ignores a diagnostic the server could not attribute", () => {
    // A parse error is reported where the parser gave up, which is often not
    // inside the construct that was wrong. Badging a nearby box would send the
    // reader somewhere there is nothing to find.
    const result = worstByNode(graph([diagnostic({ node: null, severity: "error" })]));
    expect(result.size).toBe(0);
  });

  it("reports nothing for a clean spec", () => {
    expect(worstByNode(graph([])).size).toBe(0);
  });
});

describe("worstByModule", () => {
  it("counts every diagnostic, attributed or not", () => {
    // The module list is the one place an unattributed error must still show:
    // it is the only signal that something is wrong at all.
    const result = worstByModule(
      graph([
        diagnostic({ module: "catalogue", node: null, severity: "error" }),
        diagnostic({ module: "lending", severity: "warning" }),
      ]),
    );
    expect(result.get("catalogue")).toBe("error");
    expect(result.get("lending")).toBe("warning");
  });
});
