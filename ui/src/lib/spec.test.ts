import { describe, expect, it } from "vitest";

import type { Diagnostic } from "./api/Diagnostic";
import type { Edge } from "./api/Edge";
import type { SpecGraph } from "./api/SpecGraph";
import type { Node } from "./api/Node";
import {
  fieldLinks,
  positionOf,
  reportedAgainst,
  unattributed,
  worstByModule,
  worstByNode,
} from "./spec";

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

const retirement: Node = {
  id: "identity::entity::IdentityRetirement",
  kind: "entity",
  name: "IdentityRetirement",
  module: "identity",
  qualified: "identity/IdentityRetirement",
  span: null,
  detail: { type: "none" },
};

describe("reportedAgainst", () => {
  it("finds what the server attributed to this construct", () => {
    const mine = diagnostic({ node: retirement.id, message: "no observed transition" });
    const theirs = diagnostic({ node: "identity::entity::Device", message: "unused field" });
    expect(reportedAgainst([mine, theirs], retirement)).toEqual([mine]);
  });

  it("does not join on the line the diagnostic was reported at", () => {
    // The bug this replaces. Allium reports a diagnostic on the offending line
    // *inside* a construct: `IdentityRetirement` is declared at 530 and its two
    // lifecycle warnings sit at 534. Matching on the line meant the panel never
    // showed one, while the canvas badge — which matched on `node` — promised
    // there was one to show.
    const inside = diagnostic({
      node: retirement.id,
      location: { file: "identity.allium", line: 534, column: 3 },
    });
    expect(reportedAgainst([inside], retirement)).toEqual([inside]);
  });

  it("reports nothing when nothing is selected", () => {
    expect(reportedAgainst([diagnostic({ node: retirement.id })], null)).toEqual([]);
  });

  it("leaves an unattributed diagnostic to the construct-free surface", () => {
    // A parse error is reported where the parser gave up, which is not inside
    // any declaration. Attaching it to whatever was selected would blame a
    // construct for a mistake somewhere else in the file.
    expect(reportedAgainst([diagnostic({ node: null })], retirement)).toEqual([]);
  });
});

describe("unattributed", () => {
  it("collects what no construct can carry", () => {
    const loose = diagnostic({ node: null, message: "expected '{'" });
    const attached = diagnostic({ node: retirement.id });
    expect(unattributed([loose, attached])).toEqual([loose]);
  });

  it("is empty when everything found a home", () => {
    expect(unattributed([diagnostic({ node: retirement.id })])).toEqual([]);
  });
});

describe("fieldLinks", () => {
  const outbox: Node = {
    id: "delivery::entity::OutboxEntry",
    kind: "entity",
    name: "OutboxEntry",
    module: "delivery",
    qualified: "delivery/OutboxEntry",
    span: null,
    detail: { type: "none" },
  };

  const edge = (
    from: string,
    to: string,
    kind: Edge["kind"],
    label: string,
  ): Edge => ({ from, to, kind, label, span: null });

  const edges: Edge[] = [
    edge(outbox.id, "messaging::entity::Message", "field", "message"),
    edge(outbox.id, "identity::entity::Device", "relationship", "awaiting"),
    edge("delivery::rule::QueueOnSend", outbox.id, "mutates", "status"),
    edge("delivery::rule::OutboxEntrySettles", outbox.id, "mutates", "status"),
    edge("delivery::rule::QueueOnSend", outbox.id, "mutates", "queued_at"),
    edge("delivery::rule::QueueOnSend", outbox.id, "creates", "OutboxEntry"),
    edge("messaging::entity::Message", "membership::entity::Group", "field", "group"),
  ];

  it("says where a field's type resolved to", () => {
    // The linker already worked it out; the panel was rendering the type as
    // inert text and making the reader search for the name by hand.
    const { points } = fieldLinks(edges, outbox);
    expect(points.get("message")).toBe("messaging::entity::Message");
    expect(points.get("awaiting")).toBe("identity::entity::Device");
  });

  it("says nothing about a field whose type is not a construct", () => {
    expect(fieldLinks(edges, outbox).points.get("queued_at")).toBeUndefined();
  });

  it("does not take another entity's fields for this one's", () => {
    // `Message.group` is a field edge too, and it is not on OutboxEntry.
    expect(fieldLinks(edges, outbox).points.has("group")).toBe(false);
  });

  it("collects every rule that writes one field", () => {
    expect(fieldLinks(edges, outbox).written.get("status")).toEqual([
      "delivery::rule::QueueOnSend",
      "delivery::rule::OutboxEntrySettles",
    ]);
  });

  it("keeps a rule that writes two fields against both", () => {
    const { written } = fieldLinks(edges, outbox);
    expect(written.get("queued_at")).toEqual(["delivery::rule::QueueOnSend"]);
    expect(written.get("status")).toContain("delivery::rule::QueueOnSend");
  });

  it("lists a rule once per field however many edges carry it", () => {
    const twice = [...edges, edge("delivery::rule::QueueOnSend", outbox.id, "mutates", "status")];
    expect(fieldLinks(twice, outbox).written.get("status")).toEqual([
      "delivery::rule::QueueOnSend",
      "delivery::rule::OutboxEntrySettles",
    ]);
  });

  it("does not mistake creating the entity for writing a field", () => {
    // The `creates` edge is labelled with the entity, not with a field, and
    // reading it as one would invent a field called OutboxEntry.
    expect(fieldLinks(edges, outbox).written.has("OutboxEntry")).toBe(false);
  });

  it("has nothing to say when nothing is selected", () => {
    const { points, written } = fieldLinks(edges, null);
    expect(points.size).toBe(0);
    expect(written.size).toBe(0);
  });
});
