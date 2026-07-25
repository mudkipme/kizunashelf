import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

import type { SmartFilterRule } from "@/types/api";

export const opSymbols: Record<string, string> = {
  eq: "=",
  ne: "≠",
  gt: ">",
  gte: "≥",
  lt: "<",
  lte: "≤",
};

export const unitSymbols: Record<string, string> = {
  days: "d",
  weeks: "w",
  months: "M",
  years: "y",
};

// The connective words of the criteria chips, as lazy descriptors: macros
// don't transform inside plain helper functions, so `formatRule` resolves
// these through the component-scoped `t` instead. All of them join a field
// name and a value into a compact phrase like `genres contains comedy`.
const ruleWords = {
  not: msg({
    comment:
      "Negation prefix in a filter-criteria chip, e.g. 'not genres contains comedy'",
    message: "not",
  }),
  yes: msg({
    comment: "Value of a boolean field in a filter-criteria chip, e.g. 'favorite = yes'",
    message: "yes",
  }),
  no: msg({
    comment: "Value of a boolean field in a filter-criteria chip, e.g. 'favorite = no'",
    message: "no",
  }),
  contains: msg({
    comment:
      "Verb between a field name and values in a filter-criteria chip, e.g. 'genres contains comedy, drama' (any of the values)",
    message: "contains",
  }),
  containsAll: msg({
    comment:
      "Verb between a field name and values in a filter-criteria chip, e.g. 'genres contains all comedy, drama' (every value required)",
    message: "contains all",
  }),
  startsWith: msg({
    comment: "Verb in a filter-criteria chip, e.g. 'title starts with My'",
    message: "starts with",
  }),
  endsWith: msg({
    comment: "Verb in a filter-criteria chip, e.g. 'title ends with !'",
    message: "ends with",
  }),
  hasValue: msg({
    comment: "Predicate after a field name in a filter-criteria chip, e.g. 'rating has a value'",
    message: "has a value",
  }),
  isEmpty: msg({
    comment: "Predicate after a field name in a filter-criteria chip, e.g. 'rating is empty'",
    message: "is empty",
  }),
  linksTo: msg({
    comment:
      "Verb before an entity name in a filter-criteria chip, e.g. 'links to Kyoto Animation'",
    message: "links to",
  }),
  inFolder: msg({
    comment: "Preposition before a folder path in a filter-criteria chip, e.g. 'in Media/Anime'",
    message: "in",
  }),
};

/// One rule as a compact human-readable chip. Field names are user data (never
/// localized); the connective words are. Relative dates use the same compact
/// notation as the file (`today − 90d`), which is language-neutral.
export function formatRule(
  rule: SmartFilterRule,
  t: (descriptor: MessageDescriptor) => string,
): string {
  const field = rule.field ?? "";
  const values = rule.values?.join(", ") ?? "";
  const not = rule.negated ? `${t(ruleWords.not)} ` : "";
  switch (rule.kind) {
    case "compare": {
      const op = opSymbols[rule.op ?? "eq"] ?? "=";
      let value: string;
      if (rule.relative) {
        const base = field === "file.mtime" ? "now" : "today";
        const offset = `${rule.relative.amount}${unitSymbols[rule.relative.unit] ?? rule.relative.unit}`;
        value =
          rule.relative.amount === 0
            ? base
            : `${base} ${rule.relative.future ? "+" : "−"} ${offset}`;
      } else if (rule.date) {
        value = rule.date;
      } else if (rule.number !== undefined && rule.number !== null) {
        value = String(rule.number);
      } else if (rule.boolean !== undefined && rule.boolean !== null) {
        value = rule.boolean ? t(ruleWords.yes) : t(ruleWords.no);
      } else {
        value = rule.value ?? "";
      }
      return `${field} ${op} ${value}`;
    }
    case "contains":
      return rule.mode === "all"
        ? `${not}${field} ${t(ruleWords.containsAll)} ${values}`
        : `${not}${field} ${t(ruleWords.contains)} ${values}`;
    case "startsWith":
      return `${not}${field} ${t(ruleWords.startsWith)} ${values}`;
    case "endsWith":
      return `${not}${field} ${t(ruleWords.endsWith)} ${values}`;
    case "isEmpty":
      return rule.negated
        ? `${field} ${t(ruleWords.hasValue)}`
        : `${field} ${t(ruleWords.isEmpty)}`;
    case "hasTag":
      return `${not}${(rule.values ?? []).map((tag) => `#${tag}`).join(" ")}`;
    case "linksTo":
      // A scoped rule reads "studio links to X"; the file-wide one just "links to X".
      return field
        ? `${not}${field} ${t(ruleWords.linksTo)} ${values}`
        : `${not}${t(ruleWords.linksTo)} ${values}`;
    case "inFolder":
      return `${not}${t(ruleWords.inFolder)} ${values}`;
    case "unsupported":
      return (rule.raw ?? "").trim().replace(/\s+/g, " ");
    default:
      return values;
  }
}
