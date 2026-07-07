import {
  configFields,
  configuredFieldLabel,
  isListFieldType,
  type FieldConfig,
} from "@/lib/type-config";
import { defaultTagsField } from "@/lib/constants";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary, TypeConfig } from "@/types/api";

import { currentOptions, parseDateValue } from "./frontmatter-utils";
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
  language: string,
  allTags: string[] = [],
  tagsFieldName: string = defaultTagsField,
  // Display label for the built-in tags field; callers pass a localized string
  // (this module is not a component, so it cannot resolve translations itself).
  tagsLabel: string = "Tags",
) {
  const specs: EditableFieldSpec[] = [];
  const seen = new Set<string>();

  // The built-in tags field is always editable (a list with autocomplete over the
  // whole tag vocabulary, plus free entry). Claim its key up front so a
  // schema-configured or hand-written field of the same name isn't rendered twice.
  seen.add(tagsFieldName);
  specs.push({
    key: tagsFieldName,
    label: tagsLabel,
    kind: "list",
    options: allTags,
    relationOptions: [],
    loadRelationOptions: undefined,
    seasonLanguage: "zh",
    configured: true,
  });

  for (const field of configFields(typeConfig)) {
    const key = field.field.trim();
    if (!key || seen.has(key) || isVirtualTitleField(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: configuredFieldLabel(field),
      kind: fieldKind(field, frontmatter[key]),
      options: optionsForConfiguredField(field, frontmatter[key]),
      relationOptions:
        field.fieldType === "relation"
          ? relationOptionsForField(field, relationSuggestions, language)
          : [],
      loadRelationOptions:
        field.fieldType === "relation" && onRelationSearch
          ? async (query, signal) =>
              relationOptionsForField(
                field,
                await onRelationSearch({ relationType: field.relationType, query, signal }),
                language,
              )
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
      kind: unknownFieldKind(frontmatter[key]),
      options: [],
      relationOptions: [],
      loadRelationOptions: undefined,
      seasonLanguage: "zh",
      configured: false,
    });
  }

  return specs;
}

// Unknown (schema-less) fields default to a plain text input, but when the raw
// frontmatter value is clearly a non-string scalar/list we route it to the same
// editor the configured fields use so editing round-trips the type. We only
// promote shapes we can represent losslessly: string lists, numbers, booleans,
// and ISO dates. Anything else (mixed/object lists, nested objects) stays text.
function unknownFieldKind(value: FrontmatterValue | undefined): FieldKind {
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return "number";
  if (Array.isArray(value)) {
    return value.every((item) => typeof item === "string") ? "list" : "text";
  }
  if (typeof value === "string" && parseDateValue(value)) return "date";
  return "text";
}

function fieldKind(field: FieldConfig, value: FrontmatterValue | undefined): FieldKind {
  if (field.fieldType === "season") return "season";
  if (field.fieldType === "date") return "date";
  if (field.fieldType === "relation") return "relation";
  // Image kinds get a dedicated preview/upload editor; check before the generic
  // list branch, since `imageList` is also a list field type.
  if (field.fieldType === "image") return "image";
  if (field.fieldType === "imageList") return "imageList";
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

function relationOptionsForField(field: FieldConfig, suggestions: EntitySummary[], language: string) {
  const relationType = normalizeRelationType(field.relationType);
  return suggestions
    .filter((item) => !relationType || entityMatchesRelationType(item, relationType))
    .map((item) => ({
      value: item.basename,
      label: entityTitle(item, language),
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
