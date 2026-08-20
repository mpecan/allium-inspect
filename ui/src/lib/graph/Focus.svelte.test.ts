// @vitest-environment happy-dom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import type { Edge } from "../api/Edge";
import type { EdgeKind } from "../api/EdgeKind";
import type { Node } from "../api/Node";
import Focus from "./Focus.svelte";

function node(kind: Node["kind"], name: string, module = "lending"): Node {
  return {
    id: `${module}::${kind}::${name}`,
    kind,
    name,
    module,
    qualified: `${module}/${name}`,
    span: null,
    detail: { type: "none" },
  };
}

function edge(from: Node, to: Node, kind: EdgeKind): Edge {
  return { from: from.id, to: to.id, kind, label: kind, span: null };
}

const shelf = node("surface", "MemberShelf");
const borrows = node("trigger", "MemberBorrows");
const rule = node("rule", "BorrowCopy");
const loan = node("entity", "Loan");
const lonely = node("entity", "Ledger");

const spec = [shelf, borrows, rule, loan, lonely];
const links = [
  edge(shelf, borrows, "provides"),
  edge(borrows, rule, "triggers"),
  edge(rule, loan, "creates"),
];

function open(subject: Node, handlers: Partial<Record<string, () => void>> = {}) {
  const onclose = vi.fn();
  const onopen = vi.fn();
  const onselect = vi.fn();
  render(Focus, {
    props: {
      node: subject,
      nodes: spec,
      edges: links,
      severities: new Map(),
      onselect,
      onopen,
      onclose,
      ...handlers,
    },
  });
  return { onclose, onopen, onselect };
}

const form = (label: string) => screen.getByRole("button", { name: label });

describe("Focus", () => {
  it("names the construct it is a view of", () => {
    open(rule);
    expect(screen.getByRole("heading", { name: "BorrowCopy" })).toBeTruthy();
    expect(screen.getByText("lending/BorrowCopy")).toBeTruthy();
  });

  it("opens on the form that answers for anything connected at all", () => {
    // Forward from a terminal entity is empty and so is backward from a
    // surface, so opening on either would show a reader an empty pop-up for a
    // construct that plainly has neighbours.
    open(loan);
    expect(form("Adjacent").getAttribute("aria-current")).toBe("true");
  });

  it("says how many constructs it is showing", () => {
    open(rule);
    // MemberBorrows one way, Loan the other.
    expect(screen.getByText(/^2 connected/)).toBeTruthy();
  });

  it("turns off a form that has nothing to show, and says why", () => {
    // A dead button that looks live costs a click and teaches nothing.
    open(shelf);
    const backward = form("Leads here") as HTMLButtonElement;
    expect(backward.disabled).toBe(true);
    expect(backward.title).toBe("Nothing in the spec leads to MemberShelf");
  });

  it("counts the answer in the tooltip of a form that has one", () => {
    open(shelf);
    expect(form("Follows").title).toBe("3 — what happens after this");
  });

  it("switches which question it is answering", async () => {
    open(rule);
    await fireEvent.click(form("Follows"));
    expect(form("Follows").getAttribute("aria-current")).toBe("true");
    // Forward from the rule reaches only the loan it creates.
    expect(screen.getByText(/^1 connected/)).toBeTruthy();
  });

  it("says so plainly for a construct joined to nothing", () => {
    // Rather than an empty canvas, which reads as a rendering failure.
    open(lonely);
    expect(screen.getByText(/Nothing in the spec is next to/)).toBeTruthy();
    for (const label of ["Adjacent", "Follows", "Leads here"]) {
      expect((form(label) as HTMLButtonElement).disabled).toBe(true);
    }
  });

  it("closes on the close control", async () => {
    const { onclose } = open(rule);
    await fireEvent.click(screen.getByTitle("Close"));
    expect(onclose).toHaveBeenCalled();
  });
});
