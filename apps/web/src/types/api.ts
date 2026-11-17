export type {
  ActivityEntry,
  ActivityItem,
  AnalyticsActivity,
  AssetDownloadJob,
  AssetDownloadRequest,
  AssetUploadRequest,
  CalendarDay,
  Capabilities,
  CleanupQueueSummary,
  CleanupQueuesResponse,
  CleanupUnresolvedRelation,
  ConfigResponse,
  Entity,
  EntityDatesResponse,
  EntityDetailResponse,
  EntitySummary,
  ExternalMatch,
  ExternalProviderCatalog,
  ExternalProviderCatalogItem,
  ExternalProviderSummary,
  ExternalReviewResponse,
  ExternalSearchResponse,
  ImportSourceCatalogItem,
  ImportJob,
  ImportJobStatus,
  CreateImportJobRequest,
  CommitImportJobRequest,
  MappedBodySection,
  MappedFieldValue,
  QuickAddRequest,
  HealthResponse,
  HomeListResponse,
  Language,
  LanguagesResponse,
  ListItem,
  ListMarker,
  ListSection,
  LogActivityRequest,
  ProviderEpisodeGroup,
  ProviderEpisodeItem,
  Relation,
  SaveSettingsRequest,
  SettingsConfigResponse,
  SmartFilterConjunction,
  SmartFilterGroup,
  SmartFilterRule,
  SmartFilterSubgroup,
  SmartListDetail,
  SmartListPreviewRequest,
  SmartListView,
  SmartSortSpec,
  SmartViewLayout,
  CreateSmartListRequest,
  UpdateSmartListRequest,
  StatsResponse,
  TypeConfig,
  TypePresetsResponse,
  ResolveTypePresetsRequest,
  ResolveTypePresetsResponse,
} from "@kizunashelf/api-contract";

import type {
  EntityDetailResponse,
  EntitySummary,
  ImportJob,
  LanguagesResponse,
  LogActivityRequest,
  ResolveTypePresetsResponse,
  SaveSettingsRequest,
  SettingsConfigResponse,
  TypePresetsResponse,
} from "@kizunashelf/api-contract";

// Import plan sub-types, derived by indexed access into ImportJob so they unify
// with the poll response's inlined `plan` shape (orval inlines nested objects, so
// importing the standalone named ImportPlanItem/… would risk a TS2719 clash).
export type ImportPlan = NonNullable<ImportJob["plan"]>;
export type ImportPlanBucket = ImportPlan["buckets"][number];
export type ImportPlanItem = ImportPlan["items"][number];
export type ImportPlanUserData = ImportPlanItem["userData"];
export type ImportCanonicalStatus = NonNullable<ImportPlanUserData["status"]>;

// Derived by indexed access into the generated response so it unifies
// structurally with it (see the api-contract notes in ARCHITECTURE.md).
export type UserLanguage = LanguagesResponse["userLanguages"][number];

// Preset picker types, derived by indexed access so the resolve response's
// `types`/`field` unify structurally with the editor's own EntityTypeConfig/
// FieldConfig (both are orval-inlined anonymous shapes, so no TS2719 clash).
export type TypePresetSummary = TypePresetsResponse["presets"][number];
export type TypePresetCategoryInfo = TypePresetsResponse["categories"][number];
export type TypePresetBackfill = NonNullable<ResolveTypePresetsResponse["backfills"]>[number];
export type TypePresetCollision = NonNullable<ResolveTypePresetsResponse["collisions"]>[number];

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
export type ResolvedStatus = NonNullable<EntitySummary["status"]>;
export type CanonicalStatus = NonNullable<ResolvedStatus["canonical"]>;
export type StatusValues = NonNullable<FieldConfig["statusValues"]>;
export type DateRole = NonNullable<FieldConfig["dateRole"]>;
export type SeasonLanguage = NonNullable<FieldConfig["seasonLanguage"]>;
export type DailyNotesConfig = NonNullable<VaultConfig["dailyNotes"]>;

export type LogKind = NonNullable<LogActivityRequest["kind"]>;
