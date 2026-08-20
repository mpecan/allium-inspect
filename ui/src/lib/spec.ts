// Questions the UI asks of a loaded graph.
//
// Small, pure, and here rather than in a component so they can be tested as
// functions instead of by rendering a canvas and reading pixels back.

import type { Diagnostic } from "./api/Diagnostic";
import type { Node } from "./api/Node";
import type { Position } from "./api/Position";
import type { Severity } from "./api/Severity";
import type { Span } from "./api/Span";
import type { SpecGraph } from "./api/SpecGraph";

/**
 * The diagnostics reported against `node`.
 *
 * Joined on the server's own attribution, which is the same key the badge on
 * the canvas is drawn from. They used to disagree: the badge matched
 * `diagnostic.node` and the panel matched the line number, and Allium reports a
 * diagnostic on the offending line *inside* a construct rather than on its
 * declaration — `IdentityRetirement` is declared at line 530 and its two
 * lifecycle warnings sit at 534. So the line never matched, and the badge
 * promised something the panel could not produce.
 */
export function reportedAgainst(
  diagnostics: readonly Diagnostic[],
  node: Node | null,
): Diagnostic[] {
  if (node === null) {
    return [];
  }
  return diagnostics.filter((diagnostic) => diagnostic.node === node.id);
}

/**
 * The diagnostics the server could not attribute to any construct.
 *
 * A parse error is reported where the parser gave up rather than where the
 * mistake is, so it belongs to a file and not to a declaration. Nothing on the
 * canvas can carry it, which is why there is somewhere else to read it.
 */
export function unattributed(diagnostics: readonly Diagnostic[]): Diagnostic[] {
  return diagnostics.filter((diagnostic) => !diagnostic.node);
}

/** How much a severity matters, for picking the worst of several. */
const RANK: Record<Severity, number> = { info: 0, warning: 1, error: 2 };

/**
 * The worst thing reported against each construct.
 *
 * The attribution itself is the server's: it has the spec text and the byte
 * spans, so it can say which declaration a line falls inside. Doing it here
 * would mean fetching every spec file before the first badge could be drawn,
 * and guessing in the meantime.
 *
 * A diagnostic the server could not attribute to any construct — a parse error
 * reported where the parser gave up rather than where the mistake is — carries
 * no node and is counted only in the module's total.
 */
export function worstByNode(graph: SpecGraph): Map<string, Severity> {
  const worst = new Map<string, Severity>();
  for (const diagnostic of graph.diagnostics) {
    if (!diagnostic.node) {
      continue;
    }
    const current = worst.get(diagnostic.node);
    if (!current || RANK[diagnostic.severity] > RANK[current]) {
      worst.set(diagnostic.node, diagnostic.severity);
    }
  }
  return worst;
}

/** The worst thing reported in each module, for the module list. */
export function worstByModule(graph: SpecGraph): Map<string, Severity> {
  const worst = new Map<string, Severity>();
  for (const diagnostic of graph.diagnostics) {
    const current = worst.get(diagnostic.module);
    if (!current || RANK[diagnostic.severity] > RANK[current]) {
      worst.set(diagnostic.module, diagnostic.severity);
    }
  }
  return worst;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * The one-based line and column of a span's start.
 *
 * Two different units meet here and neither can be assumed. The parser reports
 * *byte* offsets; JavaScript indexes strings in UTF-16 units; a person reading
 * an editor's status bar expects a column counted in *characters*. All three
 * agree for ASCII, which is why conflating them survives every test written
 * against an ASCII fixture and then puts the caret in the wrong place on the
 * first line of prose containing an em-dash.
 *
 * So the prefix is cut in bytes and then decoded, and the column is counted in
 * characters over what comes back.
 */
export function positionOf(text: string, span: Span | null): Position | null {
  if (!span) {
    return null;
  }
  const bytes = encoder.encode(text);
  const upto = decoder.decode(bytes.slice(0, Math.min(Math.max(span.start, 0), bytes.length)));
  const lines = upto.split("\n");
  const last = lines.at(-1) ?? "";
  return { line: lines.length, column: [...last].length + 1 };
}
