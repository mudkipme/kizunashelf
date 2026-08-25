//! The rule builder's field vocabulary: what fields a type offers, what each
//! one can be asked, and the words for both.
//!
//! Everything here flows from the schema — a field is an enum because its
//! `fieldType` says so, never because of what it is called — so this module is
//! the only place that maps schema declarations onto builder behavior.

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

import type { SeasonLanguage } from "@/components/entities/metadata-types";
import {
  normalizeSeasonLanguage,
  seasonValueOptions,
  seasonYearOptions,
} from "@/components/smart-lists/season-values";

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

export const builderWords = {
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

/// The operators offered per field kind, in menu order. Every id maps to one
/// rule shape in `ruleFromEditor` and back in `editorFromRule`.
export const opsByKind: Record<RuleFieldMeta["kind"], string[]> = {
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

export const opWord: Record<string, MessageDescriptor> = {
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
