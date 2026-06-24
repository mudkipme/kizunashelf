import {
  getAnalytics,
  getCalendar,
  getCalendarPlanning,
  getCapabilities,
  getCleanupQueues,
  getConfig,
  getEntities,
  getEntity,
  getEntityDates,
  getHome,
  getRelationGroups,
  getStats,
  type GetCalendarParams,
  type GetCalendarPlanningParams,
  type GetEntitiesParams,
  type GetStatsParams,
} from "@kizunashelf/api-contract";
import { keepPreviousData, queryOptions } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import { getProviderCatalog } from "@/api/external";
import { fetchList, fetchLists } from "@/api/lists";
import { getLanguages, getRawSettingsConfig, getSettingsConfig, getVaultTemplates } from "@/api/settings";

export const queryKeys = {
  analytics: ["analytics"] as const,
  calendar: (params: GetCalendarParams) => ["calendar", params] as const,
  calendarPlanning: (params: GetCalendarPlanningParams) => ["calendarPlanning", params] as const,
  capabilities: ["capabilities"] as const,
  cleanupQueues: ["cleanupQueues"] as const,
  config: ["config"] as const,
  entities: (params: GetEntitiesParams) => ["entities", params] as const,
  entity: (id: string) => ["entity", id] as const,
  entityDates: (id: string) => ["entityDates", id] as const,
  home: ["home"] as const,
  languages: ["languages"] as const,
  lists: ["lists"] as const,
  list: (id: string) => ["list", id] as const,
  providerCatalog: ["providerCatalog"] as const,
  relationGroups: ["relationGroups"] as const,
  settingsConfig: ["settingsConfig"] as const,
  rawSettingsConfig: ["rawSettingsConfig"] as const,
  stats: (params?: GetStatsParams) => ["stats", params ?? {}] as const,
  vaultTemplates: ["vaultTemplates"] as const,
};

export function analyticsQuery() {
  return queryOptions({
    queryKey: queryKeys.analytics,
    queryFn: ({ signal }) => getAnalytics({ signal }, apiFetch),
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

export function calendarPlanningQuery(params: GetCalendarPlanningParams) {
  return queryOptions({
    queryKey: queryKeys.calendarPlanning(params),
    queryFn: ({ signal }) => getCalendarPlanning(params, { signal }, apiFetch),
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
  return queryOptions({
    queryKey: queryKeys.cleanupQueues,
    queryFn: ({ signal }) => getCleanupQueues({ signal }, apiFetch),
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
  return queryOptions({
    queryKey: queryKeys.lists,
    queryFn: ({ signal }) => fetchLists(undefined, { signal }),
  });
}

// Lists annotated with `contains` membership for one entity — backs the entity
// page's "manage lists". Shares the `["lists"]` key prefix so a single
// invalidation refreshes both this and the plain index.
export function entityListsQuery(entityId: string) {
  return queryOptions({
    queryKey: [...queryKeys.lists, { entity: entityId }] as const,
    queryFn: ({ signal }) => fetchLists({ entity: entityId }, { signal }),
  });
}

export function listQuery(id: string) {
  return queryOptions({
    queryKey: queryKeys.list(id),
    queryFn: ({ signal }) => fetchList(id, { signal }),
  });
}

export function homeQuery() {
  return queryOptions({
    queryKey: queryKeys.home,
    queryFn: ({ signal }) => getHome({ signal }, apiFetch),
  });
}

export function providerCatalogQuery() {
  return queryOptions({
    queryKey: queryKeys.providerCatalog,
    queryFn: ({ signal }) => getProviderCatalog({ signal }),
  });
}

export function relationGroupsQuery() {
  return queryOptions({
    queryKey: queryKeys.relationGroups,
    queryFn: ({ signal }) => getRelationGroups({ signal }, apiFetch),
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

export function vaultTemplatesQuery() {
  return queryOptions({
    queryKey: queryKeys.vaultTemplates,
    queryFn: ({ signal }) => getVaultTemplates({ signal }),
  });
}

export function languagesQuery() {
  return queryOptions({
    queryKey: queryKeys.languages,
    queryFn: ({ signal }) => getLanguages({ signal }),
  });
}
