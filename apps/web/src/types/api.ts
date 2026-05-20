import type {
  Entity,
  EntitySummary,
  EntityTypeConfig,
  HomeConfig,
  HomeSectionConfig,
  Relation,
} from "@kizunashelf/core";

export type {
  Entity,
  EntityDateValue,
  EntitySummary,
  HomeConfig,
  HomeSectionConfig,
  Relation,
} from "@kizunashelf/core";

export type TypeConfig = Pick<EntityTypeConfig, "id" | "label" | "path">;

export type ConfigResponse = {
  taxonomyRoot: string;
  home?: HomeConfig;
  types: TypeConfig[];
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
  dateFields: string[];
  byStatus: Array<{ name: string; count: number }>;
  topRelations: EntitySummary[];
};

export type EntityDetailResponse = {
  entity: Entity;
  relations: Relation[];
};

export type EntityDateMetadataEntry = {
  id: string;
  field: string;
  value: string;
  date?: string;
};

export type EntityDateDailyNoteEntry = {
  id: string;
  date: string;
  notePath: string;
  snippets: CalendarSnippet[];
};

export type EntityDatesResponse = {
  generatedAt: string;
  entityId: string;
  totals: {
    metadata: number;
    dailyNotes: number;
    snippets: number;
  };
  metadata: EntityDateMetadataEntry[];
  dailyNotes: EntityDateDailyNoteEntry[];
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

export type AnalyticsCoverageMetric = {
  name: string;
  count: number;
  missing: number;
  total: number;
  percent: number;
};

export type AnalyticsTimelineYear = {
  year: number;
  count: number;
  byType: Array<{ name: string; count: number }>;
  examples: EntitySummary[];
};

export type AnalyticsRelationHub = RelationTargetSummary & {
  fields: Array<{ name: string; count: number }>;
};

export type AnalyticsResponse = {
  generatedAt: string;
  totals: {
    entities: number;
    relations: number;
    unresolvedRelations: number;
    datedEntities: number;
    connectedEntities: number;
  };
  distributions: {
    byType: Array<{ id: string; label: string; count: number }>;
    byStatus: Array<{ name: string; count: number }>;
    byRelationField: Array<{ name: string; count: number }>;
    bySourceTargetType: Array<{ name: string; count: number }>;
  };
  coverage: AnalyticsCoverageMetric[];
  timeline: {
    totalDated: number;
    years: AnalyticsTimelineYear[];
    seasons: Array<{ name: string; count: number }>;
    months: Array<{ name: string; count: number }>;
  };
  relations: {
    topFields: RelationFieldSummary[];
    topTargets: AnalyticsRelationHub[];
    unresolved: {
      count: number;
      examples: Relation[];
    };
  };
  dataQuality: {
    missingCover: EntitySummary[];
    missingExternalRefs: EntitySummary[];
    missingSummary: EntitySummary[];
    isolated: EntitySummary[];
  };
};

export type CalendarSnippet = {
  text: string;
  heading?: string;
  line: number;
};

export type CalendarEntry = {
  id: string;
  date: string;
  source: "taxonomy" | "daily-note";
  entity: EntitySummary;
  dateField?: string;
  rawDate?: string;
  notePath?: string;
  snippets?: CalendarSnippet[];
};

export type CalendarDay = {
  date: string;
  entries: CalendarEntry[];
  counts: {
    total: number;
    taxonomy: number;
    dailyNotes: number;
  };
};

export type CalendarResponse = {
  generatedAt: string;
  year: number;
  month: number;
  filters: {
    type?: string;
    source: "all" | "taxonomy" | "daily-note";
  };
  totals: {
    entries: number;
    taxonomy: number;
    dailyNotes: number;
    daysWithEntries: number;
  };
  days: CalendarDay[];
};
