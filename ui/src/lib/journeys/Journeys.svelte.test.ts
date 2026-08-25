// @vitest-environment happy-dom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import type { JourneyReport } from "../api/JourneyReport";
import type { Outcome } from "../api/Outcome";
import type { Resolution } from "../api/Resolution";
import type { Verdict } from "../api/Verdict";
import type { Walk } from "../api/Walk";
import type { World } from "../api/World";
import Journeys from "./Journeys.svelte";

/** A world with one member in it, which is all any of these need. */
function world(): World {
  return {
    entities: {
      "Member#1": {
        id: "Member#1",
        entity: "Member",
        module: "lending",
        fields: { name: { kind: "str", value: "Ada" } },
      },
    },
    config: { "lending.loan_limit": { kind: "int", value: 5 } },
    now: 0,
    next_ordinal: { Member: 2 },
    answers: [],
  };
}

function line(
  verdict: Verdict,
  about: string,
  detail: string | null = null,
): Outcome {
  return { line: 4, verdict, about, detail };
}

function walk(
  name: string,
  outcomes: Outcome[],
  stipulated: string[] = [],
  inherited: Walk["inherited"] = [],
): Walk {
  return {
    name,
    cast: [
      {
        name: "ada",
        type_expr: "Member",
        entity: "Member#1",
        origin: "cast",
        line: 4,
      },
    ],
    goal: ["she borrows a copy and brings it back"],
    ends: ["the copy is back on the shelf"],
    line: 6,
    steps: [
      { number: 1, title: "she borrows it", line: 8, outcomes, world: world() },
    ],
    stipulated,
    inherited,
    notes: [],
  };
}

function report(
  walks: Walk[],
  error: string | null = null,
  evidence: Resolution = { steps: {}, unknown: [], axes: {}, undeclared: [] },
): JourneyReport {
  return {
    evidence,
    files: [
      {
        path: "specs/journeys/lending.journey",
        name: "lending.journey",
        error,
        text: "journey ACopyGoesOut {\n    cast:\n        ada: Member\n\n    1. she borrows it\n}\n",
        walks,
      },
    ],
    holding: walks.filter((w) =>
      w.steps.every((s) => s.outcomes.every((o) => o.verdict === "specified")),
    ).length,
    total: walks.length,
  };
}

