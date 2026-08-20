import { describe, expect, it } from "vitest";

import type { Health } from "./api/Health";
import { standing } from "./reload";

function health(partial: Partial<Health> = {}): Health {
  return {
    ok: true,
    revision: 1,
    allium_version: "allium 3.5.3",
    modules: ["catalogue"],
    error: null,
    errors: 0,
    warnings: 0,
    ...partial,
  };
}

describe("standing", () => {
  it("is not stale on the first answer", () => {
    // There is nothing older on screen to replace.
    expect(standing(null, health({ revision: 7 })).stale).toBe(false);
  });

  it("is stale once the revision moves", () => {
    expect(standing(7, health({ revision: 8 })).stale).toBe(true);
  });

  it("is not stale while the revision holds still", () => {
    // Otherwise every poll would refetch the whole graph, once a second,
    // forever.
    expect(standing(7, health({ revision: 7 })).stale).toBe(false);
  });

  it("is stale if the revision went backwards, which means a new server", () => {
    // The process was restarted under the same port. The graph on screen came
    // from a server that no longer exists.
    expect(standing(9, health({ revision: 1 })).stale).toBe(true);
  });

  it("reports the revision it was told about", () => {
    expect(standing(1, health({ revision: 4 })).revision).toBe(4);
  });

  it("has nothing to say about a spec in good order", () => {
    expect(standing(1, health()).trouble).toBeNull();
  });

  it("says a failed read left the picture behind", () => {
    // The reader's next move depends on this: the graph is from before the
    // edit, and nothing about it can be trusted to describe the file.
    const trouble = standing(1, health({ ok: false, error: "expected '{'" })).trouble;
    expect(trouble?.headline).toBe("The last read of the spec failed.");
    expect(trouble?.detail).toBe("expected '{'");
    expect(trouble?.showingOlder).toBe(true);
  });

  it("says a spec with errors is still the current picture", () => {
    // Different problem, different next move. Allium describes a file it could
    // not fully parse, so the graph rebuilt and is an honest picture of a spec
    // with mistakes in it — not a stale one.
    const trouble = standing(1, health({ ok: false, errors: 18 })).trouble;
    expect(trouble?.headline).toBe("The spec has 18 errors.");
    expect(trouble?.showingOlder).toBe(false);
  });

  it("counts one error as one error", () => {
    expect(standing(1, health({ ok: false, errors: 1 })).trouble?.headline).toBe(
      "The spec has 1 error.",
    );
  });

  it("leads with the failed read when both are true", () => {
    // The stale graph is the more serious of the two: its error count describes
    // a file that is no longer on disk.
    const trouble = standing(1, health({ ok: false, error: "boom", errors: 3 })).trouble;
    expect(trouble?.showingOlder).toBe(true);
  });

  it("says nothing about warnings", () => {
    // Warnings are the normal state of a spec under development. A banner for
    // them would be up permanently and would stop meaning anything.
    expect(standing(1, health({ warnings: 12 })).trouble).toBeNull();
  });
});
