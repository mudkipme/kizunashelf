export type {
  AnalyticsActivity,
  AnalyticsActivityType,
  AnalyticsActivityYear,
  AnalyticsRelationHub,
  AnalyticsResponse,
  AssetDownloadItemResult,
  AssetDownloadJob,
  AssetDownloadJobListResponse,
  AssetDownloadRequest,
  AssetDownloadResponse,
  CalendarDay,
  CalendarEntry,
  CalendarPlanningDatePoint,
  CalendarPlanningMonth,
  CalendarPlanningResponse,
  CalendarPlanningSeason,
  CalendarResponse,
  CalendarSnippet,
  Capabilities,
  CleanupQueueSummary,
  CleanupQueuesResponse,
  CleanupUnresolvedRelation,
  ConfigResponse,
  Entity,
  EntityDateDailyNoteEntry,
  EntityDatesResponse,
  EntityDateMetadataEntry,
  EntityDateValue,
  EntityDetailResponse,
  EntityListResponse,
  EntityMutationResponse,
  EntitySummary,
  ExternalCandidate,
  ExternalProviderCatalog,
  ExternalProviderCatalogItem,
  ExternalProviderSummary,
  ExternalSearchResponse,
  HomeResponse,
  HomeSectionResponse,
  Language,
  LanguagesResponse,
  ListDetail,
  ListItem,
  ListItemInput,
  ListsResponse,
  ListSummary,
  PathSuggestionsResponse,
  Relation,
  RelationFieldSummary,
  RelationGroupsResponse,
  RelationTargetHubSummary,
  RelationTargetSummary,
  RelationTargetTypeSummary,
  SaveSettingsRequest,
  SettingsConfigResponse,
  StatsResponse,
  TypeConfig,
  VaultTemplate,
  VaultTemplatesResponse,
} from "@kizunashelf/api-contract";

import type { SaveSettingsRequest, SettingsConfigResponse } from "@kizunashelf/api-contract";

// Editor config types, derived by indexed access into the generated request type
// so they are structurally identical to what the save endpoint accepts (avoids
// the orval inline-vs-named TS2719 clash). Single source of truth = the contract.
export type VaultConfig = NonNullable<SaveSettingsRequest["vault"]>;
export type AppConfig = NonNullable<SettingsConfigResponse["app"]>;
export type EntityTypeConfig = VaultConfig["types"][number];
export type FieldConfig = EntityTypeConfig["fields"][number];
export type FieldType = FieldConfig["fieldType"];
export type FilenameConfig = NonNullable<EntityTypeConfig["filename"]>;
export type ExternalFieldMapping = NonNullable<FieldConfig["externalFields"]>[number];
export type ExternalBodyMapping = NonNullable<EntityTypeConfig["bodyMappings"]>[number];
export type DateRole = NonNullable<FieldConfig["dateRole"]>;
export type SeasonLanguage = NonNullable<FieldConfig["seasonLanguage"]>;
export type TitleRole = NonNullable<FieldConfig["titleRole"]>;
export type HomeConfig = NonNullable<VaultConfig["home"]>;
export type HomeSectionConfig = NonNullable<HomeConfig["sections"]>[number];
export type HomeSectionFilterConfig = NonNullable<HomeSectionConfig["filters"]>[number];
export type DailyNotesConfig = NonNullable<VaultConfig["dailyNotes"]>;
