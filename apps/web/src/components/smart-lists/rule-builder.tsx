import { useEffect, useMemo, useRef, useState } from "react";
import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { CopyPlusIcon, PlusIcon, XIcon } from "lucide-react";

import { isAbortError } from "@/api/client";
import { useRelationSearch } from "@/api/use-relation-search";
import { formatRule } from "@/components/smart-lists/rule-format";
import {
  normalizeSeasonLanguage,
  seasonValueOptions,
  seasonYearOptions,
  seasonsOfYear,
  wholeYearsOf,
  yearsFrom,
} from "@/components/smart-lists/season-values";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { useDebouncedAbortableCallback } from "@/hooks/use-debounce";
import type { MultiValueComboboxOption } from "@/components/ui/multi-value-combobox";
import type { SeasonLanguage } from "@/components/entities/metadata-types";
import { Select } from "@/components/ui/select";
import { entityTitle } from "@/lib/title-language";
import { useTitleLanguage } from "@/lib/language";
import type {
  SmartFilterConjunction,
  SmartFilterGroup,
  SmartFilterRule,
  SmartFilterSubgroup,
} from "@/types/api";

/// The minimal structural shape of a type's schema the builder reads. Both
/// the library page's `TypeConfig` (response-side) and the settings editor's
/// `EntityTypeConfig` (request-side, more optional) satisfy it — orval
/// generates those as distinct types, so the builder stays nominal-free.
export type RuleFieldSource = {
  fields?:
    | {
        field: string;
        fieldType: string;
        displayName?: string | null;
        enumOptions?: string[] | null;
        relationType?: string | null;
        seasonLanguage?: string | null;
      }[]
    | null;
};

/// What the builder knows about one pickable field: how to edit it, never
/// inferred from its name — the kind flows from the schema `fieldType` (plus
/// the two `file.*` built-ins and the vault's tags field).
export type RuleFieldMeta = {
  /** The rule's `field` value: a frontmatter key, `file.name`, or `file.mtime`. */
  key: string;
  label: string;
  kind:
    | "enum"
    | "list"
    | "bool"
    | "number"
    | "date"
    | "season"
    | "relation"
    | "text"
    | "tags"
    | "mtime";
  options?: string[];
  relationType?: string;
  /** `season` kind: the years the "year is" operator offers. */
  yearOptions?: string[];
  /** `season` kind: the language the rule's season text is written in. */
  seasonLanguage?: SeasonLanguage;
  /** `list` kind: whether values outside `options` may be typed in. */
  allowCustomValues?: boolean;
};

