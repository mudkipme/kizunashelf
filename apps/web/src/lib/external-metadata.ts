import { configFields, configuredFieldLabel } from "@/lib/type-config";
import type {
  ExternalMatch,
  ExternalProviderCatalog,
  ExternalProviderCatalogItem,
  MappedBodySection,
  MappedFieldValue,
  TypeConfig,
} from "@/types/api";

// The schema-driven mapping (which provider field fills which entity field, list
// vs scalar, externalRef→url, date→season) lives in the Rust core and arrives on
// each match as `fields`/`bodySections`; the selection policy and the apply
// itself are also core-side (`reviewExternalCandidate`/`applyExternalCandidate`).
// This module only adds the human-facing label (presentation) and provider
// catalog lookups.

export type ExternalMetadataPreviewEntry = MappedFieldValue & { label: string };

export type ExternalBodyPreviewEntry = MappedBodySection;

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
  for (const source of externalSectionSources(typeConfig)) {
    addKnownSource(catalog, supported, source);
  }

  const priority = new Set<string>();
  for (const source of typeConfig?.externalPriority ?? []) {
    addKnownSupportedSource(priority, supported, source);
  }

  for (const field of configFields(typeConfig)) {
    if (field.fieldType === "externalRef") addKnownSupportedSource(priority, supported, field.externalRef ?? "");
  }
  for (const source of externalSectionSources(typeConfig)) {
    addKnownSupportedSource(priority, supported, source);
  }

  return [...priority];
}

export function externalTypesForSource(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
) {
  return [...(externalProvider(catalog, source)?.defaultExternalTypes ?? [])];
}

/// The core-mapped field values, decorated with the schema's display label so
/// the preview can show "Original title", "Completed date", etc.
export function matchFieldPreviewEntries(
  match: ExternalMatch,
  typeConfig: TypeConfig | undefined,
): ExternalMetadataPreviewEntry[] {
  const labels = fieldLabels(typeConfig);
  return (match.fields ?? []).map((entry) => ({
    ...entry,
    label: labels.get(entry.field) ?? entry.field,
  }));
}

export function matchFieldPatch(match: ExternalMatch, fields: Set<string>): Record<string, unknown> {
  return Object.fromEntries(
    (match.fields ?? [])
      .filter((entry) => entry.hasValue && fields.has(entry.field))
      .map((entry) => [entry.field, entry.value]),
  );
}

export function matchBodyPreviewEntries(match: ExternalMatch): ExternalBodyPreviewEntry[] {
  return match.bodySections ?? [];
}

function fieldLabels(typeConfig: TypeConfig | undefined): Map<string, string> {
  return new Map(configFields(typeConfig).map((field) => [field.field, configuredFieldLabel(field)]));
}

/// Every external-source id referenced by a type's external body sections.
function externalSectionSources(typeConfig: TypeConfig | undefined): string[] {
  const sources: string[] = [];
  for (const section of typeConfig?.bodySections ?? []) {
    if (section.kind !== "external") continue;
    for (const externalField of section.externalFields ?? []) sources.push(externalField.source);
  }
  return sources;
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

function externalProvider(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
): ExternalProviderCatalogItem | undefined {
  const expected = source.trim().toLowerCase();
  return catalog?.providers.find((provider) => provider.id === expected);
}
