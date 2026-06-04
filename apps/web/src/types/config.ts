export type EntityFieldsConfig = {
  titleLanguages: Record<string, string[]>;
  subtitle: string[];
  image: string[];
  status: string[];
  dateRoles: DateRoleConfig;
  externalRefs: string[];
  relations: string[];
};

export type DateRoleConfig = {
  planning: string[];
  completed: string[];
};

export type EntityTypeConfig = {
  id: string;
  label: string;
  icon?: string | null;
  path: string;
  defaultTitleLanguage?: string | null;
  fields: EntityFieldsConfig;
};

export type HomeSectionConfig = {
  id: string;
  title: string;
  type: string;
  status?: string | string[] | null;
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
  relationshipFields: string[];
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
