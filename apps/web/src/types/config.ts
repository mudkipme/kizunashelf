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

export type KizunaConfig = {
  vaultRoot: string;
  taxonomyRoot: string;
  assetRoot?: string | null;
  contentWritable?: boolean | null;
  // Operational tuning knob configured via the config file / KIZUNASHELF_READ_CONCURRENCY
  // env var only — not surfaced in the Settings UI, but round-tripped on save so an
  // existing value is never clobbered.
  readConcurrency?: number | null;
  home?: HomeConfig | null;
  dailyNotes?: DailyNotesConfig | null;
  types: EntityTypeConfig[];
};

export type SettingsConfigResponse = {
  configPath: string;
  exists: boolean;
  config?: KizunaConfig;
  error?: string;
};

export type PathSuggestionsResponse = {
  suggestions: string[];
};
