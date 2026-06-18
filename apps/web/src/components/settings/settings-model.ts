import {
  externalFieldOptionsForSource,
  externalTypeOptionsForSource,
  externalSourceOptions,
} from "@/lib/external-metadata";
import { isIso639TitleLanguage } from "@/lib/title-language";
import type { ExternalProviderCatalog } from "@/types/api";
import type {
  AppConfig,
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalBodyMapping,
  ExternalFieldMapping,
  FieldConfig,
  FilenameConfig,
  HomeConfig,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  MergedConfig,
  SaveSettingsRequest,
  VaultConfig,
} from "@/types/config";

export function normalizeConfig(app?: AppConfig, vault?: VaultConfig): MergedConfig {
  const base = defaultConfig();
  return {
    vaultRoot: app?.vaultRoot ?? base.vaultRoot,
    contentWritable: app?.contentWritable ?? base.contentWritable,
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
          defaultTitle: config.filename.defaultTitle ?? false,
        }
      : null,
    bodyMappings: config.bodyMappings ?? [],
    fields: (config.fields ?? []).map(normalizeField),
  };
}

function normalizeField(field: FieldConfig): FieldConfig {
  return {
    field: field.field ?? "",
    fieldType: field.fieldType ?? "text",
    displayName: field.displayName ?? "",
    titleLanguage: field.titleLanguage ?? "",
    titleRole: field.titleRole ?? null,
    externalFields: field.externalFields ?? [],
    defaultTitle: field.defaultTitle ?? false,
    enumOptions: field.enumOptions ?? [],
    totalProgressField: field.totalProgressField ?? "",
    dateRole: field.dateRole ?? null,
    seasonLanguage: field.seasonLanguage ?? "zh",
    externalRef: field.externalRef ?? "",
    externalTypes: field.externalTypes ?? [],
    relationType: field.relationType ?? "",
  };
}

export function cleanConfig(
  config: MergedConfig,
  providerCatalog?: ExternalProviderCatalog,
): SaveSettingsRequest {
  return {
    app: {
      vaultRoot: config.vaultRoot,
      contentWritable: config.contentWritable ?? undefined,
    },
    vault: {
      taxonomyRoot: config.taxonomyRoot,
      assetRoot: emptyToUndefined(config.assetRoot),
      dailyNotes: config.dailyNotes
        ? {
            paths: cleanStrings(config.dailyNotes.paths),
            dateFormat: emptyToUndefined(config.dailyNotes.dateFormat),
          }
        : undefined,
      home: config.home
        ? {
            title: emptyToUndefined(config.home.title),
            sections: config.home.sections.map((section) => ({
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
      types: config.types.map((typeConfig) => ({
        id: typeConfig.id,
        label: typeConfig.label,
        icon: emptyToUndefined(typeConfig.icon),
        path: typeConfig.path,
        externalPriority: cleanExternalPriority(providerCatalog, typeConfig.externalPriority ?? []),
        filename: cleanFilename(typeConfig.filename),
        bodyMappings: cleanExternalBodyMappings(typeConfig.bodyMappings ?? [], providerCatalog),
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
  if (!titleLanguage && !filename.defaultTitle) return undefined;
  return {
    titleLanguage,
    defaultTitle: filename.defaultTitle || undefined,
  };
}

function cleanExternalFieldMappings(values: ExternalFieldMapping[], providerCatalog?: ExternalProviderCatalog) {
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
    }))
    .filter((value) =>
      externalFieldOptionsForSource(providerCatalog, value.source).some((option) => option.field === value.field),
    );
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalBodyMappings(values: ExternalBodyMapping[], providerCatalog?: ExternalProviderCatalog) {
  const seen = new Set<string>();
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
      heading: value.heading.trim(),
    }))
    .filter((value) => {
      if (!value.heading) return false;
      if (!externalFieldOptionsForSource(providerCatalog, value.source).some((option) => option.field === value.field)) {
        return false;
      }
      const key = `${value.source}:${value.field}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalTypes(providerCatalog: ExternalProviderCatalog | undefined, source: string | null | undefined, values: string[]) {
  const options = externalTypeOptionsForSource(providerCatalog, source ?? "");
  const allowed = new Set(options.map((option) => option.value));
  const cleaned = values.map((value) => value.trim()).filter((value) => allowed.has(value));
  return cleaned.length > 0 ? cleaned : undefined;
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
    defaultTitle: field.fieldType === "title" && field.defaultTitle ? true : undefined,
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

export function defaultConfig(): MergedConfig {
  return {
    vaultRoot: "",
    taxonomyRoot: "Taxonomy",
    assetRoot: "Assets",
    contentWritable: true,
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
    filename: { defaultTitle: true },
    bodyMappings: [],
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

function cleanStrings(values: string[]) {
  return values.map((value) => value.trim()).filter(Boolean);
}

function cleanExternalPriority(providerCatalog: ExternalProviderCatalog | undefined, values: string[]) {
  const allowed = new Set(externalSourceOptions(providerCatalog).map((option) => option.source));
  const cleaned = values
    .map((value) => value.trim().toLowerCase())
    .filter((value, index, items) => allowed.has(value) && items.indexOf(value) === index);
  return cleaned.length > 0 ? cleaned : undefined;
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
