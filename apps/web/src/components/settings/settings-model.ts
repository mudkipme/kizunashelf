import {
  externalFieldOptionsForSource,
  externalTypeOptionsForSource,
  externalSourceOptions,
} from "@/lib/external-metadata";
import { isIso639TitleLanguage } from "@/lib/title-language";
import type {
  BodySection,
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalFieldMapping,
  ExternalProviderCatalog,
  FieldConfig,
  FilenameConfig,
  HomeConfig,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  SaveSettingsRequest,
  VaultConfig,
} from "@/types/api";

export function normalizeVaultConfig(vault?: VaultConfig): VaultConfig {
  const base = defaultVaultConfig();
  return {
    // When the vault config is missing entirely (e.g. a fresh vault), seed the
    // defaults so the editor has something to fill in; when it exists, respect
    // its values including disabled (null) daily notes / home.
    taxonomyRoot: vault?.taxonomyRoot ?? base.taxonomyRoot,
    assetRoot: vault?.assetRoot ?? base.assetRoot,
    dailyNotes: vault
      ? vault.dailyNotes
        ? normalizeDailyNotes(vault.dailyNotes)
        : null
      : base.dailyNotes,
    home: vault ? (vault.home ? normalizeHome(vault.home) : null) : base.home,
    // The tags block has no editor UI; carry it through verbatim so the schema
    // editor never drops a hand-set `tags.field` (preserve unknown values).
    tags: vault?.tags,
    types: vault ? (vault.types ?? []).map(normalizeEntityType) : base.types,
  };
}

function normalizeDailyNotes(config: DailyNotesConfig): DailyNotesConfig {
  return {
    paths: config.paths ?? [],
    dateFormat: config.dateFormat ?? "",
  };
}

function normalizeHome(config: HomeConfig): HomeConfig {
  return {
    title: config.title ?? "",
    sections: config.sections ?? [],
  };
}

function normalizeEntityType(config: EntityTypeConfig): EntityTypeConfig {
  return {
    id: config.id ?? "",
    label: config.label ?? "",
    icon: config.icon ?? "",
    path: config.path ?? "",
    externalPriority: config.externalPriority ?? [],
    filename: config.filename
      ? {
          titleLanguage: config.filename.titleLanguage ?? "",
          titleRole: config.filename.titleRole ?? null,
        }
      : null,
    bodySections: config.bodySections ?? [],
    fields: (config.fields ?? []).map(normalizeField),
  };
}

/** One editable row of the external-body-sections editor (heading + one source field). */
export type ExternalBodyRow = { source: string; field: string; heading: string };

/** Expands a type's external body sections into per-source editor rows. */
export function externalBodyRows(sections: BodySection[]): ExternalBodyRow[] {
  const rows: ExternalBodyRow[] = [];
  for (const section of sections) {
    if (section.kind !== "external") continue;
    for (const externalField of section.externalFields ?? []) {
      rows.push({ source: externalField.source, field: externalField.field, heading: section.heading });
    }
  }
  return rows;
}

/** Folds editor rows back into body sections (grouping by heading so one heading
 * can carry multiple sources), preserving any non-external sections (episodes). */
export function bodySectionsFromRows(rows: ExternalBodyRow[], existing: BodySection[]): BodySection[] {
  const byHeading = new Map<string, ExternalBodyRow[]>();
  for (const row of rows) {
    const group = byHeading.get(row.heading) ?? [];
    group.push(row);
    byHeading.set(row.heading, group);
  }
  const externalSections: BodySection[] = [...byHeading.entries()].map(([heading, group]) => ({
    heading,
    kind: "external" as const,
    externalFields: group.map((row) => ({ source: row.source, field: row.field })),
  }));
  const preserved = existing.filter((section) => section.kind !== "external");
  return [...externalSections, ...preserved];
}

function normalizeField(field: FieldConfig): FieldConfig {
  return {
    field: field.field ?? "",
    fieldType: field.fieldType ?? "text",
    displayName: field.displayName ?? "",
    titleLanguage: field.titleLanguage ?? "",
    titleRole: field.titleRole ?? null,
    externalFields: field.externalFields ?? [],
    enumOptions: field.enumOptions ?? [],
    totalProgressField: field.totalProgressField ?? "",
    dateRole: field.dateRole ?? null,
    seasonLanguage: field.seasonLanguage ?? "zh",
    externalRef: field.externalRef ?? "",
    externalTypes: field.externalTypes ?? [],
    relationType: field.relationType ?? "",
  };
}

