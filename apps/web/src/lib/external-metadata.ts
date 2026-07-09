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
// each match as `fields`/`bodySections`. This module only adds the human-facing
// label (presentation) and merges a chosen body section into live editor text.

export type ExternalMetadataPreviewEntry = MappedFieldValue & { label: string };

export type ExternalBodyPreviewEntry = MappedBodySection;

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

/// The mapped externalRef field carries the candidate URL rather than a provider
/// metadata field, so its `externalField` is absent (see MappedFieldValue). It's
/// the anchor future refreshes match against, so matching always applies it.
export function isExternalRefField(entry: MappedFieldValue): boolean {
  return entry.externalField == null;
}

/// How a candidate value should default in the selection UI. `locked` fixes the
/// checkbox: a value equal to the current one — or an absent one — must not apply
/// (off); the external ref must apply (on). Equality is checked first so that
/// re-matching the same candidate leaves the already-set ref locked off, not on.
export type SelectionState = { checked: boolean; locked: boolean };

export function fieldSelectionState(entry: MappedFieldValue, currentValue: unknown): SelectionState {
  if (!entry.hasValue) return { checked: false, locked: true };
  if (valuesEqual(entry.value, currentValue)) return { checked: false, locked: true };
  if (isExternalRefField(entry)) return { checked: true, locked: true };
  // Fill blanks by default; leave populated fields for the user to opt into.
  return { checked: isEmptyValue(currentValue), locked: false };
}

/// Fields checked by default when a candidate is chosen.
export function matchDefaultFields(
  match: ExternalMatch,
  currentValues?: Record<string, unknown>,
): string[] {
  return (match.fields ?? [])
    .filter((entry) => fieldSelectionState(entry, currentValues?.[entry.field]).checked)
    .map((entry) => entry.field);
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

/// Body-section counterpart to `fieldSelectionState`. A section absent from the
/// body defaults on; one whose current content already matches is locked off; a
/// differing existing section stays off but editable.
export function bodySectionSelectionState(
  entry: MappedBodySection,
  body: string | undefined,
): SelectionState {
  if (!entry.hasValue) return { checked: false, locked: true };
  if (body === undefined) return { checked: true, locked: false };
  const current = currentBodySectionMarkdown(body, entry.heading);
  if (current === undefined) return { checked: true, locked: false };
  if (current === entry.markdown.trim()) return { checked: false, locked: true };
  return { checked: false, locked: false };
}

/// Body sections checked by default when a candidate is chosen.
export function matchDefaultBodySections(match: ExternalMatch, body?: string): string[] {
  return (match.bodySections ?? [])
    .filter((entry) => bodySectionSelectionState(entry, body).checked)
    .map((entry) => entry.key);
}

export function matchBodyPatch(
  match: ExternalMatch,
  selectedBodySections: Set<string>,
): ExternalBodyPatch[] {
  return (match.bodySections ?? [])
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

/// The trimmed Markdown currently under `heading`, or undefined if the section is
/// absent. Lets the UI detect when applying a candidate section would rewrite
/// identical content.
export function currentBodySectionMarkdown(body: string, heading: string): string | undefined {
  const section = findMarkdownHeadingSection(body, heading);
  if (!section) return undefined;
  return body.slice(section.contentStart, section.end).trim();
}

function isEmptyValue(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  if (typeof value === "string") return value.trim() === "";
  if (Array.isArray(value)) return value.length === 0;
  return false;
}

/// Structural equality of a candidate value against the entity's current value;
/// both empty counts as equal. Anything not provably equal falls through as
/// "differs", so at worst a same-valued field stays editable rather than locked.
function valuesEqual(a: unknown, b: unknown): boolean {
  if (isEmptyValue(a) && isEmptyValue(b)) return true;
  return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
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
