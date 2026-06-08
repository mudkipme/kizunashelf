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

export function candidateMetadataEntries(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
): ExternalMetadataEntry[] {
  const metadata = (candidate.metadata ?? {}) as Record<string, unknown>;
  const entries: ExternalMetadataEntry[] = [];
  const used = new Set<string>();
  const fields = configFields(typeConfig);

  for (const field of fields) {
    const semanticValue = candidateValueForField(candidate, metadata, field);
    if (hasValue(semanticValue)) {
      addEntry(entries, used, field, normalizeValueForField(field, semanticValue));
    }
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

function addEntry(
  entries: ExternalMetadataEntry[],
  used: Set<string>,
  field: FieldConfig,
  value: unknown,
) {
  if (used.has(field.field) || !hasValue(value)) return;
  used.add(field.field);
  entries.push({
    field: field.field,
    label: configuredFieldLabel(field),
    value,
  });
}

function candidateValueForField(
  candidate: ExternalCandidate,
  metadata: Record<string, unknown>,
  field: FieldConfig,
) {
  if (field.fieldType === "externalRef" && externalRefMatches(candidate, field.externalRef ?? "")) {
    return candidate.url;
  }

  const mapping = field.externalFields?.find((item) => externalSourceMatches(candidate.provider, item.source));
  if (!mapping) return undefined;

  return metadata[mapping.field];
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

function hasValue(value: unknown): value is NonNullable<unknown> {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return true;
}
