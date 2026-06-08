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

export function hasAnyFieldType(typeConfig: TypeConfig | undefined, fieldTypes: FieldType[]) {
  return configFields(typeConfig).some((field) => fieldTypes.includes(field.fieldType));
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

export function titleLanguageOptions(typeConfig: TypeConfig | undefined): string[] {
  const languages = new Set<string>();
  const filenameLanguage = iso639TitleLanguage(typeConfig?.filename?.titleLanguage);
  if (filenameLanguage) languages.add(filenameLanguage);
  for (const field of fieldsByType(typeConfig, "title")) {
    const fieldLanguage = iso639TitleLanguage(field.titleLanguage);
    if (fieldLanguage) languages.add(fieldLanguage);
  }
  return [...languages];
}

export function defaultTitleOption(typeConfig: TypeConfig | undefined): string | undefined {
  const filenameLanguage = iso639TitleLanguage(typeConfig?.filename?.titleLanguage);
  if (typeConfig?.filename?.defaultTitle && filenameLanguage) return filenameLanguage;
  const explicit = fieldsByType(typeConfig, "title").find((field) => field.defaultTitle);
  const explicitLanguage = iso639TitleLanguage(explicit?.titleLanguage);
  if (explicitLanguage) return explicitLanguage;
  if (filenameLanguage) return filenameLanguage;
  return fieldsByType(typeConfig, "title")
    .map((field) => iso639TitleLanguage(field.titleLanguage))
    .find(Boolean);
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
  if (field.displayName) return field.displayName;
  if (field.fieldType === "title" && field.titleRole === "original") return `Original title: ${field.field}`;
  if (field.fieldType === "title" && field.titleLanguage) return `${field.titleLanguage} title: ${field.field}`;
  if (field.fieldType === "date") {
    const role = field.dateRole === "completed" ? "Completed date" : "Planning date";
    return `${role}: ${field.field}`;
  }
  if (field.fieldType === "totalProgress") return `Total progress: ${field.field}`;
  return `${fieldTypeLabel(field.fieldType)}: ${field.field}`;
}
