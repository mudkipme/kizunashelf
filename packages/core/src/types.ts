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

export type KizunaConfig = {
  vaultRoot: string;
  taxonomyRoot: string;
  types: EntityTypeConfig[];
  relationshipFields: string[];
};

export type EntitySummary = {
  id: string;
  type: string;
  typeLabel: string;
  title: string;
  subtitle?: string;
  status?: string;
  date?: string;
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
