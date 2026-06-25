import {
  AnalyticsResponse as AnalyticsResponseSchema,
} from "./generated/analyticsResponse.zod.js";
import { CalendarPlanningResponse as CalendarPlanningResponseSchema } from "./generated/calendarPlanningResponse.zod.js";
import { CalendarResponse as CalendarResponseSchema } from "./generated/calendarResponse.zod.js";
import { CapabilitiesResponse as CapabilitiesResponseSchema } from "./generated/capabilitiesResponse.zod.js";
import { CleanupQueuesResponse as CleanupQueuesResponseSchema } from "./generated/cleanupQueuesResponse.zod.js";
import { ConfigResponse as ConfigResponseSchema } from "./generated/configResponse.zod.js";
import { EntityDatesResponse as EntityDatesResponseSchema } from "./generated/entityDatesResponse.zod.js";
import { EntityDetailResponse as EntityDetailResponseSchema } from "./generated/entityDetailResponse.zod.js";
import { EntityListResponse as EntityListResponseSchema } from "./generated/entityListResponse.zod.js";
import { EntityMutationResponse as EntityMutationResponseSchema } from "./generated/entityMutationResponse.zod.js";
import { ExternalSearchResponse as ExternalSearchResponseSchema } from "./generated/externalSearchResponse.zod.js";
import { ExternalProviderCatalogResponse as ExternalProviderCatalogResponseSchema } from "./generated/externalProviderCatalogResponse.zod.js";
import { HealthResponse as HealthResponseSchema } from "./generated/healthResponse.zod.js";
import { HomeResponse as HomeResponseSchema } from "./generated/homeResponse.zod.js";
import { RelationGroupsResponse as RelationGroupsResponseSchema } from "./generated/relationGroupsResponse.zod.js";
import { RelationListResponse as RelationListResponseSchema } from "./generated/relationListResponse.zod.js";
import { StatsResponse as StatsResponseSchema } from "./generated/statsResponse.zod.js";
import type { AnalyticsResponse } from "./generated/analyticsResponse.zod.js";
import type { CalendarPlanningResponse } from "./generated/calendarPlanningResponse.zod.js";
import type { CalendarResponse } from "./generated/calendarResponse.zod.js";
import type { CapabilitiesResponse } from "./generated/capabilitiesResponse.zod.js";
import type { CleanupQueuesResponse } from "./generated/cleanupQueuesResponse.zod.js";
import type { ConfigResponse } from "./generated/configResponse.zod.js";
import type { EntityDatesResponse } from "./generated/entityDatesResponse.zod.js";
import type { EntityDetailResponse } from "./generated/entityDetailResponse.zod.js";
import type { EntityListResponse } from "./generated/entityListResponse.zod.js";
import type { EntityMutationResponse } from "./generated/entityMutationResponse.zod.js";
import type { ExternalSearchResponse } from "./generated/externalSearchResponse.zod.js";
import type { ExternalProviderCatalogResponse } from "./generated/externalProviderCatalogResponse.zod.js";
import type { HomeResponse } from "./generated/homeResponse.zod.js";
import type { RelationFieldSummary as RelationFieldSummaryType } from "./generated/relationFieldSummary.zod.js";
import type { RelationGroupsResponse } from "./generated/relationGroupsResponse.zod.js";
import type { RelationTargetSummary as RelationTargetSummaryType } from "./generated/relationTargetSummary.zod.js";

