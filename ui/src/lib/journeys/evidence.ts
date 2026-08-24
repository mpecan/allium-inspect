// How a step's standing is shown, and where its pictures come from.
//
// Kept out of the component for the same reason `verdicts.ts` is: the mapping
// is a decision rather than markup. The decision here is that a standing is
// *not* a verdict and must not be drawn as one — they answer different
// questions and a reader who conflates them will draw the wrong conclusion in
// both directions.
//
//   a verdict   does the specification support this step
//   a standing  has anybody shown the software doing it
//
// A step can hold and have never been photographed. A step that has been
// photographed can still be one the spec does not support. Neither implies the
// other, so neither substitutes for the other.

import type { Frame } from "../api/Frame";
import type { Resolution } from "../api/Resolution";
import type { Standing } from "../api/Standing";
import type { StepEvidence } from "../api/StepEvidence";

/**
 * What each standing means, in the reader's terms.
 *
 * `claimed` is the one worth the words. A test that says it demonstrates a
 * step and produced no picture is not the same as a step nobody has covered,
 * and the difference is the whole reason the code is scanned at all.
 */
export const MEANING = {
  shown: "the software does this",
  failing: "the run stopped here",
  stale: "the step was reworded after this was taken",
  claimed: "a test says it shows this, and showed nothing",
  unclaimed: "nobody has shown this",
} as const satisfies Record<Standing, string>;

/** The short word the strip is labelled with. */
export const WORD = {
  shown: "shown",
  failing: "stopped here",
  stale: "stale",
  claimed: "claimed",
  unclaimed: "not shown",
} as const satisfies Record<Standing, string>;

/** Whether this standing is one a reader has to do something about. */
export function needsAttention(standing: Standing): boolean {
  return standing === "failing" || standing === "stale" || standing === "claimed";
}

/**
 * Whether a standing is worth any room at all.
 *
 * Most steps of most journeys have never been photographed, and a strip
 * saying so under every one of them would be the loudest thing on the page
 * while carrying no information. Silence is the honest rendering of "nobody
 * has shown this"; the count at the top is where it is accounted for.
 */
export function worthShowing(evidence: StepEvidence | undefined): evidence is StepEvidence {
  return evidence !== undefined && evidence.standing !== "unclaimed";
}

/** The step's entry in the resolution, by the id a marker would spell. */
export function at(
  resolution: Resolution | undefined,
  journey: string,
  step: number,
): StepEvidence | undefined {
  return resolution?.steps[`${journey}.${step}`];
}

/**
 * Where the server will hand over a picture.
 *
 * By image name, which is what the manifest carries and the only thing the
 * route will answer to. Encoded because a name is a path segment and a harness
 * is free to put a space in a caption-derived file name.
 */
export function pictureUrl(frame: Frame): string {
  return `/api/evidence/${encodeURIComponent(frame.image)}`;
}

/**
 * One question a reader can narrow by, and the answers anybody gave to it.
 *
 * A tag is named — `theme: dark` rather than `dark` — and that is what makes a
 * dropdown possible at all. The name says which pictures are *alternatives* to
 * each other: pick `dark` and you are declining `light`, but you have said
 * nothing about `platform`. A flat list of words could not tell a reader which
 * of them were answers to the same question.
 *
 * Derived from what the frames actually carry rather than from a list the tool
 * holds. The tool never learns what `theme` means; anything a harness names
 * becomes an axis, and two harnesses with different vocabularies produce two
 * axes rather than an argument.
 */
export interface Axis {
  key: string;
  values: string[];
}

/** Every axis in the evidence, and the answers given on each, sorted. */
export function axes(resolution: Resolution | undefined): Axis[] {
  const found = new Map<string, Set<string>>();

  for (const step of Object.values(resolution?.steps ?? {})) {
    for (const frame of step.frames) {
      for (const [key, value] of Object.entries(frame.tags)) {
        const values = found.get(key) ?? new Set<string>();
        values.add(value);
        found.set(key, values);
      }
    }
  }

  return [...found.entries()]
    .map(([key, values]) => ({ key, values: [...values].sort() }))
    .filter((axis) => axis.values.length > 0)
    .sort((a, b) => a.key.localeCompare(b.key));
}

/** What the reader has narrowed to: an axis key to one of its values. */
export type Narrowing = Record<string, string>;

/**
 * Whether a picture answers to what the reader asked for.
 *
 * A frame that carries nothing on an axis is shown whatever is picked on it.
 * Silence is not disagreement: a harness that never said which platform it was
 * photographing has not thereby said it was photographing the other one.
 */
export function matches(frame: Frame, narrowing: Narrowing): boolean {
  return Object.entries(narrowing).every(([key, value]) => {
    const carried = frame.tags[key];
    return carried === undefined || carried === value;
  });
}

/** The frames of a step that answer to what the reader asked for. */
export function narrow(evidence: StepEvidence, narrowing: Narrowing): Frame[] {
  return evidence.frames.filter((frame) => matches(frame, narrowing));
}

/** How many steps have been shown, out of how many there are. */
export function tally(resolution: Resolution | undefined): { shown: number; total: number } {
  const steps = Object.values(resolution?.steps ?? {});
  return {
    shown: steps.filter((step) => step.standing === "shown").length,
    total: steps.length,
  };
}

/**
 * The standings in a journey that a reader should be told about, worst first.
 *
 * `unclaimed` is left out for the reason `worthShowing` gives; `shown` is kept,
 * because "four of five steps are shown" is the sentence somebody opening a
 * journey wants and it cannot be said without counting them.
 */
export function summary(
  resolution: Resolution | undefined,
  journey: string,
  steps: number[],
): { standing: Standing; count: number }[] {
  const ORDER: Standing[] = ["failing", "stale", "claimed", "shown"];
  const mine = steps.map((step) => at(resolution, journey, step)?.standing);

  return ORDER.map((standing) => ({
    standing,
    count: mine.filter((each) => each === standing).length,
  })).filter((entry) => entry.count > 0);
}
