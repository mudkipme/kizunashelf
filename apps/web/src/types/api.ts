export type {
  ActivityEntry,
  ActivityEpisodeRef,
  ActivityItem,
  ActivityResponse,
  AnalyticsActivity,
  AnalyticsActivityType,
  AnalyticsActivityYear,
  AnalyticsResponse,
  AssetDownloadItemResult,
  AssetDownloadJob,
  AssetDownloadJobListResponse,
  AssetDownloadRequest,
  AssetDownloadResponse,
  AssetUploadRequest,
  AssetUploadResponse,
  CalendarDay,
  CalendarEntry,
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
  ExistingEntityRef,
  ExternalCandidate,
  ExternalMatch,
  ExternalProviderCatalog,
  ExternalProviderCatalogItem,
  ExternalProviderSummary,
  ExternalSearchResponse,
  MappedBodySection,
  MappedFieldValue,
  QuickAddEpisodeResult,
  QuickAddRequest,
  QuickAddResponse,
  HomeResponse,
  HomeSectionResponse,
  EpisodeSource,
  EpisodeSyncResponse,
  Language,
  LanguagesResponse,
  ListDetail,
  ProviderEpisodeGroup,
  ProviderEpisodeItem,
  ListItem,
  ListItemInput,
  ListMarker,
  ListSection,
  ListsResponse,
  ListSummary,
  LogActivityRequest,
  LogActivityResponse,
  PathSuggestionsResponse,
  Relation,
  SaveSettingsRequest,
  SettingsConfigResponse,
  StatsResponse,
  TagsResponse,
  TypeConfig,
  VaultTemplate,
  VaultTemplatesResponse,
} from "@kizunashelf/api-contract";

import type {
  EntityDetailResponse,
  EntitySummary,
  SaveSettingsRequest,
  SettingsConfigResponse,
} from "@kizunashelf/api-contract";

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
export type BodySection = NonNullable<EntityTypeConfig["bodySections"]>[number];
export type BodySectionKind = BodySection["kind"];
export type EntityEpisodes = NonNullable<EntityDetailResponse["episodes"]>;
export type EpisodeGroup = EntityEpisodes["groups"][number];
export type Episode = EpisodeGroup["items"][number];
export type EpisodeTracking = EntityEpisodes["tracking"];
export type EpisodeProgress = NonNullable<EntitySummary["episodeProgress"]>;
export type ResolvedStatus = NonNullable<EntitySummary["status"]>;
export type CanonicalStatus = NonNullable<ResolvedStatus["canonical"]>;
export type EnumRole = NonNullable<FieldConfig["enumRole"]>;
export type StatusValues = NonNullable<FieldConfig["statusValues"]>;
export type DateRole = NonNullable<FieldConfig["dateRole"]>;
export type SeasonLanguage = NonNullable<FieldConfig["seasonLanguage"]>;
export type TitleRole = NonNullable<FieldConfig["titleRole"]>;
export type HomeConfig = NonNullable<VaultConfig["home"]>;
export type HomeSectionConfig = NonNullable<HomeConfig["sections"]>[number];
export type HomeSectionFilterConfig = NonNullable<HomeSectionConfig["filters"]>[number];
export type DailyNotesConfig = NonNullable<VaultConfig["dailyNotes"]>;
