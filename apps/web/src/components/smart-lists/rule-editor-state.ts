//! The translation between a stored rule and the (field, operator, inputs)
//! triple a builder row edits, in both directions.
//!
//! `ruleFromEditor` and `editorFromRule` are inverses over the vocabulary the
//! builder supports; a rule outside it returns `null` from `editorFromRule` and
//! is rendered read-only rather than rewritten, so hand-written criteria survive
//! a round trip through the UI untouched.

import type { MessageDescriptor } from "@lingui/core";

import { builderWords, type RuleFieldMeta } from "@/components/smart-lists/rule-field-meta";
import { seasonsOfYear, wholeYearsOf, yearsFrom } from "@/components/smart-lists/season-values";
import type { SmartFilterRule } from "@/types/api";

/// The single-value and multi-value sides of a season row, each filled from the
/// other when it's empty.
function carriedSeasonInputs(inputs: { values: string[]; text: string }) {
  const text = inputs.text.trim();
  const values = inputs.values.length > 0 ? inputs.values : text ? [text] : [];
  return { values, text: text || values[0] || "" };
}

/// Builds the rule a row's (field, operator, inputs) selection means.
export function ruleFromEditor(
  meta: RuleFieldMeta,
  op: string,
  editorInputs: {
    values: string[];
    text: string;
    number: string;
    date: string;
    amount: string;
    unit: string;
  },
): SmartFilterRule {
  const field = meta.key;
  // A season rule holds one value or several depending on its operator, so the
  // two sides carry into each other: switching "is Spring 2024" to "is any of"
  // keeps that season picked rather than emptying the row.
  const inputs =
    meta.kind === "season"
      ? { ...editorInputs, ...carriedSeasonInputs(editorInputs) }
      : editorInputs;
  const compare = (extra: Partial<SmartFilterRule>): SmartFilterRule => ({
    kind: "compare",
    field,
    negated: false,
    values: [],
    ...extra,
  });
  const relative = (op: "gte" | "lt" | "lte", future: boolean): SmartFilterRule =>
    compare({
      op,
      relative: {
        amount: Math.max(0, Math.floor(Number(inputs.amount) || 0)),
        unit: (["days", "weeks", "months", "years"] as const).includes(
          inputs.unit as "days" | "weeks" | "months" | "years",
        )
          ? (inputs.unit as "days" | "weeks" | "months" | "years")
          : "days",
        future,
      },
    });
  switch (op) {
    case "is":
      return compare({ op: "eq", value: inputs.text });
    case "isNot":
      return compare({ op: "ne", value: inputs.text });
    case "isEmpty":
      return { kind: "isEmpty", field, negated: false, values: [] };
    case "hasValue":
      return { kind: "isEmpty", field, negated: true, values: [] };
    case "containsAny":
    case "isAnyOf":
      return { kind: "contains", field, mode: "any", negated: false, values: inputs.values };
    // "Year is 2024" is stored as that year's four seasons, which is both what
    // Obsidian can evaluate and what works on a multi-season field.
    case "yearIs":
      return {
        kind: "contains",
        field,
        mode: "any",
        negated: false,
        values: yearsFrom(inputs.values).flatMap((year) =>
          seasonsOfYear(year, meta.seasonLanguage ?? "zh"),
        ),
      };
    case "containsAll":
      return { kind: "contains", field, mode: "all", negated: false, values: inputs.values };
    case "notContains":
      return meta.kind === "text"
        ? { kind: "contains", field, mode: "any", negated: true, values: [inputs.text] }
        : { kind: "contains", field, mode: "any", negated: true, values: inputs.values };
    case "isTrue":
      return compare({ op: "eq", boolean: true });
    case "isFalse":
      return compare({ op: "eq", boolean: false });
    case "eq":
    case "ne":
    case "gt":
    case "gte":
    case "lt":
    case "lte":
      return compare({ op, number: Number(inputs.number) || 0 });
    case "on":
      return compare({ op: "eq", date: inputs.date });
    case "before":
      return compare({ op: "lt", date: inputs.date });
    case "onOrBefore":
      return compare({ op: "lte", date: inputs.date });
    case "after":
      return compare({ op: "gt", date: inputs.date });
    case "onOrAfter":
      return compare({ op: "gte", date: inputs.date });
    case "inLast":
      return relative("gte", false);
    case "olderThan":
      return relative("lt", false);
    case "withinNext":
      return relative("lte", true);
    case "contains":
      return { kind: "contains", field, mode: "any", negated: false, values: [inputs.text] };
    case "startsWith":
      return { kind: "startsWith", field, negated: false, values: [inputs.text] };
    case "endsWith":
      return { kind: "endsWith", field, negated: false, values: [inputs.text] };
    // A relation meta's key scopes the link to that field; the synthetic
    // "any link" meta (empty key) keeps the file-wide form.
    case "linksTo":
      return {
        kind: "linksTo",
        field: field || undefined,
        negated: false,
        values: inputs.values.slice(0, 1),
      };
    case "notLinksTo":
      return {
        kind: "linksTo",
        field: field || undefined,
        negated: true,
        values: inputs.values.slice(0, 1),
      };
    case "hasAny":
      return { kind: "hasTag", negated: false, values: inputs.values };
    case "notHasAny":
      return { kind: "hasTag", negated: true, values: inputs.values };
    default:
      return { kind: "unsupported", negated: false, values: [], raw: "" };
  }
}