describe("Journeys", () => {
  it("tells a reader how to load journeys when none are loaded", () => {
    // The state most people meet first, and a blank panel would read as "you
    // have none" rather than "you did not ask for any".
    render(Journeys, { report: report([]), failure: null });
    expect(screen.getByText(/--journeys/)).toBeTruthy();
  });

  it("counts what holds rather than what does not", () => {
    // A journey is the demand written first, so most of it not being supported
    // yet is the ordinary case. Leading with a failure count would frame
    // ordinary progress as a broken build.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.getByText(/1 of 1 hold end to end/)).toBeTruthy();
  });

  it("shows the journey's own goal and ending, not just its verdicts", () => {
    // A list of verdicts with the intent stripped off cannot be read against
    // anything — the reader's question is whether the spec delivers *this*.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(
      screen.getByText(/she borrows a copy and brings it back/),
    ).toBeTruthy();
    expect(screen.getByText(/the copy is back on the shelf/)).toBeTruthy();
  });

  it("shows every line the journey wrote, not only the ones that failed", () => {
    // The terminal report shows only failures because forty passing lines do
    // not fit. The browser has the room, and seeing what the journey actually
    // claimed is half of reading the verdict.
    render(Journeys, {
      report: report([
        walk("A", [
          line("specified", "then loan.status = open"),
          line("refused", "then x = 1", "x is 2"),
        ]),
      ]),
      failure: null,
    });
    expect(screen.getByText("then loan.status = open")).toBeTruthy();
    expect(screen.getByText("then x = 1")).toBeTruthy();
    expect(screen.getByText("x is 2")).toBeTruthy();
  });

  it("leads with what the journey was told rather than shown", () => {
    // The guardrail. An agent can make any journey pass; it cannot make one
    // pass invisibly.
    render(Journeys, {
      report: report([
        walk(
          "A",
          [line("specified", "then x = 1")],
          ["ada.is_at_limit = false"],
        ),
      ]),
      failure: null,
    });
    expect(screen.getByText(/Told rather than shown/)).toBeTruthy();
    expect(screen.getByText("ada.is_at_limit = false")).toBeTruthy();
  });

  it("shows what the file laid out before this journey said anything", () => {
    // The same guardrail one level out. A step holding because of a line
    // somewhere else in the file is passing invisibly until this says so.
    render(Journeys, {
      report: report([
        walk(
          "A",
          [line("specified", "then x = 1")],
          [],
          [{ said: "ada.status = active", line: 4, overridden: false }],
        ),
      ]),
      failure: null,
    });
    expect(screen.getByText(/Laid out by the file/)).toBeTruthy();
    expect(screen.getByText("ada.status = active")).toBeTruthy();
    expect(screen.getByText(/line 4/)).toBeTruthy();
  });

  // The third case, and the dangerous one: inherited, and then quietly made to
  // mean something else.
  it("marks a line the journey inherited and then changed", () => {
    const { container } = render(Journeys, {
      report: report([
        walk(
          "A",
          [line("specified", "then x = 1")],
          [],
          [
            { said: "copy.status = available", line: 5, overridden: true },
            { said: "ada.status = active", line: 4, overridden: false },
          ],
        ),
      ]),
      failure: null,
    });
    // By the mark rather than by the word: the paragraph above explains what
    // "overridden" means and so contains it too.
    const marked = [...container.querySelectorAll(".overridden")];
    expect(marked).toHaveLength(1);
    expect(marked[0]?.closest("li")?.textContent).toContain(
      "copy.status = available",
    );
  });

  it("says nothing about a world when the journey inherits none", () => {
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.queryByText(/Laid out by the file/)).toBeNull();
  });

  it("says nothing about stipulations when a journey made none", () => {
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.queryByText(/Told rather than shown/)).toBeNull();
  });

  it("names a file that would not parse instead of dropping it", () => {
    // A journey silently vanishing from the list reads as "it passed", which is
    // the worst answer available.
    render(Journeys, {
      report: report([], "line 12: expected `<name>: <Type>`"),
      failure: null,
    });
    expect(screen.getByText(/Would not parse/)).toBeTruthy();
    expect(screen.getByText(/line 12/)).toBeTruthy();
  });

  it("says which verdicts a journey carries and how many of each", () => {
    render(Journeys, {
      report: report([
        walk("A", [
          line("specified", "then a = 1"),
          line("specified", "then b = 2"),
          line(
            "unspecified",
            "ada does Nope on Desk",
            "no trigger called `Nope`",
          ),
        ]),
      ]),
      failure: null,
    });
    expect(screen.getByText("the spec does not have this yet")).toBeTruthy();
    expect(screen.getByText("2")).toBeTruthy();
  });

  it("shows the journey's own source, not just what the spec said about it", () => {
    // A journey is a document somebody wrote, the same as the spec it is
    // written against — and it was the one thing in this tool you could not
    // read. The strip carries the address, so the header no longer repeats it.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.getByText("specs/journeys/lending.journey:6")).toBeTruthy();
    expect(screen.getByLabelText("Journey source")).toBeTruthy();
  });

  it("reports its own failure without claiming there are no journeys", () => {
    render(Journeys, { report: null, failure: "Could not load the journeys." });
    expect(screen.getByText("Could not load the journeys.")).toBeTruthy();
    expect(screen.queryByText(/--journeys/)).toBeNull();
  });
});

