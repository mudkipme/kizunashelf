import {
  configFields,
  configuredFieldLabel,
  isListFieldType,
  type FieldConfig,
} from "@/lib/type-config";
import type { ExternalCandidate, ExternalProviderCatalog, ExternalProviderCatalogItem, TypeConfig } from "@/types/api";

export type ExternalMetadataEntry = {
  field: string;
  label: string;
  value: unknown;
};

export type ExternalMetadataPreviewEntry = ExternalMetadataEntry & {
  source: string;
  externalField?: string;
  hasValue: boolean;
};

export type ExternalFieldOption = {
  field: string;
  label: string;
};

export type ExternalSourceOption = {
  source: string;
  label: string;
};

export function externalSourceOptions(catalog: ExternalProviderCatalog | undefined): ExternalSourceOption[] {
  return (catalog?.providers ?? []).map((provider) => ({
    source: provider.id,
    label: provider.label,
  }));
}

export function externalFieldOptionsForSource(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
): ExternalFieldOption[] {
  return externalProvider(catalog, source)?.fields ?? [];
}

export function externalTypeOptionsForSource(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
) {
  return externalProvider(catalog, source)?.types ?? [];
}

export function externalSourceLabel(catalog: ExternalProviderCatalog | undefined, source: string) {
  return externalProvider(catalog, source)?.label ?? source;
}

export function externalProviderPriority(
  catalog: ExternalProviderCatalog | undefined,
  typeConfig: TypeConfig | undefined,
) {
  const supported = new Set<string>();
  for (const field of configFields(typeConfig)) {
    if (field.fieldType === "externalRef") addKnownSource(catalog, supported, field.externalRef ?? "");
  }

  const priority = new Set<string>();
  for (const source of typeConfig?.externalPriority ?? []) {
    addKnownSupportedSource(priority, supported, source);
  }

  for (const field of configFields(typeConfig)) {
    if (field.fieldType === "externalRef") addKnownSupportedSource(priority, supported, field.externalRef ?? "");
  }

  return [...priority];
}

export function externalTypesForSource(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
) {
  return [...(externalProvider(catalog, source)?.defaultExternalTypes ?? [])];
}

export function defaultExternalPriority(
  catalog: ExternalProviderCatalog | undefined,
  externalRefs: string[],
) {
  return cleanExternalPriority(catalog, externalRefs.map(externalSourceForField)) ?? [];
}

export function defaultExternalMappings(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
  role: string,
) {
  const field = defaultExternalField(catalog, source, role);
  return field ? [{ source, field }] : [];
}

export function candidateMetadataEntries(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
): ExternalMetadataEntry[] {
  return candidateMetadataPreviewEntries(candidate, typeConfig)
    .filter((entry) => entry.hasValue)
    .map(({ field, label, value }) => ({ field, label, value }));
}

export function candidateMetadataPreviewEntries(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
): ExternalMetadataPreviewEntry[] {
  const metadata = (candidate.metadata ?? {}) as Record<string, unknown>;
  const entries: ExternalMetadataPreviewEntry[] = [];
  const used = new Set<string>();
  const fields = configFields(typeConfig);

  for (const field of fields) {
    const mapped = candidateMappedValueForField(candidate, metadata, field);
    if (!mapped) continue;
    addPreviewEntry(entries, used, field, mapped);
  }

  return entries;
}

export function candidateMetadataPatch(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
  fields: Set<string>,
) {
  const entries = candidateMetadataEntries(candidate, typeConfig);
  return Object.fromEntries(
    entries.filter((entry) => fields.has(entry.field)).map((entry) => [entry.field, entry.value]),
  );
}

function addPreviewEntry(
  entries: ExternalMetadataPreviewEntry[],
  used: Set<string>,
  field: FieldConfig,
  mapped: { source: string; externalField?: string; value: unknown },
) {
  if (used.has(field.field)) return;
  used.add(field.field);
  const normalized = normalizeValueForField(field, mapped.value);
  entries.push({
    field: field.field,
    label: configuredFieldLabel(field),
    value: normalized,
    source: mapped.source,
    externalField: mapped.externalField,
    hasValue: hasValue(normalized),
  });
}

function candidateMappedValueForField(
  candidate: ExternalCandidate,
  metadata: Record<string, unknown>,
  field: FieldConfig,
) {
  if (field.fieldType === "externalRef" && externalRefMatches(candidate, field.externalRef ?? "")) {
    return { source: candidate.provider, value: candidate.url };
  }

  const mapping = field.externalFields?.find((item) => externalSourceMatches(candidate.provider, item.source));
  if (!mapping) return undefined;

  return {
    source: mapping.source,
    externalField: mapping.field,
    value: metadata[mapping.field],
  };
}

function normalizeValueForField(field: FieldConfig, value: unknown) {
  if (field.fieldType === "imageList") {
    return Array.isArray(value) ? value : [value];
  }
  if (isListFieldType(field.fieldType) && !Array.isArray(value)) {
    return [value];
  }
  return value;
}

function externalRefMatches(candidate: ExternalCandidate, externalRef: string) {
  const expected = externalRef.trim().toLowerCase();
  if (!expected) return false;
  return externalSourceMatches(candidate.provider, expected);
}

function externalSourceMatches(provider: string, source: string) {
  const expected = source.trim().toLowerCase();
  if (!expected) return false;
  return provider === expected;
}

function addKnownSource(
  catalog: ExternalProviderCatalog | undefined,
  target: Set<string>,
  source: string,
) {
  const expected = source.trim().toLowerCase();
  if (externalProvider(catalog, expected)) {
    target.add(expected);
  }
}

function addKnownSupportedSource(target: Set<string>, supported: Set<string>, source: string) {
  const expected = source.trim().toLowerCase();
  if (supported.has(expected)) target.add(expected);
}

function hasValue(value: unknown): value is NonNullable<unknown> {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return true;
}

function externalProvider(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
): ExternalProviderCatalogItem | undefined {
  const expected = source.trim().toLowerCase();
  return catalog?.providers.find((provider) => provider.id === expected);
}

function externalSourceForField(field: string) {
  return field.replace(/_url$/, "");
}

function defaultExternalField(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
  role: string,
) {
  return externalProvider(catalog, source)?.defaultFieldMappings.find((mapping) =>
    mapping.roles.includes(role),
  )?.field;
}

function cleanExternalPriority(catalog: ExternalProviderCatalog | undefined, values: string[]) {
  const allowed = new Set(externalSourceOptions(catalog).map((option) => option.source));
  const cleaned = values
    .map((value) => value.trim().toLowerCase())
    .filter((value, index, items) => allowed.has(value) && items.indexOf(value) === index);
  return cleaned.length > 0 ? cleaned : undefined;
}
