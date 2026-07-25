import {
  getActivity,
  getAnalytics,
  getCalendar,
  getCapabilities,
  getCleanupQueues,
  getConfig,
  getEntities,
  getEntity,
  getEntityDates,
  getHome,
  getHealth,
  getStats,
  getTags,
  getUpcoming,
  type GetActivityParams,
  type GetCalendarParams,
  type GetEntitiesParams,
  type GetSmartListResultsParams,
  type GetStatsParams,
  type GetUpcomingParams,
  type SmartListPreviewRequest,
} from "@kizunashelf/api-contract";
import { infiniteQueryOptions, keepPreviousData, queryOptions } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import { todayLocal } from "@/lib/date";
import { getProviderCatalog } from "@/api/external";
import { fetchList, fetchLists } from "@/api/lists";
import { fetchSmartList, fetchSmartListPreview, fetchSmartListResults } from "@/api/smart-lists";
import { getLanguages, getRawSettingsConfig, getSettingsConfig, getTypePresets } from "@/api/settings";

export const queryKeys = {
  activity: (params: Omit<GetActivityParams, "cursor">) => ["activity", params] as const,
  upcoming: (params: GetUpcomingParams) => ["upcoming", params] as const,
  analytics: ["analytics"] as const,
  calendar: (params: GetCalendarParams) => ["calendar", params] as const,
  capabilities: ["capabilities"] as const,
  cleanupQueues: (today: string) => ["cleanupQueues", today] as const,
  config: ["config"] as const,
  entities: (params: GetEntitiesParams) => ["entities", params] as const,
  entity: (id: string) => ["entity", id] as const,
  entityDates: (id: string) => ["entityDates", id] as const,
  home: ["home"] as const,
  health: ["health"] as const,
  languages: ["languages"] as const,
  lists: ["lists"] as const,
  list: (id: string) => ["list", id] as const,
  smartList: (id: string) => ["smartList", id] as const,
  smartListResults: (id: string, params: GetSmartListResultsParams) =>
    ["smartListResults", id, params] as const,
  smartListPreview: (request: SmartListPreviewRequest) =>
    ["smartListPreview", request] as const,
  providerCatalog: ["providerCatalog"] as const,
  settingsConfig: ["settingsConfig"] as const,
  rawSettingsConfig: ["rawSettingsConfig"] as const,
  stats: (params?: GetStatsParams) => ["stats", params ?? {}] as const,
  tags: ["tags"] as const,
  typePresets: (language?: string) => ["typePresets", language ?? "en"] as const,
};

// The vault's whole tag vocabulary, cached client-side (it changes rarely and is
// read by the tag editor combobox + the Library tag filter). The server memoizes
// it per content revision; a long staleTime avoids refetching on every mount.
export function allTagsQuery() {
  return queryOptions({
    queryKey: queryKeys.tags,
    queryFn: ({ signal }) => getTags({ signal }, apiFetch),
    staleTime: 5 * 60 * 1000,
  });
}

export function analyticsQuery() {
  return queryOptions({
    queryKey: queryKeys.analytics,
    queryFn: ({ signal }) => getAnalytics({ signal }, apiFetch),
  });
}

// The reverse-chronological activity feed, paged by item count: each page gathers
// whole months until it holds ~`limit` items (so sparse months don't each cost a
// request), and `cursor` (a `YYYY-MM`) drives the following page.
export function activityFeedQuery(params: Omit<GetActivityParams, "cursor">) {
  return infiniteQueryOptions({
    queryKey: queryKeys.activity(params),
    queryFn: ({ pageParam, signal }) =>
      getActivity({ ...params, cursor: pageParam ?? undefined }, { signal }, apiFetch),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.cursor ?? undefined,
  });
}

export function upcomingQuery(params: GetUpcomingParams) {
  return queryOptions({
    queryKey: queryKeys.upcoming(params),
    queryFn: ({ signal }) => getUpcoming(params, { signal }, apiFetch),
  });
}

export function calendarQuery(params: GetCalendarParams) {
  return queryOptions({
    queryKey: queryKeys.calendar(params),
    queryFn: ({ signal }) => getCalendar(params, { signal }, apiFetch),
    // Keep the previous month on screen while the next one loads.
    placeholderData: keepPreviousData,
  });
}

export function capabilitiesQuery() {
  return queryOptions({
    queryKey: queryKeys.capabilities,
    queryFn: ({ signal }) => getCapabilities({ signal }, apiFetch),
  });
}

export function cleanupQueuesQuery() {
  // The status-mismatch queue is date-relative, so scope it to the client's local
  // date (and key the cache by it, so it refetches when the day rolls over).
  const today = todayLocal();
  return queryOptions({
    queryKey: queryKeys.cleanupQueues(today),
    queryFn: ({ signal }) => getCleanupQueues({ today }, { signal }, apiFetch),
  });
}

export function configQuery() {
  return queryOptions({
    queryKey: queryKeys.config,
    queryFn: ({ signal }) => getConfig({ signal }, apiFetch),
  });
}

