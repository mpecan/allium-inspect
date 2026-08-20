// @vitest-environment happy-dom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import type { Node } from "../api/Node";
import type { NodeDetail } from "../api/NodeDetail";
import ConstructBody from "./ConstructBody.svelte";

function node(kind: Node["kind"], name: string, detail: NodeDetail): Node {
  return {
    id: `m::${kind}::${name}`,
    kind,
    name,
    module: "m",
    qualified: `m/${name}`,
    span: null,
    detail,
  };
}

function draw(construct: Node) {
  return render(ConstructBody, { props: { node: construct } });
}

describe("ConstructBody", () => {
  it("shows a construct's kind and name", () => {
    draw(node("entity", "Book", { type: "none" }));
    expect(screen.getByText("Book")).toBeTruthy();
    expect(screen.getByText("entity")).toBeTruthy();
  });

  it("renders a rule that creates two entities", () => {
    // The shape that took the whole canvas down. `summaryRows` emits one row
    // per created entity, both labelled `creates`, and keying the list by label
    // made that a duplicate key — which Svelte 5 throws on, unmounting
    // everything rather than just this node.
    const rule = node("rule", "SendMessage", {
      type: "rule",
      trigger: "MemberSends",
      source: "external",
      clauses: [],
      creates: ["Message", "Attachment"],
      emits: ["MessageSent", "MessageQueued"],
    });
    expect(() => draw(rule)).not.toThrow();
    expect(screen.getAllByText("creates")).toHaveLength(2);
    expect(screen.getAllByText("emits")).toHaveLength(2);
    expect(screen.getByText("Attachment")).toBeTruthy();
  });

  it("renders an entity whose fields repeat a type", () => {
    const entity = node("entity", "Loan", {
      type: "entity",
      kind: "internal",
      parent: null,
      transitions: [],
      fields: [
        { name: "opened_at", type_expr: "Timestamp", enum_values: [], derived: false, relationship: false, when: null },
        { name: "due_at", type_expr: "Timestamp", enum_values: [], derived: false, relationship: false, when: null },
      ],
    });
    expect(() => draw(entity)).not.toThrow();
    expect(screen.getAllByText("Timestamp")).toHaveLength(2);
  });

  it("marks an unresolved reference as undeclared", () => {
    draw(node("external", "Phantom", { type: "none" }));
    expect(screen.getByText("not declared")).toBeTruthy();
  });

  it("badges a severity with an accessible name", () => {
    const { container } = render(ConstructBody, {
      props: { node: node("entity", "Copy", { type: "none" }), severity: "warning" },
    });
    const badge = container.querySelector(".severity");
    expect(badge?.getAttribute("aria-label")).toContain("warning");
  });

  it("shows no severity badge when nothing was reported", () => {
    const { container } = draw(node("entity", "Book", { type: "none" }));
    expect(container.querySelector(".severity")).toBeNull();
  });

  it("carries the family and kind as classes, which is what colours and shapes it", () => {
    const { container } = draw(node("trigger", "MemberSends", { type: "none" }));
    const box = container.querySelector(".construct");
    expect(box?.classList.contains("behaviour")).toBe(true);
    expect(box?.classList.contains("kind-trigger")).toBe(true);
  });

  it("dims when a trace excludes it", () => {
    const { container } = render(ConstructBody, {
      props: { node: node("entity", "Book", { type: "none" }), dimmed: true },
    });
    expect(container.querySelector(".construct")?.classList.contains("dimmed")).toBe(true);
  });
});
