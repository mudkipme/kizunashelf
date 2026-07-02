import type { TypeConfig } from "@/types/api";
import { iso639TitleLanguage } from "@/lib/title-language";

export type FieldConfig = NonNullable<TypeConfig["fields"]>[number];
export type FieldType = FieldConfig["fieldType"];
export type DateRole = NonNullable<FieldConfig["dateRole"]>;

export function configFields(typeConfig?: TypeConfig): FieldConfig[] {
  return typeConfig?.fields ?? [];
}

export function fieldsByType(typeConfig: TypeConfig | undefined, fieldType: FieldType): FieldConfig[] {
  return configFields(typeConfig).filter((field) => field.fieldType === fieldType);
}

export function fieldNamesByType(typeConfig: TypeConfig | undefined, fieldType: FieldType): string[] {
  return fieldsByType(typeConfig, fieldType).map((field) => field.field);
}

export function fieldDisplayLabel(field: FieldConfig) {
  return field.displayName?.trim() || field.field;
}

export function fieldLabelForKey(typeConfig: TypeConfig | undefined, key: string) {
  const field = configFields(typeConfig).find((item) => item.field === key);
  if (field) return fieldDisplayLabel(field);
  // The untyped body-wikilink pseudo-field ("body") reads as "Notes" — matching
  // the detail page's Notes section — unless the schema defines a real field named
  // "body" (handled above). The technical term stays only in the schema editor.
  if (key === "body") return "Notes";
  return key;
}

export function fieldLabelAcrossTypes(typeConfigs: TypeConfig[] | undefined, key: string) {
  const field = (typeConfigs ?? [])
    .flatMap((typeConfig) => configFields(typeConfig))
    .find((item) => item.field === key && item.displayName?.trim());
  return field ? fieldDisplayLabel(field) : key;
}

export function titleFieldLabelForLanguage(typeConfig: TypeConfig | undefined, language: string) {
  const field = fieldsByType(typeConfig, "title").find(
    (item) => iso639TitleLanguage(item.titleLanguage) === language && item.displayName?.trim(),
  );
  return field ? fieldDisplayLabel(field) : undefined;
}

export function titleLabelForKey(typeConfig: TypeConfig | undefined, key: string) {
  const field = configFields(typeConfig).find((item) => item.field === key);
  return titleFieldLabelForLanguage(typeConfig, key) ?? (field ? fieldDisplayLabel(field) : undefined);
}

export function fieldLabelsByType(typeConfigs: TypeConfig[] | undefined) {
  return new Map(
    (typeConfigs ?? []).map((typeConfig) => [
      typeConfig.id,
      new Map(configFields(typeConfig).map((field) => [field.field, fieldDisplayLabel(field)])),
    ]),
  );
}

/// Maps each entity type's id to its configured label, so callers can render the
/// schema label ("Games") for a bare type id ("games") even when no resolved
/// entity of that type is on hand. Mirrors the server's `EntitySummary.typeLabel`.
export function typeLabelsById(typeConfigs: TypeConfig[] | undefined) {
  return new Map((typeConfigs ?? []).map((typeConfig) => [typeConfig.id, typeConfig.label]));
}

/// Whether a type declares a cover — an `image`/`imageList` field. Covers are
/// shown for types that have one (with a placeholder when the entity has no
/// value); types without one show no cover slot at all.
export function typeHasCoverField(typeConfig: TypeConfig | undefined): boolean {
  return configFields(typeConfig).some(
    (field) => field.fieldType === "image" || field.fieldType === "imageList",
  );
}

/// The set of type ids that declare a cover field.
export function coverTypeIds(typeConfigs: TypeConfig[] | undefined): Set<string> {
  return new Set((typeConfigs ?? []).filter(typeHasCoverField).map((typeConfig) => typeConfig.id));
}

export function entityFieldLabel(
  labelsByType: ReadonlyMap<string, ReadonlyMap<string, string>> | undefined,
  type: string,
  field: string,
) {
  return labelsByType?.get(type)?.get(field) ?? field;
}

export function dateRoleFields(typeConfig: TypeConfig | undefined, dateRole: DateRole): FieldConfig[] {
  return configFields(typeConfig).filter(
    (field) => isDateFieldType(field.fieldType) && field.dateRole === dateRole,
  );
}

export function dateFieldNames(typeConfig: TypeConfig | undefined): string[] {
  return configFields(typeConfig)
    .filter((field) => isDateFieldType(field.fieldType) && field.dateRole)
    .map((field) => field.field);
}

export function isListFieldType(fieldType: FieldType) {
  return (
    fieldType === "imageList" ||
    fieldType === "enumList" ||
    fieldType === "season" ||
    fieldType === "relation" ||
    fieldType === "textList"
  );
}

export function isDateFieldType(fieldType: FieldType) {
  return fieldType === "date" || fieldType === "season";
}

export function supportsEnumOptions(fieldType: FieldType) {
  return fieldType === "enum" || fieldType === "enumList";
}

export function supportsTitleOptions(fieldType: FieldType) {
  return fieldType === "title";
}

export function supportsDateRole(fieldType: FieldType) {
  return isDateFieldType(fieldType);
}

export function fieldTypeLabel(fieldType: FieldType) {
  if (fieldType === "id") return "ID";
  if (fieldType === "title") return "Title";
  if (fieldType === "image") return "Image";
  if (fieldType === "imageList") return "Image list";
  if (fieldType === "enum") return "Enum";
  if (fieldType === "enumList") return "Enum list";
  if (fieldType === "progress") return "Progress";
  if (fieldType === "totalProgress") return "Total progress";
  if (fieldType === "rating") return "Rating";
  if (fieldType === "bool") return "Bool";
  if (fieldType === "season") return "Season";
  if (fieldType === "date") return "Date";
  if (fieldType === "externalRef") return "External ref";
  if (fieldType === "relation") return "Relation";
  if (fieldType === "textList") return "Text list";
  return "Text";
}

export function configuredFieldLabel(field: FieldConfig) {
  if (field.displayName?.trim()) return field.displayName.trim();
  if (field.fieldType === "title" && field.titleRole === "original") return `Original title: ${field.field}`;
  if (field.fieldType === "title" && field.titleLanguage) return `${field.titleLanguage} title: ${field.field}`;
  if (field.fieldType === "date") {
    const role =
      field.dateRole === "completed"
        ? "Completed date"
        : field.dateRole === "started"
          ? "Started date"
          : "Planning date";
    return `${role}: ${field.field}`;
  }
  if (field.fieldType === "totalProgress") return `Total progress: ${field.field}`;
  return `${fieldTypeLabel(field.fieldType)}: ${field.field}`;
}