// The schema editor saves the vault config only; the app config (vault root +
// write mode) is owned server-side per runtime and is never sent.
export function cleanVaultConfig(
  config: VaultConfig,
  providerCatalog?: ExternalProviderCatalog,
): SaveSettingsRequest {
  return {
    vault: {
      taxonomyRoot: config.taxonomyRoot,
      assetRoot: emptyToUndefined(config.assetRoot),
      dailyNotes: config.dailyNotes
        ? {
            paths: cleanStrings(config.dailyNotes.paths ?? []),
            dateFormat: emptyToUndefined(config.dailyNotes.dateFormat),
          }
        : undefined,
      home: config.home
        ? {
            title: emptyToUndefined(config.home.title),
            sections: (config.home.sections ?? []).map((section) => ({
              id: section.id,
              title: section.title,
              type: section.type,
              filters: cleanHomeSectionFilters(section.filters ?? []),
              limit: section.limit ?? undefined,
              sort: emptyToUndefined(section.sort),
              direction: section.direction ?? undefined,
            })),
          }
        : undefined,
      // No tags UI, but round-trip the block so saving the schema never drops it.
      tags: config.tags,
      types: config.types.map((typeConfig) => ({
        id: typeConfig.id,
        label: typeConfig.label,
        icon: emptyToUndefined(typeConfig.icon),
        path: typeConfig.path,
        externalPriority: cleanExternalPriority(providerCatalog, typeConfig.externalPriority ?? []),
        filename: cleanFilename(typeConfig.filename),
        bodySections: cleanBodySections(typeConfig.bodySections ?? [], providerCatalog),
        fields: typeConfig.fields
          .map((field) => cleanField(field, providerCatalog))
          .filter((field): field is FieldConfig => Boolean(field)),
      })),
    },
  };
}