describe("Journeys cast panel", () => {
  it("names everybody the journey named, with their type", () => {
    // A journey names instances rather than roles, so this is a list of people
    // and things — and the type is what tells a reader where to look them up.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.getByText("ada")).toBeTruthy();
    expect(screen.getByText("Member")).toBeTruthy();
  });

  it("says where each name came from", () => {
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.getByText("cast")).toBeTruthy();
  });

  it("shows the configuration the journey ran against", () => {
    // The other half of why a step came out the way it did. A panel showing
    // Ada's fields but not the limit leaves the deciding value invisible.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.getByText("loan_limit")).toBeTruthy();
    expect(screen.getByText("5")).toBeTruthy();
  });

  it("opens a cast member to show what the world held", async () => {
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    await fireEvent.click(screen.getByRole("button", { name: /ada/ }));
    expect(screen.getByText("name")).toBeTruthy();
    // Quoted, because the simulator's renderer quotes strings — `"5"` and `5`
    // are different values and a field panel is where that matters.
    expect(screen.getByText('"Ada"')).toBeTruthy();
  });

  it("keeps a member closed until it is asked for", () => {
    // Eight fields under every one of six members buries the journey the panel
    // sits beside.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")])]),
      failure: null,
    });
    expect(screen.queryByText('"Ada"')).toBeNull();
  });

  describe("evidence", () => {
    function standing(
      kind: "shown" | "failing" | "stale" | "claimed" | "unclaimed",
      extra: Partial<Resolution["steps"][string]> = {},
    ): Resolution {
      return {
        steps: {
          "ACopyGoesOut.1": {
            standing: kind,
            frames: [],
            claims: [],
            says_now: null,
            ...extra,
          },
        },
        unknown: [],
        axes: {},
        undeclared: [],
      };
    }

    const picture = {
      step: "ACopyGoesOut.1",
      image: "01-she-borrows-it.png",
      caption: "the copy in her hands",
      passed: true,
      taken_at: "2026-08-24T09:00:00Z",
      source: null,
      said: "1. she borrows it",
      tags: {},
    };

    it("shows a picture of a step somebody photographed", () => {
      render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("shown", { frames: [picture] }),
        ),
        failure: null,
      });

      const shot = screen.getByRole("img", { name: "the copy in her hands" });
      expect(shot.getAttribute("src")).toBe(
        "/api/evidence/01-she-borrows-it.png",
      );
    });

    /// The state the source scan exists for, and the one a reader must be able
    /// to tell from silence.
    it("says when a test claims a step and showed nothing", () => {
      render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("claimed", {
            claims: [{ step: "ACopyGoesOut.1", file: "walk.ts", line: 12 }],
          }),
        ),
        failure: null,
      });

      // Twice: once in the count above the walk, once on the step itself.
      expect(screen.getAllByText("claimed")).toHaveLength(2);
      expect(screen.getByText("walk.ts:12")).toBeTruthy();
    });

    it("puts what a stale step said beside what it says now", () => {
      render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("stale", {
            frames: [{ ...picture, said: "1. she takes one off the shelf" }],
            says_now: "1. she borrows it",
          }),
        ),
        failure: null,
      });

      expect(screen.getByText("1. she takes one off the shelf")).toBeTruthy();
      expect(screen.getByText("1. she borrows it")).toBeTruthy();
      expect(screen.getByText(/reworded after this was taken/)).toBeTruthy();
    });

    /// Most steps of most journeys have never been photographed. A strip
    /// saying so under every one of them would be the loudest thing on the
    /// page while carrying nothing.
    it("says nothing at all about a step nobody has shown", () => {
      const { container } = render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("unclaimed"),
        ),
        failure: null,
      });

      expect(container.querySelector(".evidence")).toBeNull();
      expect(screen.queryByText("not shown")).toBeNull();
    });

    it("says nothing when no evidence was loaded at all", () => {
      const { container } = render(Journeys, {
        report: report([
          walk("ACopyGoesOut", [line("specified", "ada does Borrow")]),
        ]),
        failure: null,
      });
      expect(container.querySelector(".evidence")).toBeNull();
    });

    it("opens a picture and closes it again", async () => {
      render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("shown", { frames: [picture] }),
        ),
        failure: null,
      });

      const button = screen.getByRole("button", {
        name: "the copy in her hands",
      });
      expect(button.getAttribute("aria-pressed")).toBe("false");

      await fireEvent.click(button);
      expect(button.getAttribute("aria-pressed")).toBe("true");

      await fireEvent.click(button);
      expect(button.getAttribute("aria-pressed")).toBe("false");
    });

    describe("tags", () => {
      const dark = {
        ...picture,
        image: "01-dark.png",
        tags: { theme: "dark" },
      };
      const light = {
        ...picture,
        image: "01-light.png",
        tags: { theme: "light" },
      };

      function tagged() {
        return report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("shown", { frames: [dark, light] }),
        );
      }

      it("shows every picture of a step until the reader narrows", () => {
        render(Journeys, { report: tagged(), failure: null });
        expect(
          screen.getAllByRole("img", { name: "the copy in her hands" }),
        ).toHaveLength(2);
      });

      it("offers one control per question the pictures answer", () => {
        render(Journeys, { report: tagged(), failure: null });
        const control = screen.getByRole("combobox", { name: "theme" });
        const options = [...control.querySelectorAll("option")].map(
          (o) => o.textContent,
        );
        expect(options).toEqual(["either", "dark", "light"]);
      });

      it("offers nothing to narrow by when nothing is tagged", () => {
        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            standing("shown", { frames: [picture] }),
          ),
          failure: null,
        });
        expect(screen.queryByRole("combobox")).toBeNull();
      });

      it("switches the pictures when the reader picks one", async () => {
        render(Journeys, { report: tagged(), failure: null });

        await fireEvent.change(
          screen.getByRole("combobox", { name: "theme" }),
          {
            target: { value: "light" },
          },
        );

        const shown = screen.getAllByRole("img", {
          name: "the copy in her hands",
        });
        expect(shown).toHaveLength(1);
        expect(shown[0]?.getAttribute("src")).toBe(
          "/api/evidence/01-light.png",
        );
      });

      it("goes back to both when the reader picks either", async () => {
        render(Journeys, { report: tagged(), failure: null });
        const control = screen.getByRole("combobox", { name: "theme" });

        await fireEvent.change(control, { target: { value: "dark" } });
        expect(
          screen.getAllByRole("img", { name: "the copy in her hands" }),
        ).toHaveLength(1);

        await fireEvent.change(control, { target: { value: "" } });
        expect(
          screen.getAllByRole("img", { name: "the copy in her hands" }),
        ).toHaveLength(2);
      });

      it("labels each picture with what it is of", () => {
        const { container } = render(Journeys, {
          report: tagged(),
          failure: null,
        });
        // Scoped to the strip: the words are also in the dropdown, which is a
        // different thing saying a different thing.
        const chips = [...container.querySelectorAll(".tags li")].map(
          (li) => li.textContent,
        );
        expect(chips).toEqual(["theme dark", "theme light"]);
      });

      /// The point of declaring: a journey that says how it should be shown
      /// gets the control before anybody has photographed anything.
      it("offers what the journey asked for before any picture exists", () => {
        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            {
              steps: {},
              unknown: [],
              axes: {
                ACopyGoesOut: [
                  {
                    key: "theme",
                    values: ["dark", "light"],
                    missing: ["dark", "light"],
                    line: 4,
                  },
                ],
              },
              undeclared: [],
            },
          ),
          failure: null,
        });

        const control = screen.getByRole("combobox", { name: "theme" });
        const options = [...control.querySelectorAll("option")].map((o) =>
          o.textContent?.trim(),
        );
        expect(options).toEqual([
          "either",
          "dark — none yet",
          "light — none yet",
        ]);
      });

      it("stops saying none yet once something answers", () => {
        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            {
              steps: {
                "ACopyGoesOut.1": {
                  standing: "shown",
                  frames: [dark],
                  claims: [],
                  says_now: null,
                },
              },
              unknown: [],
              axes: {
                ACopyGoesOut: [
                  {
                    key: "theme",
                    values: ["dark", "light"],
                    missing: ["light"],
                    line: 4,
                  },
                ],
              },
              undeclared: [],
            },
          ),
          failure: null,
        });

        const options = [
          ...screen
            .getByRole("combobox", { name: "theme" })
            .querySelectorAll("option"),
        ].map((o) => o.textContent?.trim());
        expect(options).toEqual(["either", "dark", "light — none yet"]);
      });

      /// The typo the declaration exists to catch. Not styled as a failure:
      /// the picture it names is a real picture of a real run.
      it("reports a tag the journey does not ask for", () => {
        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            {
              steps: {},
              unknown: [],
              axes: {
                ACopyGoesOut: [
                  {
                    key: "theme",
                    values: ["dark", "light"],
                    missing: [],
                    line: 4,
                  },
                ],
              },
              undeclared: [
                {
                  step: "ACopyGoesOut.1",
                  image: "01.png",
                  key: "them",
                  value: "dark",
                  key_undeclared: true,
                },
              ],
            },
          ),
          failure: null,
        });

        expect(screen.getByText("no such tag")).toBeTruthy();
        expect(screen.getByText("them=dark")).toBeTruthy();
        expect(screen.getByText("01.png")).toBeTruthy();
      });

      it("says nothing about another journey's tags", () => {
        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            {
              steps: {},
              unknown: [],
              axes: {},
              undeclared: [
                {
                  step: "Elsewhere.1",
                  image: "99.png",
                  key: "x",
                  value: "y",
                  key_undeclared: true,
                },
              ],
            },
          ),
          failure: null,
        });

        expect(screen.queryByText("no such tag")).toBeNull();
      });

      /// A step that was photographed, just not the way the reader asked to see
      /// it. The standing above still reads `shown` — that is a fact about the
      /// step, not about the filter — so saying nothing here would leave the
      /// panel contradicting itself.
      it("says so when narrowing hides every picture of a step", async () => {
        // Step 1 was only ever photographed dark. `light` is on the axis
        // because another step of the walk was photographed that way.
        const lopsided: Resolution = {
          steps: {
            "ACopyGoesOut.1": {
              standing: "shown",
              frames: [dark],
              claims: [],
              says_now: null,
            },
            "ACopyGoesOut.2": {
              standing: "shown",
              frames: [light],
              claims: [],
              says_now: null,
            },
          },
          unknown: [],
          axes: {},
          undeclared: [],
        };

        render(Journeys, {
          report: report(
            [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
            null,
            lopsided,
          ),
          failure: null,
        });

        expect(
          screen.getAllByRole("img", { name: "the copy in her hands" }),
        ).toHaveLength(1);

        await fireEvent.change(
          screen.getByRole("combobox", { name: "theme" }),
          {
            target: { value: "light" },
          },
        );

        expect(
          screen.queryByRole("img", { name: "the copy in her hands" }),
        ).toBeNull();
        expect(
          screen.getByText(/not the way you have asked to see it/),
        ).toBeTruthy();
        // And the standing is unchanged, because the step has not changed.
        expect(screen.getAllByText("shown").length).toBeGreaterThan(0);
      });
    });

    /// The two counts answer different questions and are drawn apart, so the
    /// header has to carry both without either standing in for the other.
    it("counts what was shown beside what the spec supports", () => {
      render(Journeys, {
        report: report(
          [walk("ACopyGoesOut", [line("specified", "ada does Borrow")])],
          null,
          standing("shown", { frames: [picture] }),
        ),
        failure: null,
      });

      expect(screen.getByText("the spec does this")).toBeTruthy();
      expect(screen.getAllByText("shown")).toHaveLength(2);
    });
  });
});
