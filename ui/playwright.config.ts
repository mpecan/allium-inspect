import { defineConfig } from "@playwright/test";

/// The walk tier: a browser driving the built tool, photographing what it does.
///
/// Deliberately outside `just check`. It needs a browser downloaded, a binary
/// built and a server running, and it takes about as long as the rest of the
/// suite put together — the same reasoning that keeps `just mutants` a decision
/// rather than a step. `just walk` is where it lives.
///
/// It asserts almost nothing on purpose. A journey's verdicts are the
/// assertion; a browser test that re-litigated them would be a second opinion
/// with no standing. What this produces is the half a walk cannot reach: a
/// photograph of the software doing what the step describes.
export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.walk.ts",
  // One at a time. The frames are numbered in the order the walk took them, and
  // two workers appending to one log would interleave them into an order no
  // reader could follow.
  workers: 1,
  fullyParallel: false,
  // A photograph of a flaky moment is worse than no photograph: it would be
  // sealed, shown, and believed.
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: process.env.INSPECT_URL ?? "http://127.0.0.1:7171",
    // The pictures are the product here, so they are taken deliberately rather
    // than harvested from failures.
    screenshot: "off",
    video: "off",
    trace: "off",
    viewport: { width: 1440, height: 900 },
    // Pinned rather than inherited. The tool follows the reader's OS theme, so
    // a walk that took the default would photograph a different-looking product
    // on a laptop set to light than on one set to dark — and the pictures are
    // committed, compared between runs, and read months later. Same reasoning
    // as the ordered maps and the clock-as-a-field: the same walk twice should
    // produce the same evidence.
    colorScheme: "dark",
  },
});
