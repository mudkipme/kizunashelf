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

export type ExternalBodyPreviewEntry = {
  key: string;
  source: string;
  externalField: string;
  heading: string;
  value: unknown;
  markdown: string;
  hasValue: boolean;
};

export type ExternalBodyPatch = {
  key: string;
  source: string;
  field: string;
  heading: string;
  markdown: string;
};

export type ExternalBodySectionState = "replace" | "append";

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
  for (const mapping of typeConfig?.bodyMappings ?? []) {
    addKnownSource(catalog, supported, mapping.source);
  }

  const priority = new Set<string>();
  for (const source of typeConfig?.externalPriority ?? []) {
    addKnownSupportedSource(priority, supported, source);
  }

  for (const field of configFields(typeConfig)) {
    if (field.fieldType === "externalRef") addKnownSupportedSource(priority, supported, field.externalRef ?? "");
  }
  for (const mapping of typeConfig?.bodyMappings ?? []) {
    addKnownSupportedSource(priority, supported, mapping.source);
  }

  return [...priority];
}

export function externalTypesForSource(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
) {
  return [...(externalProvider(catalog, source)?.defaultExternalTypes ?? [])];
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

export function candidateBodyPreviewEntries(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
): ExternalBodyPreviewEntry[] {
  const metadata = (candidate.metadata ?? {}) as Record<string, unknown>;
  return (typeConfig?.bodyMappings ?? [])
    .filter((mapping) => externalSourceMatches(candidate.provider, mapping.source))
    .map((mapping) => {
      const markdown = formatExternalBodyValue(metadata[mapping.field]);
      return {
        key: externalBodyMappingKey(mapping),
        source: mapping.source,
        externalField: mapping.field,
        heading: mapping.heading,
        value: metadata[mapping.field],
        markdown,
        hasValue: hasValue(markdown),
      };
    });
}

export function candidateBodyPatch(
  candidate: ExternalCandidate,
  typeConfig: TypeConfig | undefined,
  selectedBodySections: Set<string>,
): ExternalBodyPatch[] {
  return candidateBodyPreviewEntries(candidate, typeConfig)
    .filter((entry) => entry.hasValue && selectedBodySections.has(entry.key))
    .map((entry) => ({
      key: entry.key,
      source: entry.source,
      field: entry.externalField,
      heading: entry.heading,
      markdown: entry.markdown,
    }));
}

export function applyExternalBodySections(body: string, patches: ExternalBodyPatch[]) {
  return patches.reduce((nextBody, patch) => applyExternalBodySection(nextBody, patch), body);
}

export function externalBodySectionState(body: string, heading: string): ExternalBodySectionState {
  return findMarkdownHeadingSection(body, heading) ? "replace" : "append";
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

function externalBodyMappingKey(mapping: { source: string; field: string; heading: string }) {
  return `${mapping.source}:${mapping.field}:${mapping.heading}`;
}

function formatExternalBodyValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value.trim();
  if (Array.isArray(value)) {
    return value.map(formatExternalBodyValue).filter(Boolean).join("\n\n");
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return `\`\`\`json\n${JSON.stringify(value, null, 2)}\n\`\`\``;
}

function applyExternalBodySection(body: string, patch: ExternalBodyPatch) {
  const section = findMarkdownHeadingSection(body, patch.heading);
  if (!section) {
    const next = body.trimEnd();
    return `${next}${next ? "\n\n" : ""}${renderExternalBodySection(patch)}\n`;
  }

  const before = body.slice(0, section.start);
  const after = body.slice(section.end).replace(/^\n+/, "");
  const headingLine = body.slice(section.start, section.contentStart).trimEnd();
  const separator = after ? "\n\n" : "\n";
  return `${before}${headingLine}\n\n${patch.markdown.trim()}${separator}${after}`;
}

function renderExternalBodySection(patch: ExternalBodyPatch) {
  const heading = sanitizeExternalBodyHeading(patch.heading);
  return [`## ${heading}`, "", patch.markdown.trim()].join("\n");
}

function sanitizeExternalBodyHeading(value: string) {
  return value.replace(/[\r\n#]/g, " ").replace(/\s+/g, " ").trim() || "External Notes";
}

function findMarkdownHeadingSection(body: string, heading: string) {
  const target = normalizeMarkdownHeadingText(heading);
  if (!target) return undefined;

  const headings = markdownHeadings(body);
  const startIndex = headings.findIndex((item) => normalizeMarkdownHeadingText(item.text) === target);
  if (startIndex < 0) return undefined;

  const start = headings[startIndex];
  const next = headings
    .slice(startIndex + 1)
    .find((item) => item.level <= start.level);
  return {
    start: start.start,
    contentStart: start.end,
    end: next?.start ?? body.length,
  };
}

function markdownHeadings(body: string) {
  const headings: Array<{ start: number; end: number; level: number; text: string }> = [];
  let offset = 0;
  let fence: { marker: "`" | "~"; length: number } | undefined;
  for (const line of body.match(/[^\n]*(?:\n|$)/g) ?? []) {
    if (!line) continue;
    const content = line.replace(/\r?\n$/, "");
    const fenceMatch = content.match(/^ {0,3}(`{3,}|~{3,})/);
    if (fenceMatch) {
      const marker = fenceMatch[1][0] as "`" | "~";
      const length = fenceMatch[1].length;
      if (!fence) {
        fence = { marker, length };
      } else if (fence.marker === marker && length >= fence.length) {
        fence = undefined;
      }
    } else if (!fence) {
      const headingMatch = content.match(/^ {0,3}(#{1,6})(?:[ \t]+|$)(.*)$/);
      if (headingMatch) {
        headings.push({
          start: offset,
          end: offset + line.length,
          level: headingMatch[1].length,
          text: headingMatch[2].replace(/[ \t]+#+[ \t]*$/, "").trim(),
        });
      }
    }
    offset += line.length;
  }
  return headings;
}

function normalizeMarkdownHeadingText(value: string) {
  return sanitizeExternalBodyHeading(value).toLowerCase();
}

function externalProvider(
  catalog: ExternalProviderCatalog | undefined,
  source: string,
): ExternalProviderCatalogItem | undefined {
  const expected = source.trim().toLowerCase();
  return catalog?.providers.find((provider) => provider.id === expected);
}