export function entitiesQuery(params: GetEntitiesParams) {
  return queryOptions({
    queryKey: queryKeys.entities(params),
    queryFn: ({ signal }) => getEntities(params, { signal }, apiFetch),
    // Hold the current page/sort/filter results visible while the next set
    // loads, so the grid never blanks between param changes.
    placeholderData: keepPreviousData,
  });
}

export function entityQuery(id: string) {
  return queryOptions({
    queryKey: queryKeys.entity(id),
    queryFn: ({ signal }) => getEntity(id, { signal }, apiFetch),
  });
}

export function entityDatesQuery(id: string) {
  return queryOptions({
    queryKey: queryKeys.entityDates(id),
    queryFn: ({ signal }) => getEntityDates(id, { signal }, apiFetch),
  });
}

export function listsQuery() {
  // The client's local date drives smart-list `today()` criteria in itemCount/
  // contains (and keys the cache, so it refetches when the day rolls over).
  const today = todayLocal();
  return queryOptions({
    queryKey: [...queryKeys.lists, { today }] as const,
    queryFn: ({ signal }) => fetchLists({ today }, { signal }),
  });
}

// Lists annotated with `contains` membership for one entity — backs the entity
// page's "manage lists". Shares the `["lists"]` key prefix so a single
// invalidation refreshes both this and the plain index.
export function entityListsQuery(entityId: string) {
  const today = todayLocal();
  return queryOptions({
    queryKey: [...queryKeys.lists, { entity: entityId, today }] as const,
    queryFn: ({ signal }) => fetchLists({ entity: entityId, today }, { signal }),
  });
}

export function listQuery(id: string) {
  return queryOptions({
    queryKey: queryKeys.list(id),
    queryFn: ({ signal }) => fetchList(id, { signal }),
  });
}

export function smartListQuery(id: string) {
  return queryOptions({
    queryKey: queryKeys.smartList(id),
    queryFn: ({ signal }) => fetchSmartList(id, { signal }),
  });
}

export function smartListResultsQuery(id: string, params: GetSmartListResultsParams) {
  // `today()` criteria in the list's filters resolve against the client's local
  // date; folding it into params also keys the cache for day-rollover refetch.
  const merged = { ...params, today: todayLocal() };
  return queryOptions({
    queryKey: queryKeys.smartListResults(id, merged),
    queryFn: ({ signal }) => fetchSmartListResults(id, merged, { signal }),
    // Hold the current results visible while a view/page change loads, like
    // the library page.
    placeholderData: keepPreviousData,
  });
}

/// Results of an *unsaved* smart-list definition: the smart-list editor's live
/// preview, and the library browser (which is nothing but an unsaved smart
/// list). `today()` criteria resolve against the client's local date, folded
/// into the request so it also keys the cache for day-rollover refetch.
export function smartListPreviewQuery(request: SmartListPreviewRequest) {
  const merged = { ...request, today: todayLocal() };
  return queryOptions({
    queryKey: queryKeys.smartListPreview(merged),
    queryFn: ({ signal }) => fetchSmartListPreview(merged, { signal }),
    // Hold the current results visible while a page/criteria change loads.
    placeholderData: keepPreviousData,
  });
}

export function homeQuery() {
  // Date-relative home sections (`today() - "30d"`) resolve against the client's
  // local date, keyed so the page refetches when the day rolls over.
  const today = todayLocal();
  return queryOptions({
    queryKey: [...queryKeys.home, today] as const,
    queryFn: ({ signal }) => getHome({ today }, { signal }, apiFetch),
  });
}

export function healthQuery() {
  return queryOptions({
    queryKey: queryKeys.health,
    queryFn: ({ signal }) => getHealth({ signal }, apiFetch),
  });
}

export function providerCatalogQuery() {
  return queryOptions({
    queryKey: queryKeys.providerCatalog,
    queryFn: ({ signal }) => getProviderCatalog({ signal }),
  });
}

export function settingsConfigQuery() {
  return queryOptions({
    queryKey: queryKeys.settingsConfig,
    queryFn: ({ signal }) => getSettingsConfig({ signal }),
  });
}

export function rawSettingsConfigQuery() {
  return queryOptions({
    queryKey: queryKeys.rawSettingsConfig,
    queryFn: ({ signal }) => getRawSettingsConfig({ signal }),
  });
}

export function statsQuery(params?: GetStatsParams) {
  return queryOptions({
    queryKey: queryKeys.stats(params),
    queryFn: ({ signal }) => getStats(params, { signal }, apiFetch),
  });
}

// `language` is the raw user preference (it may carry a script subtag) — the
// server localizes the preset display text for it, English fallback.
export function typePresetsQuery(language?: string) {
  return queryOptions({
    queryKey: queryKeys.typePresets(language),
    queryFn: ({ signal }) => getTypePresets(language ? { language } : undefined, { signal }),
  });
}

export function languagesQuery() {
  return queryOptions({
    queryKey: queryKeys.languages,
    queryFn: ({ signal }) => getLanguages({ signal }),
  });
}
