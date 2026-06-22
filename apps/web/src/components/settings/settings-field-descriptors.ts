import { fieldTypeLabel } from "@/lib/type-config";
import type { FieldConfig, FieldType } from "@/types/api";

export type FieldOptionKey =
  | "titleOptions"
  | "enumOptions"
  | "externalMappings"
  | "progressTotal"
  | "dateRole"
  | "seasonLanguage"
  | "externalRef"
  | "relationType";

type FieldTypeDescriptor = {
  type: FieldType;
  options: FieldOptionKey[];
};

const commonMappedFieldOptions: FieldOptionKey[] = ["externalMappings"];

const fieldTypeDescriptors: FieldTypeDescriptor[] = [
  { type: "id", options: commonMappedFieldOptions },
  { type: "title", options: ["titleOptions", ...commonMappedFieldOptions] },
  { type: "image", options: commonMappedFieldOptions },
  { type: "imageList", options: commonMappedFieldOptions },
  { type: "enum", options: ["enumOptions", ...commonMappedFieldOptions] },
  { type: "enumList", options: ["enumOptions", ...commonMappedFieldOptions] },
  { type: "progress", options: ["progressTotal", ...commonMappedFieldOptions] },
  { type: "totalProgress", options: commonMappedFieldOptions },
  { type: "rating", options: commonMappedFieldOptions },
  { type: "bool", options: commonMappedFieldOptions },
  { type: "season", options: ["dateRole", "seasonLanguage", ...commonMappedFieldOptions] },
  { type: "date", options: ["dateRole", ...commonMappedFieldOptions] },
  { type: "externalRef", options: ["externalRef"] },
  { type: "relation", options: ["relationType", ...commonMappedFieldOptions] },
  { type: "text", options: commonMappedFieldOptions },
  { type: "textList", options: commonMappedFieldOptions },
];

const fieldTypeDescriptorMap = new Map(
  fieldTypeDescriptors.map((descriptor) => [descriptor.type, descriptor]),
);

export const fieldTypeOptions = fieldTypeDescriptors.map((descriptor) => descriptor.type);

export function fieldOptionKeys(fieldType: FieldType): FieldOptionKey[] {
  return fieldTypeDescriptorMap.get(fieldType)?.options ?? commonMappedFieldOptions;
}

export function fieldConfigSummary(field: FieldConfig): string[] {
  const options = new Set(fieldOptionKeys(field.fieldType));
  const summary = [fieldTypeLabel(field.fieldType)];

  if (options.has("titleOptions") && field.titleLanguage) summary.push(`lang ${field.titleLanguage}`);
  if (options.has("titleOptions") && field.titleRole) summary.push(field.titleRole);
  if (options.has("enumOptions") && field.enumOptions?.length) summary.push(`${field.enumOptions.length} values`);
  if (options.has("externalMappings") && field.externalFields?.length) {
    summary.push(`${field.externalFields.length} mappings`);
  }
  if (options.has("progressTotal") && field.totalProgressField) {
    summary.push(`total ${field.totalProgressField}`);
  }
  if (options.has("dateRole") && field.dateRole) summary.push(field.dateRole);
  if (options.has("seasonLanguage") && field.seasonLanguage) {
    summary.push(`season ${field.seasonLanguage}`);
  }
  if (options.has("externalRef") && field.externalRef) summary.push(field.externalRef);
  if (options.has("externalRef") && field.externalTypes?.length) {
    summary.push(`${field.externalTypes.length} external types`);
  }
  if (options.has("relationType") && field.relationType) summary.push(field.relationType);

  return summary;
}
