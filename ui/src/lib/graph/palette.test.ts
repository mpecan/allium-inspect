import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

// Read from beside this file rather than from the working directory: `just
// ui-test` and a hand-run `vitest` do not always start from the same place, and
// a stylesheet that silently reads as empty makes every assertion below pass
// vacuously. Not `?raw` — Vite's style pipeline resolves that to an empty
// string, which is exactly the vacuous case.
const THEME = readFileSync(join(import.meta.dirname, "../../theme.css"), "utf8");

import type { Node } from "../api/Node";
import type { NodeKind } from "../api/NodeKind";
import type { ViewKind } from "../client";
import { familyOf } from "./layout";
import { inView } from "./views";

/**
 * How far apart two colours have to be, by the redmean approximation.
 *
 * Not a round number somebody liked: it is just under the closest pair the
 * palette actually contains, so the gate holds the current design rather than
 * permitting it to drift toward a collision one kind at a time.
 */
const SEPARATION = 70;

const VIEWS: ViewKind[] = ["domain", "flow", "lifecycle", "chain"];

// The palette lives in CSS, so this reads the CSS. A colour contract that is
// only checked by looking at the screen is one that drifts the first time a
// kind is added — and the failure is silent: the new kind renders with no
// accent, which looks like a deliberate grey rather than like a bug.
const KINDS: NodeKind[] = [
  "entity",
  "value",
  "variant",
  "enum",
  "rule",
  "trigger",
  "surface",
  "actor",
  "config",
  "invariant",
  "external",
];

/** Every `--kind-x: #rrggbb` declaration, in file order. */
function declarations(): { kind: string; colour: string }[] {
  return [...THEME.matchAll(/--kind-([a-z]+):\s*(#[0-9a-f]{6})/g)].map((match) => ({
    kind: match[1] ?? "",
    colour: match[2] ?? "",
  }));
}

/** A node of `kind`, with just enough on it for `inView` to decide. */
function sample(kind: NodeKind): Node {
  return {
    id: `m::${kind}::X`,
    kind,
    name: "X",
    module: "m",
    qualified: "m/X",
    span: null,
    prose: { note: [], guidance: [] },
    // The lifecycle view draws only entities that have one, and this test is
    // about which kinds it *can* draw.
    detail:
      kind === "entity"
        ? {
            type: "entity",
            kind: "internal",
            parent: null,
            fields: [],
            transitions: [
              { field: "status", states: ["a", "b"], edges: [{ from: "a", to: "b" }], terminal: [] },
            ],
          }
        : { type: "none" },
  };
}

/** Every unordered pair from `items`. */
function pairs<T>(items: T[]): [T, T][] {
  return items.flatMap((a, at) => items.slice(at + 1).map((b) => [a, b] as [T, T]));
}

/** The colours declared for `kind`, one per theme. */
function coloursOf(kind: string): string[] {
  return declarations()
    .filter((each) => each.kind === kind)
    .map((each) => each.colour);
}

describe("the construct palette", () => {
  it("gives every node kind a colour of its own", () => {
    for (const kind of KINDS) {
      expect(coloursOf(kind).length, `--kind-${kind}`).toBeGreaterThan(0);
    }
  });

  it("declares each kind once per theme, and there are three", () => {
    // Dark, the `prefers-color-scheme` light block, and the explicit
    // `[data-theme="light"]` one. A kind declared in two of the three renders
    // with no accent for whichever viewer lands on the third.
    for (const kind of KINDS) {
      expect(coloursOf(kind).length, `--kind-${kind}`).toBe(3);
    }
  });

  it("gives no two kinds the same colour within a theme", () => {
    // The whole point. Triggers read as rules and enums as entities when two
    // kinds share a hue, and a reader scanning a Flow view for "what fires
    // this" has nothing to scan for.
    const perTheme = KINDS.map((kind) => coloursOf(kind));
    for (const theme of [0, 1, 2]) {
      const colours = perTheme.map((each) => each[theme]);
      expect(new Set(colours).size, `theme ${theme}: ${colours.join(" ")}`).toBe(
        KINDS.length,
      );
    }
  });

  it("keeps any two kinds that share a view apart, in every theme", () => {
    // The real rule, and the reason the bands can share a hue family at all:
    // two kinds only have to be told apart if a reader can see both at once.
    // An entity and an invariant may sit close together, because no view draws
    // both. An entity and an enum may not — the Lifecycle view is nothing but
    // those two.
    for (const view of VIEWS) {
      const drawn = KINDS.filter((kind) => inView(sample(kind), view));
      for (const [a, b] of pairs(drawn)) {
        for (const theme of [0, 1, 2]) {
          const first = coloursOf(a)[theme] ?? "";
          const second = coloursOf(b)[theme] ?? "";
          expect(
            distance(first, second),
            `${view} theme ${theme}: ${a} ${first} vs ${b} ${second}`,
          ).toBeGreaterThan(SEPARATION);
        }
      }
    }
  });

  it("tells apart the two pairs the palette was widened for", () => {
    // Both were one colour. Named here so that a future narrowing shows up as
    // this test rather than as somebody noticing on screen months later.
    for (const [a, b] of [
      ["trigger", "rule"],
      ["enum", "entity"],
    ] as const) {
      for (const theme of [0, 1, 2]) {
        const first = coloursOf(a)[theme] ?? "";
        const second = coloursOf(b)[theme] ?? "";
        expect(distance(first, second), `${a} vs ${b}`).toBeGreaterThan(SEPARATION);
      }
    }
  });

  it("keeps each band's anchor pointing at a kind rather than at a literal", () => {
    // `--thing: var(--kind-entity)`. Written as a hex it would be a second
    // place to change, and the two would part company the first time one moved.
    for (const [family, kind] of [
      ["thing", "entity"],
      ["behaviour", "rule"],
      ["boundary", "surface"],
      ["constraint", "invariant"],
    ] as const) {
      const anchors = [...THEME.matchAll(new RegExp(`--${family}:\\s*([^;]+);`, "g"))];
      expect(anchors.length, `--${family}`).toBe(3);
      for (const anchor of anchors) {
        expect(anchor[1]?.trim()).toBe(`var(--kind-${kind})`);
      }
    }
  });

  it("colours every kind the family function knows about", () => {
    // The two are separate on purpose — grouping and differentiation do
    // different jobs — but a kind in one and not the other is a gap.
    for (const kind of KINDS) {
      expect(familyOf(kind)).toBeTruthy();
    }
  });
});

/**
 * How different two colours look, by the redmean approximation.
 *
 * Plain Euclidean distance in RGB badly underrates a difference in saturation,
 * which is exactly the axis a palette built from four hue bands moves along —
 * it would have called a muted sage and a vivid mint nearly the same colour.
 * Redmean weights the channels by where in the red range the pair sits, and
 * costs one square root.
 */
function distance(a: string, b: string): number {
  const channels = (hex: string) =>
    [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16));
  const [ar = 0, ag = 0, ab = 0] = channels(a);
  const [br = 0, bg = 0, bb = 0] = channels(b);
  const red = (ar + br) / 2;
  return Math.sqrt(
    (2 + red / 256) * (ar - br) ** 2 +
      4 * (ag - bg) ** 2 +
      (2 + (255 - red) / 256) * (ab - bb) ** 2,
  );
}
