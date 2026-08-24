/// Walking `SomebodyMeetsASpecTheyDidNotWrite` on the running tool.
///
/// The journey says what Ines sets out to do; the walk photographs her doing
/// it against the specification of the tool she is doing it with. That is as
/// self-referential as it sounds and it is the point: the pictures under the
/// Journeys view are of this repository's own UI, taken by this repository's
/// own test run, filed under the steps of its own journey.
///
/// **It asserts almost nothing.** A journey's verdicts are the assertion — that
/// is what the walk engine is for — and a browser test that re-litigated them
/// would be a second opinion with no standing. What this establishes is
/// narrower and is the half a walk cannot reach: that a person can get from a
/// directory name to a declaration in its own words, and here is what they saw.
///
/// Steps 1 and 2 are not here. They are command-line acts, and a browser cannot
/// photograph a terminal; they are claimed from a Rust test instead, which is
/// how they read as `claimed` rather than as never covered.

import { expect, test } from "@playwright/test";

import { Frames } from "./frames";

const JOURNEY = "SomebodyMeetsASpecTheyDidNotWrite";
const HERE = "ui/e2e/reading-a-spec.walk.ts";

const frames = Frames.open(process.env.INSPECT_EVIDENCE ?? "../target/evidence");

test.describe.configure({ mode: "serial" });

/// Which way of looking at the tool this run is photographing.
///
/// The project's name, so the axis and its answers come from the config rather
/// than from a second list here that could disagree with it.
function tags(): Record<string, string> {
  return { theme: test.info().project.name };
}

test.beforeEach(() => frames.begin());

test.afterEach(({}, info) => {
  if (info.status !== info.expectedStatus) {
    frames.stopped();
  }
});

// journey: SomebodyMeetsASpecTheyDidNotWrite.3
test("she opens it in a browser", async ({ page }) => {
  await page.goto("/");

  // The Domain view is what a session opens on, which is the step's own claim:
  // `then session.looking_at = domain`.
  const views = page.getByRole("navigation", { name: "Views and filters" });
  await expect(views.getByRole("button", { name: /Domain/ })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.locator(".svelte-flow__node").first()).toBeVisible();

  await frames.take(page, `${JOURNEY}.3`, "the domain view, first paint", `${HERE}:37`, tags());
});

// journey: SomebodyMeetsASpecTheyDidNotWrite.4
test("she picks something that looks load-bearing", async ({ page }) => {
  await page.goto("/");

  const node = page.locator(".svelte-flow__node").first();
  await expect(node).toBeVisible();
  await node.click();

  // `then selection.showing_source = true` — the panel names what was picked.
  const details = page.getByRole("complementary", { name: "Construct details" });
  await expect(details.locator("h2")).toBeVisible();

  await frames.take(page, `${JOURNEY}.4`, "a construct picked, and its fields", `${HERE}:54`, tags());
});

// journey: SomebodyMeetsASpecTheyDidNotWrite.5
test("and reads what the author wrote", async ({ page }) => {
  await page.goto("/");

  await page.locator(".svelte-flow__node").first().click();

  // By position rather than by name: the chevron is `aria-hidden`, so this
  // button's accessible name is the file and line it is pointing at, which is
  // different on every spec set and is the point of the strip.
  const strip = page.locator("section.strip");
  await strip.locator("header button").click();

  // The strip holds text sliced from the file, not a rendering of the AST —
  // which is the whole of what this step demands.
  await expect(strip).toHaveClass(/open/);
  await expect(strip).toContainText(/entity|rule|surface|actor|invariant|enum|config/);

  await frames.take(page, `${JOURNEY}.5`, "the declaration in its own words", `${HERE}:70`, tags());
});
