// Comment lines as paragraphs.
//
// The ingestion keeps a note exactly as the author left it: one entry per line,
// with a blank entry where they left a blank line. That is the right shape to
// carry, because it is the shape they wrote — but a paragraph broken at the
// column their editor happened to wrap at, rendered into a panel a third the
// width, reads as a poem.
//
// So the lines are joined and the blanks are where the joining stops.

/** Group `lines` into paragraphs, splitting on the blank ones. */
export function paragraphs(lines: readonly string[]): string[] {
  const found: string[] = [];
  let current: string[] = [];

  for (const line of lines) {
    if (line.trim() === "") {
      if (current.length > 0) {
        found.push(current.join(" "));
        current = [];
      }
      continue;
    }
    current.push(line.trim());
  }
  if (current.length > 0) {
    found.push(current.join(" "));
  }
  return found;
}

/** One run of a paragraph, and how it was marked up. */
export interface Piece {
  kind: "text" | "code" | "strong";
  text: string;
}

/**
 * `code` and `**emphasis**` as the pieces they are.
 *
 * The authors of a real spec set write 593 backticked spans and 75 bold ones
 * into their comments, which is not incidental — `` `expired` `` is a state
 * name and **staggered but whole** is the sentence the paragraph was built to
 * land. Showing the markers instead of what they mark is showing the reader the
 * scaffolding.
 *
 * Two patterns only, and no dependency. Anything else — a link, a list, a
 * heading — stays exactly as written, because guessing at a markup language the
 * spec never claimed to be written in is how a tool starts putting words in an
 * author's mouth. An unmatched marker is literal for the same reason.
 */
export function inline(text: string): Piece[] {
  const pieces: Piece[] = [];
  const pattern = /`([^`]+)`|\*\*([^*]+)\*\*/g;
  let at = 0;

  for (const match of text.matchAll(pattern)) {
    const start = match.index;
    if (start > at) {
      pieces.push({ kind: "text", text: text.slice(at, start) });
    }
    pieces.push(
      match[1] === undefined
        ? { kind: "strong", text: match[2] ?? "" }
        : { kind: "code", text: match[1] },
    );
    at = start + match[0].length;
  }

  if (at < text.length) {
    pieces.push({ kind: "text", text: text.slice(at) });
  }
  return pieces;
}