function cleanHomeSectionFilters(filters: HomeSectionFilterConfig[]) {
  const cleaned = filters
    .map((filter) => ({
      field: filter.field.trim(),
      values: cleanStrings(filter.values ?? []),
    }))
    .filter((filter) => filter.field);
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanFilename(filename: FilenameConfig | null | undefined): FilenameConfig | undefined {
  if (!filename) return undefined;
  const titleLanguage = isIso639TitleLanguage(filename.titleLanguage)
    ? filename.titleLanguage
    : undefined;
  const titleRole = filename.titleRole || undefined;
  if (!titleLanguage && !titleRole) return undefined;
  return {
    titleLanguage,
    titleRole,
  };
}

function cleanExternalFieldMappings(values: ExternalFieldMapping[], providerCatalog?: ExternalProviderCatalog) {
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
    }))
    .filter((value) => value.source && value.field)
    // When the provider catalog is unavailable (query failed/loading), keep the
    // mappings as-is instead of validating against an empty option list and
    // silently dropping provider config the user never touched.
    .filter(
      (value) =>
        providerCatalog === undefined ||
        externalFieldOptionsForSource(providerCatalog, value.source).some(
          (option) => option.field === value.field,
        ),
    );
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanBodySections(
  sections: BodySection[],
  providerCatalog?: ExternalProviderCatalog,
): BodySection[] | undefined {
  const cleaned: BodySection[] = [];
  for (const section of sections) {
    const heading = section.heading.trim();
    if (!heading) continue;
    if (section.kind === "episodes") {
      // No structured editor yet — carry episodes sections through verbatim.
      cleaned.push({
        heading,
        kind: "episodes",
        itemNoun: emptyToUndefined(section.itemNoun),
        tracking: section.tracking ?? undefined,
      });
      continue;
    }
    const seen = new Set<string>();
    const externalFields = (section.externalFields ?? [])
      .map((value) => ({ source: value.source.trim(), field: value.field.trim() }))
      .filter((value) => {
        if (!value.field) return false;
        // Skip catalog validation when the catalog is unavailable (see above).
        if (
          providerCatalog !== undefined &&
          !externalFieldOptionsForSource(providerCatalog, value.source).some(
            (option) => option.field === value.field,
          )
        ) {
          return false;
        }
        const key = `${value.source}:${value.field}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
    if (externalFields.length === 0) continue;
    cleaned.push({ heading, kind: "external", externalFields });
  }
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalTypes(providerCatalog: ExternalProviderCatalog | undefined, source: string | null | undefined, values: string[]) {
  const cleaned = values.map((value) => value.trim()).filter(Boolean);
  // Without a catalog, keep the (deduped) values rather than dropping them all.
  if (providerCatalog === undefined) {
    const deduped = Array.from(new Set(cleaned));
    return deduped.length > 0 ? deduped : undefined;
  }
  const options = externalTypeOptionsForSource(providerCatalog, source ?? "");
  const allowed = new Set(options.map((option) => option.value));
  const filtered = cleaned.filter((value) => allowed.has(value));
  return filtered.length > 0 ? filtered : undefined;
}

function cleanField(field: FieldConfig, providerCatalog?: ExternalProviderCatalog): FieldConfig | undefined {
  const key = field.field.trim();
  if (!key) return undefined;
  return {
    field: key,
    fieldType: field.fieldType,
    displayName: emptyToUndefined(field.displayName),
    titleLanguage:
      field.fieldType === "title" && isIso639TitleLanguage(field.titleLanguage)
        ? field.titleLanguage
        : undefined,
    titleRole: field.fieldType === "title" ? field.titleRole || undefined : undefined,
    externalFields:
      field.fieldType !== "externalRef"
        ? cleanExternalFieldMappings(field.externalFields ?? [], providerCatalog)
        : undefined,
    enumOptions:
      field.fieldType === "enum" || field.fieldType === "enumList"
        ? cleanStrings(field.enumOptions ?? [])
        : undefined,
    totalProgressField:
      field.fieldType === "progress" ? emptyToUndefined(field.totalProgressField) : undefined,
    dateRole:
      field.fieldType === "date" || field.fieldType === "season"
        ? field.dateRole || undefined
        : undefined,
    seasonLanguage: field.fieldType === "season" ? field.seasonLanguage || "zh" : undefined,
    externalRef: field.fieldType === "externalRef" ? emptyToUndefined(field.externalRef) : undefined,
    externalTypes:
      field.fieldType === "externalRef"
        ? cleanExternalTypes(providerCatalog, field.externalRef, field.externalTypes ?? [])
        : undefined,
    relationType: field.fieldType === "relation" ? emptyToUndefined(field.relationType) : undefined,
  };
}

export function defaultVaultConfig(): VaultConfig {
  return {
    taxonomyRoot: "Taxonomy",
    assetRoot: "Assets",
    dailyNotes: defaultDailyNotes(),
    home: defaultHome(),
    types: [defaultEntityType()],
  };
}

export function defaultDailyNotes(): DailyNotesConfig {
  return {
    paths: ["Daily Notes"],
    dateFormat: "YYYY-MM-DD",
  };
}

export function defaultHome(): HomeConfig {
  return { title: "Home", sections: [] };
}

export function defaultHomeSection(type = ""): HomeSectionConfig {
  return {
    id: "section",
    title: "Section",
    type,
    limit: 12,
    sort: "title",
    direction: "asc",
  };
}

export function defaultEntityType(): EntityTypeConfig {
  return {
    id: "type",
    label: "Type",
    icon: "",
    path: "Type",
    externalPriority: [],
    filename: { titleRole: "original" },
    bodySections: [],
    fields: [
      { field: "id", fieldType: "id", displayName: "ID" },
      {
        field: "state",
        fieldType: "enum",
        displayName: "State",
        enumOptions: ["Backlog", "Active", "Completed", "Paused", "Dropped"],
      },
      { field: "progress", fieldType: "progress", displayName: "Progress" },
    ],
  };
}

export function defaultField(): FieldConfig {
  return { field: "field", fieldType: "text", displayName: "" };
}

export function replaceAt<T, K extends keyof T>(
  object: T,
  key: K,
  index: number,
  value: T[K] extends Array<infer U> ? U : never,
): T {
  const current = object[key];
  if (!Array.isArray(current)) return object;
  return { ...object, [key]: replaceArray(current, index, value) };
}

export function replaceArray<T>(items: T[], index: number, value: T) {
  return items.map((item, itemIndex) => (itemIndex === index ? value : item));
}

/// Binds a `values` array to its `onChange` and returns the three list-edit
/// operations the settings editors all need (append / replace-at / remove-at),
/// so each editor stops re-inlining the same spread/`replaceArray`/`filter`
/// closures. `onChange` receives the new array; adapt it (e.g.
/// `(next) => onChange({ ...section, filters: next })`) for nested arrays.
export function arrayEditor<T>(values: T[], onChange: (next: T[]) => void) {
  return {
    append: (item: T) => onChange([...values, item]),
    update: (index: number, item: T) => onChange(replaceArray(values, index, item)),
    remove: (index: number) => onChange(values.filter((_, itemIndex) => itemIndex !== index)),
  };
}

function cleanStrings(values: string[]) {
  return values.map((value) => value.trim()).filter(Boolean);
}

function cleanExternalPriority(providerCatalog: ExternalProviderCatalog | undefined, values: string[]) {
  const deduped = values
    .map((value) => value.trim().toLowerCase())
    .filter((value, index, items) => Boolean(value) && items.indexOf(value) === index);
  // Without a catalog, keep the deduped priority list rather than clearing it.
  if (providerCatalog === undefined) {
    return deduped.length > 0 ? deduped : undefined;
  }
  const allowed = new Set(externalSourceOptions(providerCatalog).map((option) => option.source));
  const filtered = deduped.filter((value) => allowed.has(value));
  return filtered.length > 0 ? filtered : undefined;
}

function emptyToUndefined(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function joinPath(base: string, path: string) {
  if (!base) return path;
  if (!path) return base;
  return `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

export function relativeToBase(path: string, base: string) {
  const normalizedBase = base.replace(/\/+$/, "");
  if (path === normalizedBase) return "";
  if (path.startsWith(`${normalizedBase}/`)) return path.slice(normalizedBase.length + 1);
  return path;
}
