// Showing a simulated value to a person.
//
// The rule throughout: never render an undecided value as blank or as a dash.
// An empty cell reads as "nothing here", and "the simulator does not know what
// is here" is a completely different statement — it is the one the whole tool is
// built to make clearly. So `unknown` renders as a word, in the loud style, and
// `null` renders as `null` because the spec can and does assert about it.
//
// Durations are shown in the units the spec wrote them in wherever that is
// recoverable, because `1814400000` is not a number anyone reads as three weeks.

import type { Value } from "../api/Value";

/** How a value should be shown. */
export interface Rendered {
  text: string;
  /** Undecided values are styled apart from every other kind. */
  unknown: boolean;
}

/** `value` as text a person can read. */
export function render(value: Value): Rendered {
  switch (value.kind) {
    case "unknown":
      return { text: "unknown", unknown: true };
    case "null":
      return { text: "null", unknown: false };
    case "bool":
      return { text: value.value ? "true" : "false", unknown: false };
    case "int":
      return { text: String(value.value), unknown: false };
    case "float":
      return { text: String(value.value), unknown: false };
    case "str":
      return { text: `"${value.value}"`, unknown: false };
    case "enum":
      return { text: value.value, unknown: false };
    case "duration":
      return { text: duration(value.value), unknown: false };
    case "timestamp":
      return { text: `t+${duration(value.value)}`, unknown: false };
    case "ref":
      return { text: value.value, unknown: false };
    case "set":
      return {
        text:
          value.value.length === 0
            ? "(empty)"
            : `{${value.value.map((item) => render(item).text).join(", ")}}`,
        // A collection holding an undecided element is itself only partly
        // known, and saying so is what stops it reading as a settled list.
        unknown: value.value.some((item) => render(item).unknown),
      };
  }
}

/** Milliseconds in the largest unit that divides them exactly. */
export function duration(millis: number): string {
  if (millis === 0) {
    return "0";
  }
  const units: [number, string][] = [
    [604_800_000, "week"],
    [86_400_000, "day"],
    [3_600_000, "hour"],
    [60_000, "minute"],
    [1_000, "second"],
  ];
  const magnitude = Math.abs(millis);
  for (const [size, name] of units) {
    if (magnitude % size === 0) {
      const count = millis / size;
      return `${count} ${name}${Math.abs(count) === 1 ? "" : "s"}`;
    }
  }
  return `${millis}ms`;
}

/** A value parsed back from what the world editor's input holds.
 *
 * Deliberately conservative: text that is not clearly something else stays a
 * string, because a spec field typed `String` holding `5` is a string, and
 * guessing otherwise would silently change what the simulator is told.
 */
export function parse(text: string, states: string[] = []): Value {
  const trimmed = text.trim();

  if (trimmed === "") {
    // An empty box has not been filled in. That is `unknown` — the simulator
    // has not been told — and emphatically not `null`, which is a claim that
    // the field is empty.
    return { kind: "unknown" };
  }
  if (states.includes(trimmed)) {
    return { kind: "enum", value: trimmed };
  }
  if (trimmed === "null") {
    return { kind: "null" };
  }
  if (trimmed === "true" || trimmed === "false") {
    return { kind: "bool", value: trimmed === "true" };
  }
  if (/^[A-Za-z_][A-Za-z0-9_]*#\d+$/.test(trimmed)) {
    return { kind: "ref", value: trimmed };
  }
  if (/^-?\d+$/.test(trimmed)) {
    return { kind: "int", value: Number(trimmed) };
  }
  if (/^-?\d*\.\d+$/.test(trimmed)) {
    return { kind: "float", value: Number(trimmed) };
  }

  const spelled = /^(-?\d+)\.(millisecond|second|minute|hour|day|week)s?$/.exec(trimmed);
  if (spelled) {
    const sizes: Record<string, number> = {
      millisecond: 1,
      second: 1_000,
      minute: 60_000,
      hour: 3_600_000,
      day: 86_400_000,
      week: 604_800_000,
    };
    return { kind: "duration", value: Number(spelled[1]) * (sizes[spelled[2] ?? ""] ?? 1) };
  }

  return { kind: "str", value: trimmed.replace(/^"|"$/g, "") };
}
