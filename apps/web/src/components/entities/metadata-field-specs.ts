import {
  configFields,
  configuredFieldLabel,
  isListFieldType,
  type FieldConfig,
} from "@/lib/type-config";
import type { EntitySummary, TypeConfig } from "@/types/api";

import { currentOptions } from "./frontmatter-utils";
import type {
  EditableFieldSpec,
  FieldKind,
  FrontmatterDraft,
  FrontmatterValue,
  RelationSuggestionSearch,
  SeasonLanguage,
} from "./metadata-types";

export function editableFieldSpecs(
  typeConfig: TypeConfig | undefined,
  frontmatter: FrontmatterDraft,
  relationSuggestions: EntitySummary[],
  onRelationSearch: RelationSuggestionSearch | undefined,
) {
  const specs: EditableFieldSpec[] = [];
  const seen = new Set<string>();

  for (const field of configFields(typeConfig)) {
    const key = field.field.trim();
    if (!key || seen.has(key) || isVirtualTitleField(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: configuredFieldLabel(field),
      kind: fieldKind(field, frontmatter[key]),
      options: optionsForConfiguredField(field, frontmatter[key]),
      relationOptions: field.fieldType === "relation" ? relationOptionsForField(field, relationSuggestions) : [],
      loadRelationOptions:
        field.fieldType === "relation" && onRelationSearch
          ? async (query, signal) => relationOptionsForField(field, await onRelationSearch({ relationType: field.relationType, query, signal }))
          : undefined,
      relationType: field.fieldType === "relation" ? field.relationType : undefined,
      seasonLanguage: normalizeSeasonLanguage(field.seasonLanguage),
      configured: true,
    });
  }

  for (const key of Object.keys(frontmatter)) {
    if (seen.has(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: humanizeField(key),
      kind: "text",
      options: [],
      relationOptions: [],
      loadRelationOptions: undefined,
      seasonLanguage: "zh",
      configured: false,
    });
  }

  return specs;
}

function fieldKind(field: FieldConfig, value: FrontmatterValue | undefined): FieldKind {
  if (field.fieldType === "season") return "season";
  if (field.fieldType === "date") return "date";
  if (field.fieldType === "relation") return "relation";
  if (isListFieldType(field.fieldType) || Array.isArray(value)) return "list";
  if (field.fieldType === "enum") return "select";
  if (field.fieldType === "bool") return "boolean";
  if (field.fieldType === "progress") return "progress";
  if (field.fieldType === "totalProgress" || field.fieldType === "rating") {
    return "number";
  }
  return "text";
}

function optionsForConfiguredField(field: FieldConfig, value: FrontmatterValue | undefined) {
  const options = new Set(field.enumOptions ?? []);
  for (const current of currentOptions(value)) {
    options.add(current);
  }
  return [...options];
}

function relationOptionsForField(field: FieldConfig, suggestions: EntitySummary[]) {
  const relationType = normalizeRelationType(field.relationType);
  return suggestions
    .filter((item) => !relationType || entityMatchesRelationType(item, relationType))
    .map((item) => ({
      value: item.basename,
      label: item.title,
    }))
    .filter((item) => item.value);
}

function entityMatchesRelationType(item: EntitySummary, relationType: string) {
  return normalizeRelationType(item.type) === relationType || normalizeRelationType(item.typeLabel) === relationType;
}

function normalizeRelationType(value: string | null | undefined) {
  return (value ?? "").trim().toLowerCase().replace(/[\s_-]+/g, "");
}

function normalizeSeasonLanguage(language: FieldConfig["seasonLanguage"] | undefined): SeasonLanguage {
  return language === "en" || language === "ja" ? language : "zh";
}

function humanizeField(key: string) {
  return key.replace(/[_-]+/g, " ");
}

function isVirtualTitleField(key: string) {
  return ["filename", "basename", "$filename", "$basename"].includes(key);
}
