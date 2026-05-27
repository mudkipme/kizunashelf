import {
  GetAnalyticsResponse,
  GetCalendarResponse,
  GetConfigResponse,
  GetEntitiesResponse,
  GetEntityDatesResponse,
  GetEntityResponse,
  GetHealthResponse,
  GetHomeResponse,
  GetRelationGroupResponse,
  GetRelationGroupsResponse,
  GetRelationsResponse,
  GetRelationTargetResponse,
  GetStatsResponse,
} from "./generated.js";
import type { z } from "zod";

export * from "./generated.js";

export const ApiResponseSchemas = {
  health: GetHealthResponse,
  config: GetConfigResponse,
  home: GetHomeResponse,
  stats: GetStatsResponse,
  analytics: GetAnalyticsResponse,
  entities: GetEntitiesResponse,
  entityDetail: GetEntityResponse,
  entityDates: GetEntityDatesResponse,
  relations: GetRelationsResponse,
  relationGroups: GetRelationGroupsResponse,
  relationField: GetRelationGroupResponse,
  relationTarget: GetRelationTargetResponse,
  calendar: GetCalendarResponse,
} as const;

export type HealthResponse = z.infer<typeof GetHealthResponse>;
export type ConfigResponse = z.infer<typeof GetConfigResponse>;
export type HomeResponse = z.infer<typeof GetHomeResponse>;
export type StatsResponse = z.infer<typeof GetStatsResponse>;
export type AnalyticsResponse = z.infer<typeof GetAnalyticsResponse>;
export type EntityListResponse = z.infer<typeof GetEntitiesResponse>;
export type EntityDetailResponse = z.infer<typeof GetEntityResponse>;
export type EntityDatesResponse = z.infer<typeof GetEntityDatesResponse>;
export type RelationListResponse = z.infer<typeof GetRelationsResponse>;
export type RelationGroupsResponse = z.infer<typeof GetRelationGroupsResponse>;
export type RelationFieldResponse = z.infer<typeof GetRelationGroupResponse>;
export type RelationTargetResponse = z.infer<typeof GetRelationTargetResponse>;
export type CalendarResponse = z.infer<typeof GetCalendarResponse>;

export type Entity = EntityDetailResponse["entity"];
export type EntitySummary = EntityListResponse["items"][number];
export type EntityDateValue = EntitySummary["dates"][number];
export type Relation = EntityDetailResponse["relations"][number];
export type CalendarDay = CalendarResponse["days"][number];
export type CalendarEntry = CalendarDay["entries"][number];
export type CalendarSnippet = NonNullable<CalendarEntry["snippets"]>[number];
export type HomeSectionResponse = HomeResponse["sections"][number];
export type TypeConfig = ConfigResponse["types"][number];
export type RelationTargetSummary = RelationGroupsResponse["fields"][number]["topTargets"][number];
export type RelationFieldSummary = RelationGroupsResponse["fields"][number];
export type AnalyticsCoverageMetric = AnalyticsResponse["coverage"][number];
export type AnalyticsTimelineYear = AnalyticsResponse["timeline"]["years"][number];
export type AnalyticsRelationHub = AnalyticsResponse["relations"]["topTargets"][number];
export type EntityDateMetadataEntry = EntityDatesResponse["metadata"][number];
export type EntityDateDailyNoteEntry = EntityDatesResponse["dailyNotes"][number];