export * from "./generated/client.js";
export * from "./generated/analyticsResponse.zod.js";
export * from "./generated/assetDownloadItemResult.zod.js";
export * from "./generated/assetDownloadJob.zod.js";
export * from "./generated/assetDownloadJobError.zod.js";
export * from "./generated/assetDownloadJobListResponse.zod.js";
export * from "./generated/assetDownloadJobRequest.zod.js";
export * from "./generated/assetDownloadJobStatus.zod.js";
export * from "./generated/assetDownloadRequest.zod.js";
export * from "./generated/assetDownloadResponse.zod.js";
export * from "./generated/assetDownloadStatus.zod.js";
export * from "./generated/calendarPlanningBoard.zod.js";
export * from "./generated/calendarPlanningDatePoint.zod.js";
export * from "./generated/calendarPlanningFilters.zod.js";
export * from "./generated/calendarPlanningMonth.zod.js";
export * from "./generated/calendarPlanningResponse.zod.js";
export * from "./generated/calendarPlanningSeason.zod.js";
export * from "./generated/calendarPlanningTotals.zod.js";
export * from "./generated/calendarPlanningTypeOption.zod.js";
export * from "./generated/calendarResponse.zod.js";
export * from "./generated/capabilitiesResponse.zod.js";
export * from "./generated/cleanupQueueSummary.zod.js";
export * from "./generated/cleanupQueuesResponse.zod.js";
export * from "./generated/cleanupUnresolvedRelation.zod.js";
export * from "./generated/configResponse.zod.js";
export * from "./generated/createEntityRequest.zod.js";
export * from "./generated/deleteEntityRequest.zod.js";
export * from "./generated/deleteEntityResponse.zod.js";
export * from "./generated/entityDatesResponse.zod.js";
export * from "./generated/entityDetailResponse.zod.js";
export * from "./generated/entityListResponse.zod.js";
export * from "./generated/entityMutationResponse.zod.js";
export * from "./generated/externalCandidate.zod.js";
export * from "./generated/bodySection.zod.js";
export * from "./generated/bodySectionKind.zod.js";
export * from "./generated/entityEpisodes.zod.js";
export * from "./generated/episode.zod.js";
export * from "./generated/episodeGroup.zod.js";
export * from "./generated/episodeProgress.zod.js";
export * from "./generated/episodeTracking.zod.js";
export * from "./generated/updateEpisodesRequest.zod.js";
export * from "./generated/externalProviderCatalogItem.zod.js";
export * from "./generated/externalProviderCatalogResponse.zod.js";
export * from "./generated/externalProviderFieldOption.zod.js";
export * from "./generated/externalProviderSummary.zod.js";
export * from "./generated/externalProviderTypeOption.zod.js";
export * from "./generated/externalSearchResponse.zod.js";
export * from "./generated/getCalendarPlanningParams.zod.js";
export * from "./generated/getCalendarParams.zod.js";
export * from "./generated/getEntitiesParams.zod.js";
export * from "./generated/getListsParams.zod.js";
export * from "./generated/getRelationsParams.zod.js";
export * from "./generated/getStatsParams.zod.js";
export * from "./generated/healthResponse.zod.js";
export * from "./generated/homeResponse.zod.js";
export * from "./generated/searchExternalSourcesParams.zod.js";
export * from "./generated/updateEntityRequest.zod.js";
export * from "./generated/libraryDiagnostic.zod.js";
export * from "./generated/relationGroupsResponse.zod.js";
export * from "./generated/relationListResponse.zod.js";
export * from "./generated/relationTargetTypeSummary.zod.js";
export * from "./generated/statsResponse.zod.js";
export * from "./generated/language.zod.js";
export * from "./generated/languagesResponse.zod.js";
export * from "./generated/vaultTemplate.zod.js";
export * from "./generated/vaultTemplatesResponse.zod.js";
export * from "./generated/tagsResponse.zod.js";
export * from "./generated/episodeSyncResponse.zod.js";
export * from "./generated/episodeSource.zod.js";
export * from "./generated/providerEpisodeGroup.zod.js";
export * from "./generated/providerEpisodeItem.zod.js";
export * from "./generated/fetchEpisodesRequest.zod.js";
export * from "./generated/importEpisodesRequest.zod.js";
export * from "./generated/listsResponse.zod.js";
export * from "./generated/listSummary.zod.js";
export * from "./generated/listDetail.zod.js";
export * from "./generated/listSection.zod.js";
export * from "./generated/listSectionInput.zod.js";
export * from "./generated/listItem.zod.js";
export * from "./generated/listItemInput.zod.js";
export * from "./generated/createListRequest.zod.js";
export * from "./generated/updateListRequest.zod.js";
export * from "./generated/addListItemRequest.zod.js";
export * from "./generated/deleteListResponse.zod.js";
export * from "./generated/saveSettingsRequest.zod.js";
export * from "./generated/settingsConfigResponse.zod.js";
export * from "./generated/rawConfigResponse.zod.js";
export * from "./generated/saveRawConfigRequest.zod.js";
export * from "./generated/pathSuggestionsResponse.zod.js";

