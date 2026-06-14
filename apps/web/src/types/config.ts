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
  datePattern?: string | null;
  snippetMaxLength?: number | null;
};

// App-level config stored in the local app config file (e.g. ~/.config/kizunashelf.yaml).
// Describes how this machine runs the app and where the vault lives on disk.
export type AppConfig = {
  vaultRoot: string;
  contentWritable?: boolean | null;
  // Operational tuning knob configured via the config file / KIZUNASHELF_READ_CONCURRENCY
  // env var only — not surfaced in the Settings UI, but round-tripped on save so an
  // existing value is never clobbered.
  readConcurrency?: number | null;
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

// Editor working shape: app + vault fields flattened into one form model.
export type MergedConfig = AppConfig & VaultConfig;

export type SaveSettingsRequest = {
  app: AppConfig;
  // Omitted (or null) when persisting only the app config — e.g. the onboarding
  // vault picker, which must not overwrite an existing synced vault config.
  vault?: VaultConfig | null;
};

export type SettingsConfigResponse = {
  appConfigPath: string;
  appExists: boolean;
  app?: AppConfig;
  vaultConfigPath?: string;
  vaultExists: boolean;
  vault?: VaultConfig;
  error?: string;
};

export type PathSuggestionsResponse = {
  suggestions: string[];
};
