/// Photographing a walk, into the log `allium-journey evidence seal` reads.
///
/// The harness end of the chain, and deliberately the dumb end: it writes a
/// step id, a file name and a caption, and knows nothing about what that step
/// says. Working out whether a picture still shows what its step demands is
/// `seal`'s job, in Rust, once — a harness that computed it here would be a
/// second implementation of the same question, free to disagree with the first.
///
/// One line a picture, appended as the walk goes. A walk killed half way
/// through is the interesting case: what it managed to photograph before it
/// stopped is exactly the evidence somebody wants, and a document written once
/// at the end would have none of it.

import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Page } from "@playwright/test";

/// What a harness writes; `seal` adds what the step said.
interface Shot {
  step: string;
  image: string;
  caption: string | null;
  passed: boolean;
  taken_at: string;
  source: string | null;
  /// What this picture is *of*, beyond which step: `{ theme: "dark" }`.
  ///
  /// Named rather than bare, which is what lets the panel offer one dropdown
  /// per question instead of a pile of words a reader has to sort into axes
  /// themselves.
  tags: Record<string, string>;
}

const LOG = "frames.jsonl";

export class Frames {
  private taken = 0;
  /// How many pictures the step being walked right now has taken.
  private thisStep = 0;

  private constructor(private readonly dir: string) {}

  /// Ready to append to.
  ///
  /// Emptying the directory is the *script's* job, not this one's. It was here
  /// first and it is wrong here: a config with two projects loads this module
  /// once per project, so the second would wipe the first one's pictures and
  /// the walk would finish holding half of what it took.
  static open(dir: string): Frames {
    mkdirSync(dir, { recursive: true });
    if (!existsSync(join(dir, LOG))) {
      writeFileSync(join(dir, LOG), "");
    }
    return new Frames(dir);
  }

  /// A new step is being walked, so nothing photographed before it is its.
  begin(): void {
    this.thisStep = 0;
  }

  /// One picture of one step.
  ///
  /// Numbered across the whole walk rather than per step, because a directory
  /// listing is sorted and a reader wants the order the walk took them in.
  async take(
    page: Page,
    step: string,
    caption: string,
    source: string,
    tags: Record<string, string> = {},
  ): Promise<string> {
    this.thisStep += 1;
    // The tags are in the file name as well as in the log. Two projects
    // photographing one step would otherwise write the same name twice, and the
    // second would quietly overwrite the first.
    const marks = Object.values(tags).map(slug).join("-");
    const image = `${String(++this.taken).padStart(2, "0")}-${slug(step)}${
      marks === "" ? "" : `-${marks}`
    }.png`;
    await page.screenshot({ path: join(this.dir, image) });

    const shot: Shot = {
      step,
      image,
      caption,
      passed: true,
      taken_at: new Date().toISOString(),
      source,
      tags,
    };
    appendFileSync(join(this.dir, LOG), `${JSON.stringify(shot)}\n`);
    return image;
  }

  /// Mark where a walk stopped.
  ///
  /// The last picture *this step* took is the one at the point of failure, and
  /// it is usually the most informative frame there is — so it is kept and
  /// marked rather than dropped.
  ///
  /// A step that failed before photographing anything marks nothing. The first
  /// version flipped the last frame in the file, which belonged to the step
  /// before — so a walk that broke at step 5 reported step 4 as where it
  /// stopped, and step 4 had been fine. A frame is evidence about the step it
  /// is of, and about no other.
  stopped(): void {
    if (this.thisStep === 0) {
      return;
    }
    const path = join(this.dir, LOG);
    const lines = readFileSync(path, "utf8").split("\n").filter((line) => line.trim() !== "");
    const last = lines.pop();
    if (last === undefined) {
      return;
    }
    const shot = { ...(JSON.parse(last) as Shot), passed: false };
    writeFileSync(path, [...lines, JSON.stringify(shot)].join("\n") + "\n");
  }
}

function slug(step: string): string {
  return step.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}
