export type TypeConfig = {
  id: string;
  label: string;
  path: string;
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

export type ConfigResponse = {
  taxonomyRoot: string;
  types: TypeConfig[];
};

export type StatsResponse = {
  generatedAt: string;
  total: number;
  relations: number;
  byType: Array<{ id: string; label: string; count: number }>;
  byStatus: Array<{ name: string; count: number }>;
  topRelations: EntitySummary[];
};

export type EntityDetailResponse = {
  entity: Entity;
  relations: Relation[];
};

export type EntityListResponse = {
  items: EntitySummary[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
};
