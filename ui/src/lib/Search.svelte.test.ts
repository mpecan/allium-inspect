// @vitest-environment happy-dom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import type { Node } from "./api/Node";
import Search from "./Search.svelte";

function node(kind: Node["kind"], name: string, module = "catalogue"): Node {
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

const spec = [
  node("entity", "Copy"),
  node("entity", "CopyRequest"),
  node("rule", "BorrowCopy", "lending"),
];

function open(onpick = vi.fn()) {
  render(Search, { props: { nodes: spec, onpick } });
  const field = screen.getByRole("searchbox") as HTMLInputElement;
  const type = async (text: string) => {
    field.value = text;
    await fireEvent.input(field);
  };
  const press = (key: string, on: Element = field) => fireEvent.keyDown(on, { key });
  return { onpick, field, type, press };
}

/** The construct names on offer, in the order they are offered. */
const listed = () =>
  screen
    .queryAllByRole("button")
    .map((button) => button.querySelector(".found")?.textContent?.trim() ?? "");

describe("Search", () => {
  it("lists nothing until something is typed", () => {
    open();
    expect(listed()).toEqual([]);
  });

  it("lists what matches, best first", async () => {
    const { type } = open();
    await type("copy");
    expect(listed()).toEqual(["Copy", "CopyRequest", "BorrowCopy"]);
  });

  it("says where each result lives, because two modules can use one name", async () => {
    const { type } = open();
    await type("borrow");
    expect(screen.getByText("rule · lending")).toBeTruthy();
  });

  it("opens the first result on Enter, so the hands stay on the keys", async () => {
    const { type, press, onpick } = open();
    await type("copy");
    await press("Enter");
    expect(onpick).toHaveBeenCalledWith("catalogue::entity::Copy");
  });

  it("moves down the list with the arrows and opens what is highlighted", async () => {
    const { type, press, onpick } = open();
    await type("copy");
    await press("ArrowDown");
    await press("ArrowDown");
    await press("Enter");
    expect(onpick).toHaveBeenCalledWith("lending::rule::BorrowCopy");
  });

  it("wraps around rather than sticking at the end", async () => {
    const { type, press, onpick } = open();
    await type("copy");
    await press("ArrowUp");
    await press("Enter");
    expect(onpick).toHaveBeenCalledWith("lending::rule::BorrowCopy");
  });

  it("starts each new query at the top", async () => {
    // The row that was highlighted is a different construct now, and opening
    // whatever happens to be in that position is opening the wrong thing.
    const { type, press, onpick } = open();
    await type("copy");
    await press("ArrowDown");
    await type("borrow");
    await press("Enter");
    expect(onpick).toHaveBeenCalledWith("lending::rule::BorrowCopy");
    expect(onpick).toHaveBeenCalledTimes(1);
  });

  it("clears itself on Escape", async () => {
    const { type, press } = open();
    await type("copy");
    expect(listed().length).toBeGreaterThan(0);
    await press("Escape");
    expect(listed()).toEqual([]);
  });

  it("says so when nothing matches, rather than showing an empty list", async () => {
    const { type } = open();
    await type("zzz");
    expect(listed()).toEqual([]);
    expect(screen.getByText(/Nothing in this spec set/)).toBeTruthy();
  });

  it("does nothing on Enter when nothing matched", async () => {
    const { type, press, onpick } = open();
    await type("zzz");
    await press("Enter");
    expect(onpick).not.toHaveBeenCalled();
  });

  it("takes focus when the reader presses / anywhere on the page", async () => {
    const { field, press } = open();
    field.blur();
    await press("/", document.body);
    expect(document.activeElement).toBe(field);
  });

  it("leaves / alone while the reader is already typing in a field", async () => {
    // Otherwise a query cannot contain a slash, and `membership/Group` is how
    // the language itself writes a qualified name.
    const { field, press } = open();
    field.focus();
    const event = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
    field.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    await press("Escape");
  });
});