/// The field vocabulary for one type's schema: its fields that map onto rule
/// shapes, plus the built-in tags field and the two supported `file.*`
/// properties.
///
/// `typeConfig: undefined` is an unscoped list, and then **only** the built-ins
/// are offered. A frontmatter rule bakes one type's schema into itself — the
/// operator menu, the enum choices, a relation's target type — and nothing
/// makes a field key mean the same thing in two types: `status` can be an enum
/// of watch states in one and free text in another. Merging the types would
/// build rules against a schema no single entity actually has, so a field rule
/// requires a scope. Rules already written against a field are still edited
/// (see `editorFromRule`), just not offered here.
export function ruleFieldMetas(
  typeConfig: RuleFieldSource | undefined,
  // The configured tags key, or `undefined` when the opt-in tags feature is
  // disabled — then no tags field is offered in the builder.
  tagsField: string | undefined,
  allTags: string[],
  t: (descriptor: MessageDescriptor) => string,
): RuleFieldMeta[] {
  const metas = new Map<string, RuleFieldMeta>();
  for (const field of typeConfig?.fields ?? []) {
    if (metas.has(field.field) || field.field === tagsField) continue;
    const base = { key: field.field, label: field.displayName?.trim() || field.field };
    switch (field.fieldType) {
      case "enum":
        metas.set(field.field, { ...base, kind: "enum", options: field.enumOptions ?? [] });
        break;
      case "enumList":
        metas.set(field.field, { ...base, kind: "list", options: field.enumOptions ?? [] });
        break;
      case "textList":
        metas.set(field.field, { ...base, kind: "list", options: [], allowCustomValues: true });
        break;
      case "bool":
        metas.set(field.field, { ...base, kind: "bool" });
        break;
      case "rating":
      case "progress":
      case "totalProgress":
        metas.set(field.field, { ...base, kind: "number" });
        break;
      case "date":
        metas.set(field.field, { ...base, kind: "date" });
        break;
      case "season": {
        // The offered values are written in the field's own season language —
        // see `season-values.ts` for why the rule stores plain text.
        const language = normalizeSeasonLanguage(field.seasonLanguage);
        metas.set(field.field, {
          ...base,
          kind: "season",
          seasonLanguage: language,
          options: seasonValueOptions(language),
          yearOptions: seasonYearOptions(),
        });
        break;
      }
      case "relation":
        metas.set(field.field, {
          ...base,
          kind: "relation",
          relationType: field.relationType ?? undefined,
        });
        break;
      case "text":
        metas.set(field.field, { ...base, kind: "text" });
        break;
      default:
        break;
    }
  }
  // Schema fields first: the first meta seeds a freshly added rule, and an
  // enum equality starts out complete (first option preselected) where a tags
  // rule would sit empty until values are picked.
  return [
    ...metas.values(),
    ...(tagsField
      ? [{ key: tagsField, label: t(builderWords.tags), kind: "tags" as const, options: allTags }]
      : []),
    { key: "file.name", label: t(builderWords.fileName), kind: "text" },
    { key: "file.mtime", label: t(builderWords.updated), kind: "mtime" },
  ];
}

export { isCompleteRule, pruneIncompleteRules } from "@/components/smart-lists/rule-model";

/// The operators offered per field kind, in menu order. Every id maps to one
/// rule shape in `ruleFromEditor` and back in `editorFromRule`.
const opsByKind: Record<RuleFieldMeta["kind"], string[]> = {
  enum: ["is", "isNot", "isEmpty", "hasValue"],
  list: ["containsAny", "containsAll", "notContains", "isEmpty", "hasValue"],
  bool: ["isTrue", "isFalse"],
  number: ["eq", "ne", "gt", "gte", "lt", "lte", "isEmpty", "hasValue"],
  date: [
    "on",
    "before",
    "onOrBefore",
    "after",
    "onOrAfter",
    "inLast",
    "olderThan",
    "withinNext",
    "isEmpty",
    "hasValue",
  ],
  season: ["is", "isAnyOf", "yearIs", "isNot", "isEmpty", "hasValue"],
  relation: ["linksTo", "notLinksTo"],
  text: ["contains", "notContains", "startsWith", "endsWith", "is", "isNot", "isEmpty", "hasValue"],
  tags: ["hasAny", "notHasAny"],
  mtime: ["inLast", "olderThan"],
};

