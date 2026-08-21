// Turning a byte span into lines a person can read.
//
// The server sends whole spec files and byte offsets. The strip shows numbered
// lines with the declaration highlighted, and needs enough context around it to
// be worth reading — a span on its own is a fragment, and the whole file is a
// scroll hunt.
//
// **Byte offsets, not string indices.** The parser counts bytes; JavaScript
// strings are indexed in UTF-16 units. For pure ASCII the two agree, which is
// exactly why this is easy to get wrong and hard to notice — and then a spec
// with an em-dash in a comment silently shifts every highlight below it by two.
// The specs this tool is pointed at are full of prose comments, so the drift is
// tens of characters by the middle of a file. Every offset here is therefore
// resolved against a per-line byte index rather than by slicing the string.
//
// The rest is arithmetic worth testing on its own: a span at the very start of
// a file, a span past its end, a stale span from a file edited since the read.
// All of those happen while a spec is being written with the tool open.

import type { Span } from "../api/Span";

/** One numbered line, and how the span covers it. */
export interface SourceLine {
  number: number;
  text: string;
  /** Whether the span covers any of this line. */
  highlit: boolean;
  /** Whether this is the line the declaration starts on. */
  opens: boolean;
}

/** A window onto a file, centred on a span. */
export interface SourceView {
  lines: SourceLine[];
  /** The one-based number of the first line shown, or 0 when nothing is. */
  firstLine: number;
}

/** Lines of context kept above the span when the strip is open. */
const LEAD = 2;

const encoder = new TextEncoder();

/**
 * The byte offset each line starts at.
 *
 * One pass per file rather than per lookup, and the only place the byte/UTF-16
 * distinction has to be handled — everything downstream compares byte offsets
 * to byte offsets.
 */
function lineStarts(lines: string[]): number[] {
  const starts: number[] = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    // +1 for the newline that `split` removed. A file whose last line has no
    // newline over-counts by one at the very end, which cannot move any line
    // boundary that exists.
    offset += encoder.encode(line).length + 1;
  }
  return starts;
}

/**
 * The byte span covering one 1-based line, for highlighting a whole line.
 *
 * The inverse of what the rest of this module does. A construct in a spec
 * arrives with a byte span the parser measured; a journey arrives with a line
 * number, because that is what a reader cites and what the walk records. This
 * turns the second into the first so both go through the same strip.
 *
 * Bytes throughout, like everything else here — `encoder.encode(...).length`
 * rather than `String.length`, which agree only for ASCII.
 */
export function spanOfLine(text: string, line: number): Span | null {
  const lines = text.split("\n");
  const at = line - 1;
  const found = lines[at];
  if (found === undefined) {
    return null;
  }
  const starts = lineStarts(lines);
  const start = starts[at] ?? 0;
  return { start, end: start + encoder.encode(found).length };
}

/** The zero-based line containing byte `offset`. */
function lineOf(starts: number[], offset: number): number {
  let low = 0;
  let high = starts.length - 1;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if ((starts[mid] ?? 0) <= offset) {
      low = mid;
    } else {
      high = mid - 1;
    }
  }
  return low;
}

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

  const lines = text.split("\n");
  const starts = lineStarts(lines);
  const total = encoder.encode(text).length;

  // A span from a file edited since the read can point past the end. It is
  // clamped rather than rejected: the file is still worth showing, and the last
  // line is the closest true answer to "around here".
  const start = Math.min(Math.max(span.start, 0), total);
  const end = Math.min(Math.max(span.end, start), total);

  const startLine = lineOf(starts, start);
  const endLine = lineOf(starts, end === start ? end : end - 1);

  // Context above, then as much of the declaration as the budget allows. The
  // lead is dropped rather than the declaration when the budget is tight: a
  // one-line strip should show the thing itself, not the comment above it.
  const lead = budget > LEAD * 2 ? LEAD : 0;
  const from = Math.max(0, startLine - lead);
  const to = Math.min(lines.length, from + budget);

  return {
    lines: lines.slice(from, to).map((line, index) => {
      const number = from + index + 1;
      const highlit = number >= startLine + 1 && number <= endLine + 1;
      return { number, text: line, highlit, opens: highlit && number === startLine + 1 };
    }),
    firstLine: startLine + 1,
  };
}
