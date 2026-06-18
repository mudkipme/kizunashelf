export type FieldType =
  | "id"
  | "title"
  | "image"
  | "imageList"
  | "enum"
  | "enumList"
  | "progress"
  | "totalProgress"
  | "rating"
  | "bool"
  | "season"
  | "date"
  | "externalRef"
  | "relation"
  | "text"
  | "textList";

export type DateRole = "planning" | "completed";
export type SeasonLanguage = "zh" | "ja" | "en";
export type TitleRole = "original";

export type ExternalFieldMapping = {
  source: string;
  field: string;
};

export type ExternalBodyMapping = {
  source: string;
  field: string;
  heading: string;
};

export type FilenameConfig = {
  titleLanguage?: string | null;
  defaultTitle?: boolean;
};

export type FieldConfig = {
  field: string;
  fieldType: FieldType;
  displayName?: string | null;
  titleLanguage?: string | null;
  titleRole?: TitleRole | null;
  externalFields?: ExternalFieldMapping[];
  defaultTitle?: boolean | null;
  enumOptions?: string[];
  totalProgressField?: string | null;
  dateRole?: DateRole | null;
  seasonLanguage?: SeasonLanguage | null;
  externalRef?: string | null;
  externalTypes?: string[];
  relationType?: string | null;
};

export type EntityTypeConfig = {
  id: string;
  label: string;
  icon?: string | null;
  path: string;
  externalPriority?: string[];
  filename?: FilenameConfig | null;
  bodyMappings?: ExternalBodyMapping[];
  fields: FieldConfig[];
};

export type HomeSectionConfig = {
  id: string;
  title: string;
  type: string;
  filters?: HomeSectionFilterConfig[];
  limit?: number | null;
  sort?: string | null;
  direction?: "asc" | "desc" | null;
};

export type HomeSectionFilterConfig = {
  field: string;
  values: string[];
};

export type HomeConfig = {
  title?: string | null;
  sections: HomeSectionConfig[];
};

export type DailyNotesConfig = {
  paths: string[];
  dateFormat?: string | null;
};

// App-level config: where the vault lives and whether it is writable. Sourced by
// the runtime (env vars on the web app; the in-app vault list on desktop/iOS),
// not a synced file.
export type AppConfig = {
  vaultRoot: string;
  contentWritable?: boolean | null;
};

// Vault-level config stored inside the vault at <vaultRoot>/.kizunashelf/config.yaml.
// Describes the vault's content schema and travels with the vault when synced.
export type VaultConfig = {
  taxonomyRoot: string;
  assetRoot?: string | null;
  home?: HomeConfig | null;
  dailyNotes?: DailyNotesConfig | null;
  types: EntityTypeConfig[];
};

// A built-in starter schema offered during onboarding, served by
// GET /api/vault-templates. The single source of truth lives in the Rust core.
export type VaultTemplate = {
  id: string;
  label: string;
  config: VaultConfig;
};

export type VaultTemplatesResponse = {
  templates: VaultTemplate[];
};

// A title-language option for the schema editor (ISO 639-1 code + English name),
// served by GET /api/languages. The single source of truth lives in the core.
export type Language = {
  code: string;
  label: string;
};

export type LanguagesResponse = {
  languages: Language[];
};

// The schema editor saves the vault config only; the vault root + write mode are
// owned server-side per runtime (env / native switcher / @AppStorage).
export type SaveSettingsRequest = {
  vault?: VaultConfig | null;
};

export type SettingsConfigResponse = {
  // The inline app config (vault root + write mode), owned by the runtime.
  app?: AppConfig;
  vaultConfigPath?: string;
  vaultExists: boolean;
  vault?: VaultConfig;
  error?: string;
};

export type PathSuggestionsResponse = {
  suggestions: string[];
};
