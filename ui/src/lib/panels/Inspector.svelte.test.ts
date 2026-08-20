// @vitest-environment happy-dom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import type { Node } from "../api/Node";
import type { Prose } from "../api/Prose";
import Inspector from "./Inspector.svelte";

function entity(prose: Partial<Prose>, fieldNote: string[] = []): Node {
  return {
    id: "delivery::entity::OutboxEntry",
    kind: "entity",
    name: "OutboxEntry",
    module: "delivery",
    qualified: "delivery/OutboxEntry",
    span: null,
    prose: { note: [], guidance: [], ...prose },
    detail: {
      type: "entity",
      kind: "internal",
      parent: null,
      transitions: [],
      fields: [
        {
          name: "status",
          type_expr: "queued | settled",
          enum_values: [],
          derived: false,
          relationship: false,
          when: null,
          note: fieldNote,
        },
      ],
    },
  };
}

function show(node: Node | null) {
  render(Inspector, {
    props: {
      node,
      position: null,
      modulePath: "specs/delivery.allium",
      diagnostics: [],
      findings: [],
      obligations: [],
      links: { points: new Map(), written: new Map() },
      onselect: vi.fn(),
      onselectByName: vi.fn(),
      nameOf: (id: string) => id,
    },
  });
}

describe("Inspector · prose", () => {
  it("shows what the author wrote above the declaration", () => {
    // More than half of a real spec is comment, and until this the panel showed
    // the fields and left the paragraph explaining them four lines up in a file
    // nobody had open.
    show(entity({ note: ["What this device has said", "and has not got rid of."] }));
    expect(screen.getByText("What this device has said and has not got rid of.")).toBeTruthy();
  });

  it("joins a wrapped paragraph and breaks where the author did", () => {
    // The lines were wrapped at their editor's column. Kept as lines in a panel
    // a third that width, a paragraph reads as a poem.
    show(entity({ note: ["first para", "wrapped", "", "second para"] }));
    expect(screen.getByText("first para wrapped")).toBeTruthy();
    expect(screen.getByText("second para")).toBeTruthy();
  });

  it("shows guidance under its own heading, and says nothing checks it", () => {
    // `@guidance` is advice to whoever builds this. Presenting it beside the
    // clauses without that would read as something the tool verified.
    show(entity({ guidance: ["A hub never pushes."] }));
    expect(screen.getByRole("heading", { name: "Guidance" })).toBeTruthy();
    expect(screen.getByText("A hub never pushes.")).toBeTruthy();
    expect(screen.getByText(/nothing checks it/)).toBeTruthy();
  });

  it("keeps a note and its guidance apart", () => {
    // Different acts: one is a comment the author left, the other a block the
    // language itself recognises. Merging them would be the tool deciding they
    // are the same thing.
    show(entity({ note: ["the note"], guidance: ["the guidance"] }));
    expect(screen.getByText("the note")).toBeTruthy();
    expect(screen.getByText("the guidance")).toBeTruthy();
  });

  it("puts a field's note under the field", () => {
    // Sixty-nine of them in one file. A section each would be sixty-nine
    // headings; under the field is where it was written.
    show(entity({}, ["An entry that lapses is deleted."]));
    expect(screen.getByText("An entry that lapses is deleted.")).toBeTruthy();
  });

  it("shows a backticked span as code rather than as backticks", () => {
    show(entity({ note: ["No `expired` here."] }));
    const code = screen.getByText("expired");
    expect(code.tagName).toBe("CODE");
    expect(screen.queryByText(/`expired`/)).toBeNull();
  });

  it("shows a starred span as emphasis rather than as stars", () => {
    show(entity({ note: ["ends up **staggered but whole**, not clipped"] }));
    expect(screen.getByText("staggered but whole").tagName).toBe("STRONG");
  });

  it("says nothing at all when there is nothing written", () => {
    show(entity({}));
    expect(screen.queryByRole("heading", { name: "Guidance" })).toBeNull();
  });

  it("draws no Fields section for a construct that declares none", () => {
    // A variant declares its own field inside its base expression rather than
    // in its body, so it can be written by rules and list no fields at all —
    // and an empty heading is a heading over nothing.
    const bare = entity({});
    bare.detail = { ...bare.detail, fields: [] } as typeof bare.detail;
    show(bare);
    expect(screen.queryByRole("heading", { name: "Fields" })).toBeNull();
    expect(screen.queryByText(/"Written by" lists rules/)).toBeNull();
  });

  it("survives having nothing selected", () => {
    show(null);
    expect(screen.getByText(/Pick a construct/)).toBeTruthy();
  });
});
