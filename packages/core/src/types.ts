export type EntityTypeConfig = {
  id: string;
  label: string;
  path: string;
  fields: {
    title?: string[];
    subtitle?: string[];
    image?: string[];
    status?: string[];
    date?: string[];
    externalRefs?: string[];
    relations?: string[];
  };
};

export type HomeSectionConfig = {
  id: string;
  title: string;
  type: string;
  status?: string | string[];
  limit?: number;
  sort?: string;
  direction?: "asc" | "desc";
};

export type HomeConfig = {
  title?: string;
  sections?: HomeSectionConfig[];
};

export type DailyNotesConfig = {
  paths?: string[];
  datePattern?: string;
  snippetMaxLength?: number;
};

export type KizunaConfig = {
  vaultRoot: string;
  taxonomyRoot: string;
  types: EntityTypeConfig[];
  relationshipFields: string[];
  readConcurrency?: number;
  home?: HomeConfig;
  dailyNotes?: DailyNotesConfig;
};

export type EntityDateValue = {
  field: string;
  value: string;
};

export type EntitySummary = {
  id: string;
  type: string;
  typeLabel: string;
  title: string;
  subtitle?: string;
  status?: string;
  dates: EntityDateValue[];
  image?: string;
  summary?: string;
  path: string;
  basename: string;
  externalRefs: Record<string, string>;
  relationCount: number;
};

export type Entity = EntitySummary & {
  frontmatter: Record<string, unknown>;
  body: string;
  raw: string;
};

export type Relation = {
  sourceId: string;
  targetId?: string;
  targetTitle: string;
  targetType?: string;
  field: string;
  direction: "out" | "in";
};

export type Library = {
  config: KizunaConfig;
  entities: Entity[];
  summaries: EntitySummary[];
  relations: Relation[];
  generatedAt: string;
};
