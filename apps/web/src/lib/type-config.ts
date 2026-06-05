import type { TypeConfig } from "@/types/api";

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
  if (typeConfig?.filename?.titleLanguage) languages.add(typeConfig.filename.titleLanguage);
  for (const field of fieldsByType(typeConfig, "title")) {
    if (field.titleLanguage) languages.add(field.titleLanguage);
  }
  return [...languages];
}

export function defaultTitleOption(typeConfig: TypeConfig | undefined): string | undefined {
  if (typeConfig?.filename?.defaultTitle) return typeConfig.filename.titleLanguage;
  const explicit = fieldsByType(typeConfig, "title").find((field) => field.defaultTitle);
  if (explicit?.titleLanguage) return explicit.titleLanguage;
  return typeConfig?.filename?.titleLanguage ?? fieldsByType(typeConfig, "title")[0]?.titleLanguage ?? undefined;
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
  if (fieldType === "season") return "Season";
  if (fieldType === "date") return "Date";
  if (fieldType === "externalRef") return "External ref";
  if (fieldType === "relation") return "Relation";
  if (fieldType === "textList") return "Text list";
  return "Text";
}

export function configuredFieldLabel(field: FieldConfig) {
  if (field.displayName) return field.displayName;
  if (field.fieldType === "title" && field.titleLanguage) return `${field.titleLanguage} title: ${field.field}`;
  if (field.fieldType === "date") {
    const role = field.dateRole === "completed" ? "Completed date" : "Planning date";
    return `${role}: ${field.field}`;
  }
  if (field.fieldType === "totalProgress") return `Total progress: ${field.field}`;
  return `${fieldTypeLabel(field.fieldType)}: ${field.field}`;
}
