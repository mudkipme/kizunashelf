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

const providerExternalRefs: Record<string, string[]> = {
  bangumi: ["bangumi", "bgm"],
  igdb: ["igdb"],
  thetvdb: ["thetvdb", "tvdb"],
};

export function candidateMetadataEntries(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
): ExternalMetadataEntry[] {
  const metadata = (candidate.metadata ?? {}) as Record<string, unknown>;
  const entries: ExternalMetadataEntry[] = [];
  const used = new Set<string>();
  const fields = configFields(typeConfig);

  for (const field of fields) {
    const exactValue = metadata[field.field];
    if (hasValue(exactValue)) {
      addEntry(entries, used, field, normalizeValueForField(field, exactValue));
      continue;
    }

    const semanticValue = semanticCandidateValue(candidate, metadata, field);
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

function semanticCandidateValue(
  candidate: ExternalCandidate,
  metadata: Record<string, unknown>,
  field: FieldConfig,
) {
  if (field.fieldType === "title") {
    return titleValue(candidate, field);
  }

  if (field.fieldType === "image" || field.fieldType === "imageList") {
    return candidate.coverUrl ?? metadata.cover_url;
  }

  if (field.fieldType === "externalRef" && externalRefMatches(candidate, field.externalRef ?? "")) {
    return candidate.url;
  }

  if (field.fieldType === "date" && field.dateRole === "planning") {
    return metadata.release_date;
  }

  if (field.fieldType === "totalProgress") {
    return metadata.episodes;
  }

  return undefined;
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

function titleValue(candidate: ExternalCandidate, field: FieldConfig) {
  if (field.titleRole === "original") return candidate.originalTitle ?? candidate.title;
  const language = field.titleLanguage ?? "";
  if (language && candidate.titles?.[language]) return candidate.titles[language];
  if (language === "zh" && candidate.titles?.zh) return candidate.titles.zh;
  if (language === "ja" && candidate.titles?.ja) return candidate.titles.ja;
  if (!language || language === "default") return candidate.title;
  return undefined;
}

function externalRefMatches(candidate: ExternalCandidate, externalRef: string) {
  const expected = externalRef.trim().toLowerCase();
  if (!expected) return false;
  return (providerExternalRefs[candidate.provider] ?? [candidate.provider]).includes(expected);
}

function hasValue(value: unknown): value is NonNullable<unknown> {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return true;
}