const builderWords = {
  tags: msg({ comment: "Field name for the built-in tags field in the rule builder", message: "Tags" }),
  fileName: msg({
    comment: "Rule-builder field for the note's file name (its title)",
    message: "File name",
  }),
  updated: msg({
    comment: "Rule-builder field for the note file's last-modified time",
    message: "Updated",
  }),
  anyLink: msg({
    comment:
      "Rule-builder field standing for a link from anywhere in the note, rather than from one relation field",
    message: "Any link",
  }),
  is: msg({ comment: "Rule operator: field equals the value", message: "is" }),
  isNot: msg({ comment: "Rule operator: field does not equal the value", message: "is not" }),
  isEmpty: msg({ comment: "Rule operator: field has no value", message: "is empty" }),
  hasValue: msg({ comment: "Rule operator: field has any value", message: "has a value" }),
  containsAny: msg({
    comment: "Rule operator: list field contains any of the chosen values",
    message: "contains any of",
  }),
  containsAll: msg({
    comment: "Rule operator: list field contains all of the chosen values",
    message: "contains all of",
  }),
  notContains: msg({
    comment: "Rule operator: field does not contain the value(s)",
    message: "does not contain",
  }),
  isAnyOf: msg({
    comment: "Rule operator: season field is one of the chosen seasons",
    message: "is any of",
  }),
  yearIs: msg({
    comment: "Rule operator: the season falls in one of the chosen years",
    message: "year is",
  }),
  isTrue: msg({ comment: "Rule operator on a yes/no field", message: "is yes" }),
  isFalse: msg({ comment: "Rule operator on a yes/no field", message: "is no" }),
  eq: msg({ comment: "Numeric rule operator", message: "=" }),
  ne: msg({ comment: "Numeric rule operator", message: "≠" }),
  gt: msg({ comment: "Numeric rule operator", message: ">" }),
  gte: msg({ comment: "Numeric rule operator", message: "≥" }),
  lt: msg({ comment: "Numeric rule operator", message: "<" }),
  lte: msg({ comment: "Numeric rule operator", message: "≤" }),
  on: msg({ comment: "Date rule operator: on this exact day/period", message: "is on" }),
  before: msg({ comment: "Date rule operator", message: "is before" }),
  onOrBefore: msg({ comment: "Date rule operator", message: "is on or before" }),
  after: msg({ comment: "Date rule operator", message: "is after" }),
  onOrAfter: msg({ comment: "Date rule operator", message: "is on or after" }),
  inLast: msg({
    comment: "Date rule operator: within the last N days/weeks/months/years",
    message: "in the last",
  }),
  olderThan: msg({
    comment: "Date rule operator: earlier than N days/weeks/months/years ago",
    message: "older than",
  }),
  withinNext: msg({
    comment: "Date rule operator: no later than N days/weeks/months/years from now",
    message: "within the next",
  }),
  contains: msg({ comment: "Text rule operator: substring match", message: "contains" }),
  startsWith: msg({ comment: "Text rule operator", message: "starts with" }),
  endsWith: msg({ comment: "Text rule operator", message: "ends with" }),
  linksTo: msg({ comment: "Relation rule operator: note links to the entity", message: "links to" }),
  notLinksTo: msg({
    comment: "Relation rule operator: note does not link to the entity",
    message: "does not link to",
  }),
  hasAny: msg({ comment: "Tags rule operator: has any of the chosen tags", message: "has any of" }),
  notHasAny: msg({
    comment: "Tags rule operator: has none of the chosen tags",
    message: "has none of",
  }),
  days: msg({ comment: "Duration unit in a date rule", message: "days" }),
  weeks: msg({ comment: "Duration unit in a date rule", message: "weeks" }),
  months: msg({ comment: "Duration unit in a date rule", message: "months" }),
  years: msg({ comment: "Duration unit in a date rule", message: "years" }),
};

const opWord: Record<string, MessageDescriptor> = {
  is: builderWords.is,
  isNot: builderWords.isNot,
  isEmpty: builderWords.isEmpty,
  hasValue: builderWords.hasValue,
  containsAny: builderWords.containsAny,
  containsAll: builderWords.containsAll,
  notContains: builderWords.notContains,
  isAnyOf: builderWords.isAnyOf,
  yearIs: builderWords.yearIs,
  isTrue: builderWords.isTrue,
  isFalse: builderWords.isFalse,
  eq: builderWords.eq,
  ne: builderWords.ne,
  gt: builderWords.gt,
  gte: builderWords.gte,
  lt: builderWords.lt,
  lte: builderWords.lte,
  on: builderWords.on,
  before: builderWords.before,
  onOrBefore: builderWords.onOrBefore,
  after: builderWords.after,
  onOrAfter: builderWords.onOrAfter,
  inLast: builderWords.inLast,
  olderThan: builderWords.olderThan,
  withinNext: builderWords.withinNext,
  contains: builderWords.contains,
  startsWith: builderWords.startsWith,
  endsWith: builderWords.endsWith,
  linksTo: builderWords.linksTo,
  notLinksTo: builderWords.notLinksTo,
  hasAny: builderWords.hasAny,
  notHasAny: builderWords.notHasAny,
};

