// What a construct shows on the canvas, as opposed to in the inspector.
//
// A node is a box roughly two centimetres wide. The question it has to answer
// is "what is this and should I click it?", not "what does it do" — so each
// kind gets the two or three facts that distinguish it from its neighbours, and
// everything else waits for the inspector.
//
// The choices are per kind and each has a reason:
//
//   entity     its fields, because that is what an entity *is*
//   enum       its values, for the same reason
//   rule       its trigger and what it produces, because a rule is read by
//              what makes it fire and what changes afterwards
//   trigger    how it happens — an external stimulus and a state condition are
//              driven by completely different things
//   surface    who faces it, because a boundary without a party is meaningless
//   invariant  what it constrains
//
// Kept out of the component so it can be tested as a table rather than by
// rendering eleven boxes and reading their text back.

import type { Node } from "../api/Node";

/** One line inside a node on the canvas. */
export interface Row {
  label: string;
  value?: string;
  /** Shown in a quieter style: a count or a note rather than a name. */
  muted?: boolean;
}

/** How many rows any node will show. */
const LIMIT = 5;

/** The rows to draw inside `node`. */
export function summaryRows(node: Node): Row[] {
  const detail = node.detail;

  switch (detail.type) {
    case "entity": {
      const rows = detail.fields
        .slice(0, LIMIT)
        .map((field) => ({
          label: field.name,
          value: field.enum_values.length > 0 ? states(field.enum_values) : field.type_expr,
          muted: field.derived,
        }));
      return withOverflow(rows, detail.fields.length, "field");
    }

    case "enum":
      return withOverflow(
        detail.values.slice(0, LIMIT).map((value) => ({ label: value })),
        detail.values.length,
        "value",
      );

    case "config":
      return withOverflow(
        detail.parameters.slice(0, LIMIT).map((parameter) => ({
          label: parameter.name,
          value: parameter.default_expr ?? parameter.type_expr,
        })),
        detail.parameters.length,
        "parameter",
      );

    case "rule": {
      const rows: Row[] = [];
      const requires = detail.clauses.filter((c) => c.keyword === "requires").length;
      if (requires > 0) {
        rows.push({ label: "requires", value: String(requires), muted: true });
      }
      for (const entity of detail.creates.slice(0, 2)) {
        rows.push({ label: "creates", value: entity });
      }
      for (const trigger of detail.emits.slice(0, 2)) {
        rows.push({ label: "emits", value: trigger });
      }
      return rows;
    }

    case "trigger":
      return [
        {
          label: detail.source === "external" ? "stimulus" : `${detail.source} of`,
          value: detail.entity ?? detail.parameters.slice(0, 2).join(", "),
          muted: true,
        },
      ];

    case "surface": {
      const rows: Row[] = [];
      if (detail.actor) {
        rows.push({ label: "facing", value: detail.actor });
      }
      if (detail.provides.length > 0) {
        rows.push({
          label: "provides",
          value: String(detail.provides.length),
          muted: true,
        });
      }
      return rows;
    }

    case "actor":
      return detail.entity ? [{ label: "is", value: detail.entity }] : [];

    case "invariant":
      return detail.entities.length > 0
        ? [{ label: "over", value: detail.entities.join(", ") }]
        : [{ label: "prose only", muted: true }];

    case "none":
      // An unresolved reference. Saying so is the point of drawing it at all.
      return node.kind === "external" ? [{ label: "not declared", muted: true }] : [];
  }
}

/** `listed | withdrawn`, truncated to what fits. */
function states(values: string[]): string {
  return values.length <= 2
    ? values.join(" | ")
    : `${values.slice(0, 2).join(" | ")} +${values.length - 2}`;
}

/** Append a `+n more` row when the list was cut short. */
function withOverflow(rows: Row[], total: number, noun: string): Row[] {
  if (total <= rows.length) {
    return rows;
  }
  const hidden = total - rows.length;
  return [
    ...rows,
    { label: `+${hidden} more ${noun}${hidden === 1 ? "" : "s"}`, muted: true },
  ];
}