export type EditorState = {
  meta: RuleFieldMeta;
  op: string;
  inputs: {
    values: string[];
    text: string;
    number: string;
    date: string;
    amount: string;
    unit: string;
  };
};

const emptyInputs = { values: [], text: "", number: "", date: "", amount: "30", unit: "days" };

/// Derives a row's editable state back from a rule, or `null` when the rule is
/// outside the builder's vocabulary — those rows render read-only and are
/// preserved verbatim on save.
export function editorFromRule(
  rule: SmartFilterRule,
  metas: RuleFieldMeta[],
  t: (descriptor: MessageDescriptor) => string,
): EditorState | null {
  if (rule.kind === "unsupported" || rule.kind === "inFolder") return null;
  const findMeta = (kinds: RuleFieldMeta["kind"][]): RuleFieldMeta | null => {
    const meta = metas.find((meta) => meta.key === rule.field);
    if (meta) return kinds.includes(meta.kind) ? meta : null;
    // A field the current schema doesn't declare (hand-written) still edits as
    // the shape the rule itself implies.
    if (!rule.field) return null;
    return { key: rule.field, label: rule.field, kind: kinds[0] };
  };
  const state = (meta: RuleFieldMeta | null, op: string, inputs: Partial<EditorState["inputs"]>) =>
    meta ? { meta, op, inputs: { ...emptyInputs, ...inputs } } : null;

  switch (rule.kind) {
    case "isEmpty":
      return state(
        findMeta(["text", "enum", "list", "number", "date", "season"]),
        rule.negated ? "hasValue" : "isEmpty",
        {},
      );
    case "hasTag": {
      const meta = metas.find((meta) => meta.kind === "tags");
      return meta
        ? {
            meta,
            op: rule.negated ? "notHasAny" : "hasAny",
            inputs: { ...emptyInputs, values: rule.values ?? [] },
          }
        : null;
    }
    case "linksTo": {
      // A scoped rule edits under its own relation field (so its target picker
      // searches the right type); a field-less one is the file-wide form, which
      // keeps its own synthetic meta so editing the target can't silently
      // narrow it to a field.
      const meta = rule.field
        ? (metas.find((meta) => meta.key === rule.field && meta.kind === "relation") ?? {
            key: rule.field,
            label: rule.field,
            kind: "relation" as const,
          })
        : { key: "", label: t(builderWords.anyLink), kind: "relation" as const };
      return {
        meta,
        op: rule.negated ? "notLinksTo" : "linksTo",
        inputs: { ...emptyInputs, values: rule.values ?? [] },
      };
    }
    case "startsWith":
      return rule.negated
        ? null
        : state(findMeta(["text", "enum"]), "startsWith", { text: rule.values?.[0] ?? "" });
    case "endsWith":
      return rule.negated
        ? null
        : state(findMeta(["text", "enum"]), "endsWith", { text: rule.values?.[0] ?? "" });
    case "contains": {
      const meta = findMeta(["list", "text", "tags", "enum", "season"]);
      if (!meta) return null;
      if (meta.kind === "season") {
        if (rule.negated || rule.mode === "all") return null;
        const values = rule.values ?? [];
        // Whole years round-trip back to "year is"; anything else is a plain
        // season list. See `wholeYearsOf`.
        const years = wholeYearsOf(values, meta.seasonLanguage ?? "zh");
        return years
          ? { meta, op: "yearIs", inputs: { ...emptyInputs, values: years } }
          : { meta, op: "isAnyOf", inputs: { ...emptyInputs, values } };
      }
      if (meta.kind === "list" || meta.kind === "tags") {
        const op = rule.negated
          ? rule.mode === "all"
            ? null
            : "notContains"
          : rule.mode === "all"
            ? "containsAll"
            : "containsAny";
        return op ? { meta, op, inputs: { ...emptyInputs, values: rule.values ?? [] } } : null;
      }
      if (rule.mode === "all" || (rule.values?.length ?? 0) > 1) return null;
      return {
        meta,
        op: rule.negated ? "notContains" : "contains",
        inputs: { ...emptyInputs, text: rule.values?.[0] ?? "" },
      };
    }
    case "compare": {
      if (rule.boolean !== undefined && rule.boolean !== null) {
        const flip = rule.op === "ne";
        const truthy = flip ? !rule.boolean : rule.boolean;
        if (rule.op !== "eq" && rule.op !== "ne") return null;
        return state(findMeta(["bool"]), truthy ? "isTrue" : "isFalse", {});
      }
      if (rule.number !== undefined && rule.number !== null) {
        return state(findMeta(["number", "text"]), rule.op ?? "eq", {
          number: String(rule.number),
        });
      }
      if (rule.relative) {
        const meta = findMeta(["date", "mtime", "number"]);
        const op = rule.relative.future
          ? rule.op === "lte"
            ? "withinNext"
            : null
          : rule.op === "gte"
            ? "inLast"
            : rule.op === "lt"
              ? "olderThan"
              : null;
        return op
          ? state(meta, op, {
              amount: String(rule.relative.amount),
              unit: rule.relative.unit,
            })
          : null;
      }
      if (rule.date) {
        const ops: Record<string, string> = {
          eq: "on",
          lt: "before",
          lte: "onOrBefore",
          gt: "after",
          gte: "onOrAfter",
        };
        const op = ops[rule.op ?? "eq"];
        return op ? state(findMeta(["date", "text"]), op, { date: rule.date }) : null;
      }
      if (rule.op === "eq" || rule.op === "ne") {
        return state(
          findMeta(["enum", "text", "list", "date", "season"]),
          rule.op === "eq" ? "is" : "isNot",
          { text: rule.value ?? "" },
        );
      }
      return null;
    }
    default:
      return null;
  }
}

/// A default rule for a freshly picked field — the least surprising operator
/// per kind, with empty inputs.
export function defaultRuleFor(meta: RuleFieldMeta): SmartFilterRule {
  const inputs = { ...emptyInputs, values: [] as string[] };
  switch (meta.kind) {
    case "enum":
    // The season options run newest-first, so a fresh rule starts on the
    // season a catalog is most likely to be filtered by.
    case "season":
      return ruleFromEditor(meta, "is", { ...inputs, text: meta.options?.[0] ?? "" });
    case "list":
      return ruleFromEditor(meta, "containsAny", inputs);
    case "bool":
      return ruleFromEditor(meta, "isTrue", inputs);
    case "number":
      return ruleFromEditor(meta, "gte", { ...inputs, number: "1" });
    case "date":
    case "mtime":
      return ruleFromEditor(meta, "inLast", inputs);
    case "relation":
      return ruleFromEditor(meta, "linksTo", inputs);
    case "tags":
      return ruleFromEditor(meta, "hasAny", inputs);
    default:
      return ruleFromEditor(meta, "contains", inputs);
  }
}