/// The single-value and multi-value sides of a season row, each filled from the
/// other when it's empty.
function carriedSeasonInputs(inputs: { values: string[]; text: string }) {
  const text = inputs.text.trim();
  const values = inputs.values.length > 0 ? inputs.values : text ? [text] : [];
  return { values, text: text || values[0] || "" };
}

/// Builds the rule a row's (field, operator, inputs) selection means.
function ruleFromEditor(
  meta: RuleFieldMeta,
  op: string,
  editorInputs: { values: string[]; text: string; number: string; date: string; amount: string; unit: string },
): SmartFilterRule {
  const field = meta.key;
  // A season rule holds one value or several depending on its operator, so the
  // two sides carry into each other: switching "is Spring 2024" to "is any of"
  // keeps that season picked rather than emptying the row.
  const inputs =
    meta.kind === "season" ? { ...editorInputs, ...carriedSeasonInputs(editorInputs) } : editorInputs;
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
      return { kind: "linksTo", field: field || undefined, negated: false, values: inputs.values.slice(0, 1) };
    case "notLinksTo":
      return { kind: "linksTo", field: field || undefined, negated: true, values: inputs.values.slice(0, 1) };
    case "hasAny":
      return { kind: "hasTag", negated: false, values: inputs.values };
    case "notHasAny":
      return { kind: "hasTag", negated: true, values: inputs.values };
    default:
      return { kind: "unsupported", negated: false, values: [], raw: "" };
  }
}

type EditorState = {
  meta: RuleFieldMeta;
  op: string;
  inputs: { values: string[]; text: string; number: string; date: string; amount: string; unit: string };
};

const emptyInputs = { values: [], text: "", number: "", date: "", amount: "30", unit: "days" };

/// Derives a row's editable state back from a rule, or `null` when the rule is
/// outside the builder's vocabulary — those rows render read-only and are
/// preserved verbatim on save.
function editorFromRule(
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
        ? { meta, op: rule.negated ? "notHasAny" : "hasAny", inputs: { ...emptyInputs, values: rule.values ?? [] } }
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
      return { meta, op: rule.negated ? "notLinksTo" : "linksTo", inputs: { ...emptyInputs, values: rule.values ?? [] } };
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
        return state(findMeta(["number", "text"]), rule.op ?? "eq", { number: String(rule.number) });
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
function defaultRuleFor(meta: RuleFieldMeta): SmartFilterRule {
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

export function RuleBuilder({
  fieldMetas,
  value,
  onChange,
  disabled = false,
}: {
  fieldMetas: RuleFieldMeta[];
  value: SmartFilterGroup;
  onChange: (next: SmartFilterGroup) => void;
  disabled?: boolean;
}) {
  const rules = value.rules ?? [];
  const groups = value.groups ?? [];

  const replaceRule = (index: number, rule: SmartFilterRule | null) => {
    const next = rules.flatMap((existing, i) => (i === index ? (rule ? [rule] : []) : [existing]));
    onChange({ ...value, rules: next });
  };
  const replaceGroup = (index: number, group: SmartFilterSubgroup | null) => {
    const next = groups.flatMap((existing, i) => (i === index ? (group ? [group] : []) : [existing]));
    onChange({ ...value, groups: next });
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <ConjunctionSelect
          value={value.conjunction}
          disabled={disabled}
          onChange={(conjunction) => onChange({ ...value, conjunction })}
        />
        <span className="text-xs text-muted-foreground">
          <Trans>of the following match:</Trans>
        </span>
      </div>
      {rules.map((rule, index) => (
        <RuleRow
          key={index}
          rule={rule}
          fieldMetas={fieldMetas}
          disabled={disabled}
          onChange={(next) => replaceRule(index, next)}
        />
      ))}
      {groups.map((group, index) => (
        <SubgroupBox
          key={index}
          group={group}
          fieldMetas={fieldMetas}
          disabled={disabled}
          onChange={(next) => replaceGroup(index, next)}
        />
      ))}
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled || fieldMetas.length === 0}
          onClick={() => onChange({ ...value, rules: [...rules, defaultRuleFor(fieldMetas[0])] })}
        >
          <PlusIcon data-icon="inline-start" />
          <Trans>Add rule</Trans>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled || fieldMetas.length === 0}
          onClick={() =>
            onChange({
              ...value,
              groups: [...groups, { conjunction: "any", rules: [defaultRuleFor(fieldMetas[0])] }],
            })
          }
        >
          <CopyPlusIcon data-icon="inline-start" />
          <Trans>Add group</Trans>
        </Button>
      </div>
    </div>
  );
}

