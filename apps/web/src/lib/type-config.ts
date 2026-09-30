import { i18n, setupI18n, type MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

import { iso639TitleLanguage } from "@/lib/title-language";
import type { TypeConfig } from "@/types/api";

// Translate at call time so changing the app language updates shared labels.
const fallbackI18n = setupI18n({ locale: "en", messages: { en: {} } });
const label = (message: MessageDescriptor) => (i18n.locale ? i18n : fallbackI18n)._(message);
const bodyPseudoFieldLabel = () => label(msg`Notes`);

export type FieldConfig = NonNullable<TypeConfig["fields"]>[number];
export type FieldType = FieldConfig["fieldType"];
export type DateRole = NonNullable<FieldConfig["dateRole"]>;

export function configFields(typeConfig?: TypeConfig): FieldConfig[] {
  return typeConfig?.fields ?? [];
}

export function fieldsByType(
  typeConfig: TypeConfig | undefined,
  fieldType: FieldType,
): FieldConfig[] {
  return configFields(typeConfig).filter((field) => field.fieldType === fieldType);
}

/** Provider ids (trimmed, lowercased) this type wires an `externalRef` field or an
 *  external body section to — mirroring the core's `configured_external_providers`
 *  derivation. Empty means the type has no external source and can't be searched in
 *  Quick Capture. Not validated against the provider catalog here; callers that need
 *  to drop refs to unknown providers intersect this with the known-provider set. */
export function typeExternalRefs(typeConfig: TypeConfig | undefined): string[] {
  const refs: string[] = [];
  for (const field of configFields(typeConfig)) {
    if (field.fieldType !== "externalRef") continue;
    const ref = field.externalRef?.trim().toLowerCase();
    if (ref) refs.push(ref);
  }
  for (const section of typeConfig?.bodySections ?? []) {
    if (section.kind !== "external") continue;
    for (const mapping of section.externalFields ?? []) {
      const source = mapping.source?.trim().toLowerCase();
      if (source) refs.push(source);
    }
  }
  return refs;
}

/** Whether at least one external source on this type resolves to a provider in
 *  the core catalog. Quick Capture and type-scoped Add links share this rule. */
export function typeSupportsQuickCapture(
  typeConfig: TypeConfig | undefined,
  providerIds: ReadonlySet<string>,
): boolean {
  const known = new Set([...providerIds].map((id) => id.trim().toLowerCase()));
  return typeExternalRefs(typeConfig).some((ref) => known.has(ref));
}

export function fieldNamesByType(
  typeConfig: TypeConfig | undefined,
  fieldType: FieldType,
): string[] {
  return fieldsByType(typeConfig, fieldType).map((field) => field.field);
}

export function fieldDisplayLabel(field: FieldConfig) {
  return field.displayName?.trim() || field.field;
}

export function fieldLabelForKey(typeConfig: TypeConfig | undefined, key: string) {
  const field = configFields(typeConfig).find((item) => item.field === key);
  if (field) return fieldDisplayLabel(field);
  // Unless the schema defines a real field named "body" (handled above), the
  // technical term stays only in the schema editor; here it reads as "Notes".
  if (key === "body") return bodyPseudoFieldLabel();
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
  return (
    titleFieldLabelForLanguage(typeConfig, key) ?? (field ? fieldDisplayLabel(field) : undefined)
  );
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

/// Maps each entity type's id to its configured emoji icon (when set), so a bare
/// type id can render the type's glyph — e.g. the shared cover placeholder.
/// Mirrors `typeLabelsById`; whitespace-only icons are treated as unset.
export function typeIconsById(typeConfigs: TypeConfig[] | undefined) {
  const icons = new Map<string, string>();
  for (const typeConfig of typeConfigs ?? []) {
    const icon = typeConfig.icon?.trim();
    if (icon) icons.set(typeConfig.id, icon);
  }
  return icons;
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
  const configured = labelsByType?.get(type)?.get(field);
  if (configured) return configured;
  // Matches the outgoing side (`fieldLabelForKey`) and the detail page's Notes
  // section, unless the type declares a real field named "body" (a hit above).
  if (field === "body") return bodyPseudoFieldLabel();
  return field;
}

export function dateRoleFields(
  typeConfig: TypeConfig | undefined,
  dateRole: DateRole,
): FieldConfig[] {
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

export function fieldTypeLabel(fieldType: FieldType) {
  if (fieldType === "id") return label(msg`ID`);
  if (fieldType === "title") return label(msg`Title`);
  if (fieldType === "image") return label(msg`Image`);
  if (fieldType === "imageList") return label(msg`Image list`);
  if (fieldType === "enum") return label(msg`Enum`);
  if (fieldType === "enumList") return label(msg`Enum list`);
  if (fieldType === "number") return label(msg`Number`);
  if (fieldType === "rating") return label(msg`Rating`);
  if (fieldType === "bool") return label(msg`Yes/No`);
  if (fieldType === "season") return label(msg`Season`);
  if (fieldType === "date") return label(msg`Date`);
  if (fieldType === "externalRef") return label(msg`External reference`);
  if (fieldType === "relation") return label(msg`Relation`);
  if (fieldType === "textList") return label(msg`Text list`);
  return label(msg`Text`);
}

export function configuredFieldLabel(field: FieldConfig) {
  if (field.displayName?.trim()) return field.displayName.trim();
  if (field.fieldType === "title" && field.titleRole === "original")
    return label(msg`Original title: ${field.field}`);
  if (field.fieldType === "title" && field.titleLanguage)
    return label(msg`${field.titleLanguage} title: ${field.field}`);
  if (field.fieldType === "date") {
    const role = dateRoleLabel(field.dateRole);
    return `${role}: ${field.field}`;
  }
  return `${fieldTypeLabel(field.fieldType)}: ${field.field}`;
}

export function dateRoleLabel(role: FieldConfig["dateRole"]) {
  switch (role) {
    case "planning":
      return label(msg`Planning date`);
    case "started":
      return label(msg`Started date`);
    case "completed":
      return label(msg`Completed date`);
    case "event":
      return label(msg`Event date`);
    default:
      return label(msg`Date`);
  }
}
