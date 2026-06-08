import {
  configFields,
  configuredFieldLabel,
  isListFieldType,
  type FieldConfig,
} from "@/lib/type-config";
import type { ExternalCandidate, TypeConfig } from "@/types/api";

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

export const externalFieldOptionsBySource: Record<string, ExternalFieldOption[]> = {
  bangumi: [
    { field: "name", label: "Name" },
    { field: "name_cn", label: "Chinese name" },
    { field: "cover_url", label: "Cover URL" },
    { field: "date", label: "Release date" },
    { field: "total_episodes", label: "Total episodes" },
    { field: "summary", label: "Summary" },
  ],
  igdb: [
    { field: "name", label: "Name" },
    { field: "cover_url", label: "Cover URL" },
    { field: "first_release_date", label: "First release date" },
    { field: "summary", label: "Summary" },
    { field: "storyline", label: "Storyline" },
  ],
  thetvdb: [
    { field: "name", label: "Name" },
    { field: "cover_url", label: "Cover URL" },
    { field: "first_air_time", label: "First air time" },
    { field: "year", label: "Year" },
    { field: "overview", label: "Overview" },
  ],
};

export const externalSourceOptions = [
  { source: "bangumi", label: "Bangumi" },
  { source: "igdb", label: "IGDB" },
  { source: "thetvdb", label: "TheTVDB" },
];

export function externalSourceLabel(source: string) {
  return externalSourceOptions.find((option) => option.source === source)?.label ?? source;
}

export function externalProviderPriority(typeConfig: TypeConfig | undefined) {
  const priority = new Set<string>();
  for (const source of typeConfig?.externalPriority ?? []) {
    addKnownSource(priority, source);
  }
  for (const field of configFields(typeConfig)) {
    if (field.fieldType === "externalRef") addKnownSource(priority, field.externalRef ?? "");
  }
  for (const field of configFields(typeConfig)) {
    for (const mapping of field.externalFields ?? []) addKnownSource(priority, mapping.source);
  }
  for (const option of externalSourceOptions) priority.add(option.source);
  return [...priority];
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

function addKnownSource(target: Set<string>, source: string) {
  const expected = source.trim().toLowerCase();
  if (externalSourceOptions.some((option) => option.source === expected)) {
    target.add(expected);
  }
}

function hasValue(value: unknown): value is NonNullable<unknown> {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return true;
}