function SubgroupBox({
  group,
  fieldMetas,
  disabled,
  onChange,
}: {
  group: SmartFilterSubgroup;
  fieldMetas: RuleFieldMeta[];
  disabled: boolean;
  onChange: (next: SmartFilterSubgroup | null) => void;
}) {
  const { t } = useLingui();
  const rules = group.rules ?? [];
  const replaceRule = (index: number, rule: SmartFilterRule | null) => {
    const next = rules.flatMap((existing, i) => (i === index ? (rule ? [rule] : []) : [existing]));
    // A group emptied of its last rule disappears rather than lingering.
    if (next.length === 0) onChange(null);
    else onChange({ ...group, rules: next });
  };
  return (
    <div className="flex flex-col gap-2 rounded-md border border-dashed p-2">
      <div className="flex items-center gap-2">
        <ConjunctionSelect
          value={group.conjunction}
          disabled={disabled}
          onChange={(conjunction) => onChange({ ...group, conjunction })}
        />
        <span className="mr-auto text-xs text-muted-foreground">
          <Trans>of the following match:</Trans>
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t`Remove group`}
          onClick={() => onChange(null)}
        >
          <XIcon />
        </Button>
      </div>
      {rules.map((rule, index) => (
        <RuleRow
          key={index}
          rule={rule}
          fieldMetas={fieldMetas}
          disabled={disabled}
          onChange={(next) => replaceRule(index, next)}
        />
      ))}
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="self-start"
        disabled={disabled || fieldMetas.length === 0}
        onClick={() => onChange({ ...group, rules: [...rules, defaultRuleFor(fieldMetas[0])] })}
      >
        <PlusIcon data-icon="inline-start" />
        <Trans>Add rule</Trans>
      </Button>
    </div>
  );
}

function ConjunctionSelect({
  value,
  disabled,
  onChange,
}: {
  value: SmartFilterConjunction;
  disabled: boolean;
  onChange: (value: SmartFilterConjunction) => void;
}) {
  const { t } = useLingui();
  return (
    <Select
      value={value}
      disabled={disabled}
      aria-label={t`Match mode`}
      className="w-fit"
      onChange={(event) => onChange(event.target.value as SmartFilterConjunction)}
    >
      <option value="all">{t`All`}</option>
      <option value="any">{t`Any`}</option>
      <option value="none">{t`None`}</option>
    </Select>
  );
}

