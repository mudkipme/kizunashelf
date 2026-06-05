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
  | "season"
  | "date"
  | "externalRef"
  | "relation"
  | "text"
  | "textList";

export type DateRole = "planning" | "completed";
export type SeasonLanguage = "zh" | "ja" | "en";

export type FilenameConfig = {
  titleLanguage: string;
  defaultTitle?: boolean;
};

export type FieldConfig = {
  field: string;
  fieldType: FieldType;
  displayName?: string | null;
  titleLanguage?: string | null;
  defaultTitle?: boolean | null;
  enumOptions?: string[];
  totalProgressField?: string | null;
  dateRole?: DateRole | null;
  seasonLanguage?: SeasonLanguage | null;
  externalRef?: string | null;
  relationType?: string | null;
};

export type EntityTypeConfig = {
  id: string;
  label: string;
  icon?: string | null;
  path: string;
  filename?: FilenameConfig | null;
  fields: FieldConfig[];
};

export type HomeSectionConfig = {
  id: string;
  title: string;
  type: string;
  limit?: number | null;
  sort?: string | null;
  direction?: "asc" | "desc" | null;
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
  contentWritable?: boolean | null;
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