export const ApiResponseSchemas = {
  health: HealthResponseSchema,
  capabilities: CapabilitiesResponseSchema,
  config: ConfigResponseSchema,
  home: HomeResponseSchema,
  stats: StatsResponseSchema,
  analytics: AnalyticsResponseSchema,
  cleanupQueues: CleanupQueuesResponseSchema,
  entities: EntityListResponseSchema,
  entityDetail: EntityDetailResponseSchema,
  entityMutation: EntityMutationResponseSchema,
  entityDates: EntityDatesResponseSchema,
  externalProviderCatalog: ExternalProviderCatalogResponseSchema,
  externalSearch: ExternalSearchResponseSchema,
  relations: RelationListResponseSchema,
  relationGroups: RelationGroupsResponseSchema,
  calendar: CalendarResponseSchema,
  calendarPlanning: CalendarPlanningResponseSchema,
} as const;

export type Entity = EntityDetailResponse["entity"];
export type Capabilities = CapabilitiesResponse;
export type EntitySummary = EntityListResponse["items"][number];
export type MutatedEntity = EntityMutationResponse["entity"];
export type ExternalCandidate = ExternalSearchResponse["items"][number];
export type ExternalProviderCatalog = ExternalProviderCatalogResponse;
export type ExternalProviderCatalogItem = ExternalProviderCatalogResponse["providers"][number];
export type ExternalProviderSummary = ExternalSearchResponse["providers"][number];
export type EntityDateValue = EntitySummary["dates"][number];
export type Relation = EntityDetailResponse["relations"][number];
export type CalendarDay = CalendarResponse["days"][number];
export type CalendarEntry = CalendarDay["entries"][number];
export type CalendarSnippet = NonNullable<CalendarEntry["snippets"]>[number];
export type CalendarPlanningDatePoint = CalendarPlanningResponse["board"]["upcoming"][number];
export type CalendarPlanningMonth = CalendarPlanningResponse["yearMonths"][number];
export type CalendarPlanningSeason = CalendarPlanningResponse["seasons"][number];
export type CleanupQueueSummary = CleanupQueuesResponse["queues"][number];
export type CleanupUnresolvedRelation = CleanupQueuesResponse["unresolvedRelations"][number];
export type HomeSectionResponse = HomeResponse["sections"][number];
export type TypeConfig = ConfigResponse["types"][number];
export type RelationTargetSummary = RelationTargetSummaryType;
export type RelationTargetTypeSummary = RelationGroupsResponse["targetTypes"][number];
export type RelationTargetHubSummary = RelationGroupsResponse["targetTypes"][number]["topTargets"][number];
export type RelationFieldSummary = RelationFieldSummaryType;
export type AnalyticsActivity = AnalyticsResponse["activity"];
export type AnalyticsActivityType = AnalyticsResponse["activity"]["types"][number];
export type AnalyticsActivityYear = AnalyticsResponse["activity"]["years"][number];
export type AnalyticsRelationHub = AnalyticsResponse["relations"]["topTargets"][number];
export type EntityDateMetadataEntry = EntityDatesResponse["metadata"][number];
export type EntityDateDailyNoteEntry = EntityDatesResponse["dailyNotes"][number];