function RuleRow({
  rule,
  fieldMetas,
  disabled,
  onChange,
}: {
  rule: SmartFilterRule;
  fieldMetas: RuleFieldMeta[];
  disabled: boolean;
  onChange: (next: SmartFilterRule | null) => void;
}) {
  const { t } = useLingui();
  const editor = useMemo(() => editorFromRule(rule, fieldMetas, t), [rule, fieldMetas, t]);

  if (!editor) {
    // Outside the builder's vocabulary (hand-written syntax) — shown, kept on
    // save, deletable, but not editable here.
    return (
      <div className="flex items-center gap-2 rounded-md border bg-muted/40 px-2 py-1.5">
        <code className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {formatRule(rule, t)}
        </code>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t`Remove rule`}
          onClick={() => onChange(null)}
        >
          <XIcon />
        </Button>
      </div>
    );
  }

  const { meta, op, inputs } = editor;
  const emit = (nextMeta: RuleFieldMeta, nextOp: string, nextInputs: EditorState["inputs"]) =>
    onChange(ruleFromEditor(nextMeta, nextOp, nextInputs));

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select
        value={meta.key}
        disabled={disabled}
        aria-label={t`Field`}
        className="w-36 min-w-0"
        onChange={(event) => {
          const next = fieldMetas.find((candidate) => candidate.key === event.target.value);
          if (next) onChange(defaultRuleFor(next));
        }}
      >
        {/* A field the schema doesn't declare — hand-written, or the synthetic
            "any link" scope — still shows itself. Picking a real field replaces
            the rule, so the switch is deliberately one-way. */}
        {fieldMetas.some((candidate) => candidate.key === meta.key) ? null : (
          <option value={meta.key}>{meta.label}</option>
        )}
        {fieldMetas.map((candidate) => (
          <option key={candidate.key} value={candidate.key}>
            {candidate.label}
          </option>
        ))}
      </Select>
      <Select
        value={op}
        disabled={disabled}
        aria-label={t`Operator`}
        className="w-fit min-w-0"
        onChange={(event) => emit(meta, event.target.value, inputs)}
      >
        {opsByKind[meta.kind].map((candidate) => (
          <option key={candidate} value={candidate}>
            {t(opWord[candidate])}
          </option>
        ))}
      </Select>
      <RuleValueInput
        meta={meta}
        op={op}
        inputs={inputs}
        disabled={disabled}
        onChange={(nextInputs) => emit(meta, op, nextInputs)}
      />
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={disabled}
        aria-label={t`Remove rule`}
        onClick={() => onChange(null)}
      >
        <XIcon />
      </Button>
    </div>
  );
}

