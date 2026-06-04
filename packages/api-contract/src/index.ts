import {
  AnalyticsResponse as AnalyticsResponseSchema,
} from "./generated/analyticsResponse.zod.js";
import { CalendarResponse as CalendarResponseSchema } from "./generated/calendarResponse.zod.js";
import { CleanupQueuesResponse as CleanupQueuesResponseSchema } from "./generated/cleanupQueuesResponse.zod.js";
import { ConfigResponse as ConfigResponseSchema } from "./generated/configResponse.zod.js";
import { EntityDatesResponse as EntityDatesResponseSchema } from "./generated/entityDatesResponse.zod.js";
import { EntityDetailResponse as EntityDetailResponseSchema } from "./generated/entityDetailResponse.zod.js";
import { EntityListResponse as EntityListResponseSchema } from "./generated/entityListResponse.zod.js";
import { HealthResponse as HealthResponseSchema } from "./generated/healthResponse.zod.js";
import { HomeResponse as HomeResponseSchema } from "./generated/homeResponse.zod.js";
import { RelationGroupsResponse as RelationGroupsResponseSchema } from "./generated/relationGroupsResponse.zod.js";
import { RelationListResponse as RelationListResponseSchema } from "./generated/relationListResponse.zod.js";
import { StatsResponse as StatsResponseSchema } from "./generated/statsResponse.zod.js";
import type { AnalyticsResponse } from "./generated/analyticsResponse.zod.js";
import type { CalendarResponse } from "./generated/calendarResponse.zod.js";
import type { CleanupQueuesResponse } from "./generated/cleanupQueuesResponse.zod.js";
import type { ConfigResponse } from "./generated/configResponse.zod.js";
import type { EntityDatesResponse } from "./generated/entityDatesResponse.zod.js";
import type { EntityDetailResponse } from "./generated/entityDetailResponse.zod.js";
import type { EntityListResponse } from "./generated/entityListResponse.zod.js";
import type { HomeResponse } from "./generated/homeResponse.zod.js";
import type { RelationFieldSummary as RelationFieldSummaryType } from "./generated/relationFieldSummary.zod.js";
import type { RelationGroupsResponse } from "./generated/relationGroupsResponse.zod.js";
import type { RelationTargetSummary as RelationTargetSummaryType } from "./generated/relationTargetSummary.zod.js";

export * from "./generated/client.js";
export * from "./generated/analyticsResponse.zod.js";
export * from "./generated/calendarResponse.zod.js";
export * from "./generated/cleanupQueueSummary.zod.js";
export * from "./generated/cleanupQueuesResponse.zod.js";
export * from "./generated/cleanupUnresolvedRelation.zod.js";
export * from "./generated/configResponse.zod.js";
export * from "./generated/entityDatesResponse.zod.js";
export * from "./generated/entityDetailResponse.zod.js";
export * from "./generated/entityListResponse.zod.js";
export * from "./generated/getCalendarParams.zod.js";
export * from "./generated/getEntitiesParams.zod.js";
export * from "./generated/getRelationsParams.zod.js";
export * from "./generated/getStatsParams.zod.js";
export * from "./generated/healthResponse.zod.js";
export * from "./generated/homeResponse.zod.js";
export * from "./generated/relationGroupsResponse.zod.js";
export * from "./generated/relationListResponse.zod.js";
export * from "./generated/relationTargetTypeSummary.zod.js";
export * from "./generated/statsResponse.zod.js";

export const ApiResponseSchemas = {
  health: HealthResponseSchema,
  config: ConfigResponseSchema,
  home: HomeResponseSchema,
  stats: StatsResponseSchema,
  analytics: AnalyticsResponseSchema,
  cleanupQueues: CleanupQueuesResponseSchema,
  entities: EntityListResponseSchema,
  entityDetail: EntityDetailResponseSchema,
  entityDates: EntityDatesResponseSchema,
  relations: RelationListResponseSchema,
  relationGroups: RelationGroupsResponseSchema,
  calendar: CalendarResponseSchema,
} as const;

export type Entity = EntityDetailResponse["entity"];
export type EntitySummary = EntityListResponse["items"][number];
export type EntityDateValue = EntitySummary["dates"][number];
export type Relation = EntityDetailResponse["relations"][number];
export type CalendarDay = CalendarResponse["days"][number];
export type CalendarEntry = CalendarDay["entries"][number];
export type CalendarSnippet = NonNullable<CalendarEntry["snippets"]>[number];
export type CleanupQueueSummary = CleanupQueuesResponse["queues"][number];
export type CleanupUnresolvedRelation = CleanupQueuesResponse["unresolvedRelations"][number];
export type HomeSectionResponse = HomeResponse["sections"][number];
export type TypeConfig = ConfigResponse["types"][number];
export type RelationTargetSummary = RelationTargetSummaryType;
export type RelationTargetTypeSummary = RelationGroupsResponse["targetTypes"][number];
export type RelationTargetHubSummary = RelationGroupsResponse["targetTypes"][number]["topTargets"][number];
export type RelationFieldSummary = RelationFieldSummaryType;
export type AnalyticsCoverageMetric = AnalyticsResponse["coverage"][number];
export type AnalyticsTimelineYear = AnalyticsResponse["timeline"]["years"][number];
export type AnalyticsRelationHub = AnalyticsResponse["relations"]["topTargets"][number];
export type EntityDateMetadataEntry = EntityDatesResponse["metadata"][number];
export type EntityDateDailyNoteEntry = EntityDatesResponse["dailyNotes"][number];
