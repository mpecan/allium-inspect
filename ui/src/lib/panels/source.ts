// Turning a byte span into lines a person can read.
//
// The server sends whole spec files and byte offsets. The strip shows numbered
// lines with the declaration highlighted, and needs enough context around it to
// be worth reading — a span on its own is a fragment, and the whole file is a
// scroll hunt.
//
// Kept out of the component because the interesting cases are all arithmetic:
// a span at the very start of a file, a span past its end, a stale span from a
// file that has since been edited, a span that splits a multi-byte character.
// Every one of those is a real thing that happens when a spec is being edited
// while the tool is open.

import type { Span } from "../api/Span";

/** One numbered line, and whether the span covers any of it. */
export interface SourceLine {
  number: number;
  text: string;
  highlit: boolean;
}

/** A window onto a file, centred on a span. */
export interface SourceView {
  lines: SourceLine[];
  /** The one-based number of the first line shown, or 0 when nothing is. */
  firstLine: number;
}

/** Lines of context kept above the span when the strip is open. */
const LEAD = 2;

/**
 * The lines `span` covers in `text`, with up to `budget` lines shown.
 *
 * With no span, or a span that does not land in this text, the result is empty
 * rather than the top of the file: showing line 1 of a file whose selection is
 * elsewhere is a quiet lie about where you are.
 */
export function sliceLines(
  text: string,
  span: Span | null,
  budget: number,
): SourceView {
  if (!span || text.length === 0 || budget <= 0) {
    return { lines: [], firstLine: 0 };
  }

  // A span from a file that has since been edited can point past the end. It is
  // clamped rather than rejected: the file is still worth showing, and the last
  // line is the closest true answer to "around here".
  const start = Math.min(span.start, text.length);
  const end = Math.min(Math.max(span.end, start), text.length);

  const lines = text.split("\n");
  const startLine = lineOf(text, start);
  const endLine = lineOf(text, end === start ? end : end - 1);

  // Context above, then as much of the declaration as the budget allows. The
  // lead is dropped rather than the declaration when the budget is tight: a
  // one-line strip should show the thing itself, not the comment above it.
  const lead = budget > LEAD * 2 ? LEAD : 0;
  const from = Math.max(0, startLine - lead);
  const to = Math.min(lines.length, from + budget);

  return {
    lines: lines.slice(from, to).map((line, index) => {
      const number = from + index + 1;
      return {
        number,
        text: line,
        highlit: number >= startLine + 1 && number <= endLine + 1,
      };
    }),
    firstLine: startLine + 1,
  };
}

/** The zero-based line containing byte `offset`. */
function lineOf(text: string, offset: number): number {
  let line = 0;
  for (let index = 0; index < offset && index < text.length; index += 1) {
    if (text[index] === "\n") {
      line += 1;
    }
  }
  return line;
}
