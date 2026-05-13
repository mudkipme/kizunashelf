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
  home?: HomeConfig;
  types: TypeConfig[];
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

export type HomeSectionResponse = Omit<HomeSectionConfig, "status"> & {
  typeLabel: string;
  status: string[];
  limit: number;
  sort: string;
  direction: "asc" | "desc";
  total: number;
  items: EntitySummary[];
};

export type HomeResponse = {
  generatedAt: string;
  title: string;
  sections: HomeSectionResponse[];
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

export type RelationTargetSummary = {
  key: string;
  targetTitle: string;
  targetId?: string;
  targetType?: string;
  targetTypeLabel?: string;
  count: number;
  sourceTypes: Array<{ name: string; count: number }>;
  examples: EntitySummary[];
};

export type RelationFieldSummary = {
  field: string;
  edgeCount: number;
  sourceCount: number;
  uniqueTargets: number;
  resolvedTargets: number;
  topTargets: RelationTargetSummary[];
};

export type RelationGroupsResponse = {
  generatedAt: string;
  fields: RelationFieldSummary[];
};

export type RelationFieldResponse = {
  generatedAt: string;
  field: string;
  edgeCount: number;
  uniqueTargets: number;
  targets: RelationTargetSummary[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
};

export type RelationTargetResponse = {
  generatedAt: string;
  field: string;
  target: RelationTargetSummary;
  groups: Array<{
    typeLabel: string;
    count: number;
    items: EntitySummary[];
  }>;
  total: number;
};