function RuleValueInput({
  meta,
  op,
  inputs,
  disabled,
  onChange,
}: {
  meta: RuleFieldMeta;
  op: string;
  inputs: EditorState["inputs"];
  disabled: boolean;
  onChange: (inputs: EditorState["inputs"]) => void;
}) {
  const { t } = useLingui();
  if (op === "isEmpty" || op === "hasValue" || op === "isTrue" || op === "isFalse") return null;

  if (op === "inLast" || op === "olderThan" || op === "withinNext") {
    return (
      <>
        <Input
          type="number"
          min={1}
          value={inputs.amount}
          disabled={disabled}
          aria-label={t`Amount`}
          className="w-20"
          onChange={(event) => onChange({ ...inputs, amount: event.target.value })}
        />
        <Select
          value={inputs.unit}
          disabled={disabled}
          aria-label={t`Unit`}
          className="w-fit min-w-0"
          onChange={(event) => onChange({ ...inputs, unit: event.target.value })}
        >
          <option value="days">{t(builderWords.days)}</option>
          <option value="weeks">{t(builderWords.weeks)}</option>
          <option value="months">{t(builderWords.months)}</option>
          <option value="years">{t(builderWords.years)}</option>
        </Select>
      </>
    );
  }

  if (meta.kind === "date" && ["on", "before", "onOrBefore", "after", "onOrAfter"].includes(op)) {
    return (
      <Input
        type="date"
        value={inputs.date}
        disabled={disabled}
        aria-label={t`Date`}
        className="w-40"
        onChange={(event) => onChange({ ...inputs, date: event.target.value })}
      />
    );
  }

  if (meta.kind === "number") {
    return (
      <Input
        type="number"
        step="any"
        value={inputs.number}
        disabled={disabled}
        aria-label={t`Value`}
        className="w-24"
        onChange={(event) => onChange({ ...inputs, number: event.target.value })}
      />
    );
  }

  if (meta.kind === "season" && (op === "isAnyOf" || op === "yearIs")) {
    const options = (op === "yearIs" ? meta.yearOptions : meta.options) ?? [];
    return (
      <MultiValueCombobox
        values={inputs.values}
        options={options.map((option) => ({ value: option }))}
        placeholder={op === "yearIs" ? t`Add year` : t`Add season`}
        ariaLabel={t`Values`}
        disabled={disabled}
        // A season written some other way — by hand, or in another language —
        // still filters: the engine matches the season a value names.
        allowCustomValue
        className="min-h-8 w-64 px-2 py-1 text-xs"
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  if ((meta.kind === "enum" || meta.kind === "season") && (op === "is" || op === "isNot")) {
    const options = meta.options ?? [];
    return (
      <Select
        value={inputs.text}
        disabled={disabled}
        aria-label={t`Value`}
        className="w-40 min-w-0"
        onChange={(event) => onChange({ ...inputs, text: event.target.value })}
      >
        {options.includes(inputs.text) ? null : <option value={inputs.text}>{inputs.text}</option>}
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (meta.kind === "list" || meta.kind === "tags") {
    return (
      <MultiValueCombobox
        values={inputs.values}
        options={(meta.options ?? []).map((option) => ({ value: option }))}
        placeholder={t`Add value`}
        ariaLabel={t`Values`}
        disabled={disabled}
        allowCustomValue={meta.allowCustomValues || meta.kind === "tags"}
        className="min-h-8 w-64 px-2 py-1 text-xs"
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  if (meta.kind === "relation") {
    return (
      <RelationTargetPicker
        meta={meta}
        values={inputs.values}
        disabled={disabled}
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  return (
    <Input
      value={inputs.text}
      disabled={disabled}
      aria-label={t`Value`}
      className="w-48"
      onChange={(event) => onChange({ ...inputs, text: event.target.value })}
    />
  );
}

/// Single-target entity picker for `links to` rules: an autocomplete over the
/// field's relation type, keeping at most one chip (a rule links to exactly
/// one target; add more rules — or an "any" group — for several).
function RelationTargetPicker({
  meta,
  values,
  disabled,
  onChange,
}: {
  meta: RuleFieldMeta;
  values: string[];
  disabled: boolean;
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const onRelationSearch = useRelationSearch();
  const [inputValue, setInputValue] = useState("");
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<MultiValueComboboxOption[]>([]);
  const [loading, setLoading] = useState(false);
  const labels = useRef(new Map<string, string>());
  for (const option of options) {
    if (option.label) labels.current.set(option.value, option.label);
  }

  const relationType = meta.relationType;
  const { schedule: scheduleRelationSearch, cancel: cancelRelationSearch } =
    useDebouncedAbortableCallback(
      (signal, targetType: string, query: string) => {
        setLoading(true);
        onRelationSearch({ relationType: targetType, query, signal })
          .then((items) => {
            if (signal.aborted) return;
            setOptions(
              items
                .map((item) => ({ value: item.basename, label: entityTitle(item, language) }))
                .filter((option) => option.value),
            );
          })
          .catch((caught) => {
            if (!isAbortError(caught) && !signal.aborted) setOptions([]);
          })
          .finally(() => {
            if (!signal.aborted) setLoading(false);
          });
      },
      200,
    );

  useEffect(() => {
    if (!open || !relationType) {
      cancelRelationSearch();
      return;
    }
    scheduleRelationSearch(relationType, inputValue.trim());
    return cancelRelationSearch;
  }, [
    open,
    inputValue,
    relationType,
    onRelationSearch,
    language,
    scheduleRelationSearch,
    cancelRelationSearch,
  ]);

  return (
    <MultiValueCombobox
      values={values.slice(0, 1)}
      options={options}
      placeholder={t`Entity name`}
      ariaLabel={t`Link target`}
      disabled={disabled}
      allowCustomValue
      inputValue={inputValue}
      onInputValueChange={setInputValue}
      loading={loading}
      formatChipLabel={(value) => labels.current.get(value) ?? value}
      onOpenChange={setOpen}
      onChange={(next) => onChange(next.slice(-1))}
    />
  );
}
