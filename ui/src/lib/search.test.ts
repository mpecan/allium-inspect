import { describe, expect, it } from "vitest";

import type { Node } from "./api/Node";
import { search, SHOWN } from "./search";

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
  node("rule", "QueueOnSend", "delivery"),
  node("entity", "Message", "delivery"),
  node("surface", "Desk", "lending"),
];

const names = (query: string) => search(spec, query).map((match) => match.node.name);

describe("search", () => {
  it("finds a construct by part of its name", () => {
    expect(names("borrow")).toEqual(["BorrowCopy"]);
  });

  it("ignores case, because nobody types the capitals", () => {
    expect(names("BORROWCOPY")).toEqual(["BorrowCopy"]);
  });

  it("puts the exact name first, then what starts with it, then what contains it", () => {
    // Typing a name in full and getting it third is the search telling you it
    // knows better than you do.
    expect(names("copy")).toEqual(["Copy", "CopyRequest", "BorrowCopy"]);
  });

  it("narrows on every term rather than widening", () => {
    // `rule delivery` is one request, not two. Matching either would return
    // most of the spec, which is the state the reader was trying to leave.
    expect(names("rule delivery")).toEqual(["QueueOnSend"]);
  });

  it("lets a term match the kind or the module, not only the name", () => {
    expect(names("surface")).toEqual(["Desk"]);
    // Neither name contains the word, so they rank together and the tie breaks
    // on the id — the same order every time, which is the point.
    expect(names("delivery")).toEqual(["Message", "QueueOnSend"]);
  });

  it("ranks on the term that matched the name best", () => {
    // `delivery` matches both on module alone; `send` is what names one of
    // them, and that is the one the reader meant.
    expect(names("delivery send")).toEqual(["QueueOnSend"]);
  });

  it("matches nothing on an empty query", () => {
    // Listing all three hundred constructs is what the canvas already does.
    for (const query of ["", "   ", "\t"]) {
      expect(search(spec, query)).toEqual([]);
    }
  });

  it("returns nothing rather than something near for a query nothing matches", () => {
    expect(names("zzz")).toEqual([]);
  });

  it("orders identically every time it is asked", () => {
    // Two constructs can match equally well, and a reader who searches twice
    // and clicks the second result has to get the same construct.
    const shuffled = [...spec].reverse();
    expect(search(shuffled, "copy").map((m) => m.node.id)).toEqual(
      search(spec, "copy").map((m) => m.node.id),
    );
  });

  it("reports every match, and leaves the truncation to whoever shows them", () => {
    // The count is worth telling the reader — "12 of 40" is a different
    // situation from "12", and only the full list can say which it is.
    const many = Array.from({ length: 40 }, (_, index) => node("rule", `Rule${index}`));
    expect(search(many, "rule")).toHaveLength(40);
    expect(SHOWN).toBeLessThan(40);
  });
});
