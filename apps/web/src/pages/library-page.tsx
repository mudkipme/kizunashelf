import { useEffect, useMemo, useState } from "react";
import { getConfig, getEntities, getStats } from "@kizunashelf/api-contract";
import { PlusIcon, SlidersHorizontalIcon } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { getAppCapabilities } from "@/api/entities";
import { AssetToolbar } from "@/components/assets/asset-toolbar";
import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import {
  allOptions,
  allTypes,
  defaultCategory,
  defaultDirection,
  defaultSort,
  defaultTitleOptionId,
  defaultView,
  pageSize,
} from "@/lib/constants";
import {
  defaultTitleOption,
  fieldLabelAcrossTypes,
  fieldLabelsByType,
  hasAnyFieldType,
  titleLanguageOptions,
} from "@/lib/type-config";
import { titleLanguageLabel } from "@/lib/title-language";
import {
  applyPreferencesToSearchParams,
  preferencesFromSearchParams,
  readAssetListPreferences,
  writeAssetListPreferences,
} from "@/lib/asset-list-preferences";
import type { Capabilities, ConfigResponse, EntitySummary, StatsResponse } from "@/types/api";

type StatsState = {
  global?: StatsResponse;
  category?: StatsResponse;
  loading: boolean;
  error?: string;
};

type ListState = {
  entities: EntitySummary[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
  loading: boolean;
  error?: string;
};

export function LibraryPage() {
  const [stats, setStats] = useState<StatsState>({ loading: true });
  const [config, setConfig] = useState<ConfigResponse>();
  const [capabilities, setCapabilities] = useState<Capabilities>();
  const [list, setList] = useState<ListState>({
    entities: [],
    total: 0,
    page: 1,
    pageSize,
    totalPages: 1,
    loading: true,
  });
  const [searchParams, setSearchParams] = useSearchParams();
  const firstType = stats.global?.byType[0]?.id ?? defaultCategory;
  const selectedType = searchParams.get("type") ?? allTypes;
  const isGlobalType = selectedType === allTypes;
  const refs = searchParams.get("refs") ?? allOptions;
  const cover = searchParams.get("cover") ?? allOptions;
  const sort = searchParams.get("sort") ?? defaultSort;
  const direction = searchParams.get("direction") === "desc" ? "desc" : defaultDirection;
  const view = searchParams.get("view") === "grid" ? "grid" : defaultView;
  const titleLanguage = searchParams.get("titleLanguage") ?? defaultTitleOptionId;
  const query = searchParams.get("q") ?? "";
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [mobileFiltersOpen, setMobileFiltersOpen] = useState(false);
  const selectedTypeStats = stats.global?.byType.find((type) => type.id === selectedType);
  const scopeStats = isGlobalType ? stats.global : stats.category;
  const selectedTypeConfig = config?.types.find((type) => type.id === selectedType);
  const scopeTypeConfigs = isGlobalType
    ? (config?.types ?? [])
    : selectedTypeConfig
      ? [selectedTypeConfig]
      : [];
  const fieldLabels = useMemo(() => fieldLabelsByType(config?.types), [config]);
  const supportsRefsFilter =
    !config || scopeTypeConfigs.some((typeConfig) => hasAnyFieldType(typeConfig, ["externalRef"]));
  const supportsCoverFilter =
    !config ||
    scopeTypeConfigs.some((typeConfig) => hasAnyFieldType(typeConfig, ["image", "imageList"]));
  const effectiveRefs = supportsRefsFilter ? refs : allOptions;
  const effectiveCover = supportsCoverFilter ? cover : allOptions;
  const titleLanguages = titleLanguageOptions(selectedTypeConfig);
  const defaultTitle = defaultTitleOption(selectedTypeConfig);
  const defaultTitleLabel = defaultTitle
    ? `Default title (${titleLanguageLabel(defaultTitle)})`
    : "Default title";
  const effectiveTitleLanguage = titleLanguages.includes(titleLanguage)
    ? titleLanguage
    : defaultTitleOptionId;
  const effectiveSort =
    scopeStats &&
    sort.startsWith("date:") &&
    !scopeStats.dateFields.includes(sort.slice("date:".length))
      ? defaultSort
      : sort;
  const entryCount = isGlobalType
    ? (stats.global?.total ?? list.total)
    : (selectedTypeStats?.count ?? list.total);
  const filtersActive =
    effectiveRefs !== allOptions ||
    effectiveCover !== allOptions ||
    effectiveSort !== defaultSort ||
    direction !== defaultDirection ||
    view !== defaultView ||
    effectiveTitleLanguage !== defaultTitleOptionId;

  useEffect(() => {
    const controller = new AbortController();
    void loadGlobalStats(controller.signal);
    void loadConfig(controller.signal);
    void loadCapabilities(controller.signal);
    return () => controller.abort();
  }, []);

  useEffect(() => {
    if (!stats.global) return;
    const validTypes = new Set(stats.global.byType.map((type) => type.id));
    const hasType = searchParams.has("type");
    const hasGlobalQuery = selectedType === allTypes && query.trim().length > 0;

    if (selectedType !== allTypes && !validTypes.has(selectedType)) {
      const next = new URLSearchParams(searchParams);
      next.set("type", firstType);
      next.set("page", "1");
      applyPreferencesToSearchParams(next, readAssetListPreferences(firstType));
      setSearchParams(next, { replace: true });
      return;
    }

    if (!hasType || !hasPreferenceParams(searchParams)) {
      const next = new URLSearchParams(searchParams);
      next.set("type", selectedType);
      if (!hasGlobalQuery) {
        applyPreferencesToSearchParams(next, readAssetListPreferences(selectedType));
      }
      if (next.toString() !== searchParams.toString()) {
        setSearchParams(next, { replace: true });
      }
    }
  }, [stats.global, selectedType, query]);

  useEffect(() => {
    if (!selectedType || isGlobalType) {
      setStats((current) => ({ ...current, category: undefined }));
      return;
    }
    const controller = new AbortController();
    void loadCategoryStats(selectedType, controller.signal);
    return () => controller.abort();
  }, [selectedType, isGlobalType]);

  useEffect(() => {
    if (!scopeStats || !sort.startsWith("date:")) return;
    if (scopeStats.dateFields.includes(sort.slice("date:".length))) return;
    setQueryParam("sort", defaultSort, defaultSort);
  }, [scopeStats, sort]);

  useEffect(() => {
    if (!config || supportsRefsFilter || refs === allOptions) return;
    setQueryParam("refs", allOptions);
  }, [config, supportsRefsFilter, refs]);

  useEffect(() => {
    if (!config || supportsCoverFilter || cover === allOptions) return;
    setQueryParam("cover", allOptions);
  }, [config, supportsCoverFilter, cover]);

  useEffect(() => {
    if (!config || titleLanguage === defaultTitleOptionId) return;
    if (titleLanguages.includes(titleLanguage)) return;
    setQueryParam("titleLanguage", defaultTitleOptionId, defaultTitleOptionId, false);
  }, [config, titleLanguages, titleLanguage]);

  useEffect(() => {
    if (!stats.global || !selectedType || !searchParams.has("type")) return;
    if ((!supportsRefsFilter && refs !== allOptions) || (!supportsCoverFilter && cover !== allOptions)) {
      return;
    }
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(searchParams));
  }, [
    stats.global,
    selectedType,
    searchParams,
    supportsRefsFilter,
    supportsCoverFilter,
    refs,
    cover,
    sort,
    direction,
    view,
    titleLanguage,
  ]);

  useEffect(() => {
    const controller = new AbortController();
    void loadEntities({
      type: selectedType,
      refs: effectiveRefs,
      cover: effectiveCover,
      sort: effectiveSort,
      direction,
      titleLanguage: effectiveTitleLanguage,
      q: query,
      page,
    }, controller.signal);
    return () => controller.abort();
  }, [selectedType, effectiveRefs, effectiveCover, effectiveSort, direction, effectiveTitleLanguage, query, page]);

  async function loadGlobalStats(signal: AbortSignal) {
    setStats((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const global = await getStats(undefined, { signal }, apiFetch);
      setStats((current) => ({ ...current, global, loading: false }));
    } catch (error) {
      if (isAbortError(error)) return;
      setStats((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  async function loadCategoryStats(type: string, signal: AbortSignal) {
    try {
      const category = await getStats({ type }, { signal }, apiFetch);
      setStats((current) => ({ ...current, category, error: undefined }));
    } catch (error) {
      if (isAbortError(error)) return;
      setStats((current) => ({ ...current, error: errorMessage(error) }));
    }
  }

  async function loadConfig(signal: AbortSignal) {
    try {
      const config = await getConfig({ signal }, apiFetch);
      setConfig(config);
    } catch (error) {
      if (isAbortError(error)) return;
      setStats((current) => ({ ...current, error: errorMessage(error) }));
    }
  }

  async function loadCapabilities(signal: AbortSignal) {
    try {
      setCapabilities(await getAppCapabilities({ signal }));
    } catch (error) {
      if (isAbortError(error)) return;
      setStats((current) => ({ ...current, error: errorMessage(error) }));
    }
  }

  async function loadEntities(filters: {
    type: string;
    refs: string;
    cover: string;
    sort: string;
    direction: string;
    titleLanguage: string;
    q: string;
    page: number;
  }, signal: AbortSignal) {
    setList((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const result = await getEntities(
        {
          type: filters.type,
          page: filters.page,
          pageSize,
          sort: filters.sort,
          direction: filters.direction,
          titleLanguage: filters.titleLanguage,
          ...(filters.refs !== allOptions ? { refs: filters.refs } : {}),
          ...(filters.cover !== allOptions ? { cover: filters.cover } : {}),
          ...(filters.q.trim() ? { q: filters.q.trim() } : {}),
        },
        { signal },
        apiFetch,
      );
      setList({ ...result, entities: result.items, loading: false });
      if (result.page !== filters.page) {
        const next = new URLSearchParams(searchParams);
        next.set("page", String(result.page));
        setSearchParams(next, { replace: true });
      }
    } catch (error) {
      if (isAbortError(error)) return;
      setList((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  function selectType(type: string) {
    const next = new URLSearchParams(searchParams);
    next.set("type", type);
    applyPreferencesToSearchParams(next, readAssetListPreferences(type));
    next.set("page", "1");
    setSearchParams(next, { replace: true });
  }

  function goToPage(nextPage: number) {
    const next = new URLSearchParams(searchParams);
    next.set("page", String(nextPage));
    setSearchParams(next);
  }

  function setQueryParam(key: string, value: string, defaultValue = allOptions, resetPage = true) {
    const next = new URLSearchParams(searchParams);
    if (value === defaultValue) next.delete(key);
    else next.set(key, value);
    if (resetPage) next.set("page", "1");
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(next));
    setSearchParams(next, { replace: true });
  }

  return (
    <AppFrame error={stats.error ?? list.error}>
      <div className="h-full min-h-full overflow-hidden">
        <section className="h-full min-w-0">
          <div className="flex h-full flex-col">
            <div className="border-b px-3 py-2 md:hidden">
              <div className="flex items-center gap-2">
                <Select
                  value={selectedType}
                  onChange={(event) => selectType(event.target.value)}
                  className="min-w-0 flex-1"
                  aria-label="Type"
                >
                  <option value={allTypes}>All types ({stats.global?.total ?? 0})</option>
                  {stats.global?.byType.map((type) => (
                    <option key={type.id} value={type.id}>
                      {type.label} ({type.count})
                    </option>
                  ))}
                </Select>
                <Button
                  type="button"
                  variant={mobileFiltersOpen || filtersActive ? "secondary" : "outline"}
                  size="icon"
                  className="shrink-0"
                  onClick={() => setMobileFiltersOpen((open) => !open)}
                  aria-label="Toggle filters"
                  aria-expanded={mobileFiltersOpen}
                >
                  <SlidersHorizontalIcon />
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  className="shrink-0"
                  disabled={capabilities?.contentWritable === false}
                  aria-label="Add entity"
                  title={
                    capabilities?.contentWritable === false
                      ? "Content writes are disabled"
                      : "Add entity"
                  }
                  asChild={capabilities?.contentWritable !== false}
                >
                  {capabilities?.contentWritable === false ? (
                    <span>
                      <PlusIcon />
                    </span>
                  ) : (
                    <Link to="/entities/new">
                      <PlusIcon />
                    </Link>
                  )}
                </Button>
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                <span>{entryCount.toLocaleString()} entries</span>
                <span>{(scopeStats?.relations ?? 0).toLocaleString()} links</span>
                <span>{isGlobalType ? "All types" : (selectedTypeStats?.label ?? selectedType)}</span>
              </div>
              {mobileFiltersOpen ? (
                <AssetToolbar
                  compact
                  showLabel={false}
                  className="mt-2"
                  stats={scopeStats}
                  showRefsFilter={supportsRefsFilter}
                  showCoverFilter={supportsCoverFilter}
                  refs={effectiveRefs}
                  cover={effectiveCover}
                  sort={effectiveSort}
                  direction={direction}
                  view={view}
                  titleLanguage={effectiveTitleLanguage}
                  titleLanguages={titleLanguages}
                  defaultTitleLabel={defaultTitleLabel}
                  dateFieldLabel={(field) => fieldLabelAcrossTypes(scopeTypeConfigs, field)}
                  onRefsChange={(value) => setQueryParam("refs", value)}
                  onCoverChange={(value) => setQueryParam("cover", value)}
                  onSortChange={(value) => setQueryParam("sort", value, defaultSort)}
                  onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
                  onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
                  onTitleLanguageChange={(value) =>
                    setQueryParam("titleLanguage", value, defaultTitleOptionId, false)
                  }
                />
              ) : null}
            </div>

            <AssetToolbar
              className="hidden md:flex"
              stats={scopeStats}
              showRefsFilter={supportsRefsFilter}
              showCoverFilter={supportsCoverFilter}
              refs={effectiveRefs}
              cover={effectiveCover}
              sort={effectiveSort}
              direction={direction}
              view={view}
              titleLanguage={effectiveTitleLanguage}
              titleLanguages={titleLanguages}
              defaultTitleLabel={defaultTitleLabel}
              dateFieldLabel={(field) => fieldLabelAcrossTypes(scopeTypeConfigs, field)}
              onRefsChange={(value) => setQueryParam("refs", value)}
              onCoverChange={(value) => setQueryParam("cover", value)}
              onSortChange={(value) => setQueryParam("sort", value, defaultSort)}
              onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
              onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
              onTitleLanguageChange={(value) =>
                setQueryParam("titleLanguage", value, defaultTitleOptionId, false)
              }
            />

            <div className="flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2 text-xs text-muted-foreground">
              <span>
                {list.total} entries
                {list.total > 0 ? ` · page ${list.page}/${list.totalPages}` : ""}
              </span>
              <div className="flex items-center gap-2">
                <span>
                  {list.loading || stats.loading ? "Loading" : stats.global?.generatedAt.slice(0, 10)}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={capabilities?.contentWritable === false}
                  title={
                    capabilities?.contentWritable === false
                      ? "Content writes are disabled"
                      : "Add entity"
                  }
                  asChild={capabilities?.contentWritable !== false}
                >
                  {capabilities?.contentWritable === false ? (
                    <span>
                      <PlusIcon data-icon="inline-start" />
                      Add
                    </span>
                  ) : (
                    <Link to="/entities/new">
                      <PlusIcon data-icon="inline-start" />
                      Add
                    </Link>
                  )}
                </Button>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-auto">
              {view === "grid" ? (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 p-3">
                  {list.entities.map((entity) => (
                    <EntityGridItem
                      key={entity.id}
                      entity={entity}
                      titleLanguage={effectiveTitleLanguage}
                      labelsByType={fieldLabels}
                    />
                  ))}
                </div>
              ) : (
                list.entities.map((entity) => (
                  <EntityListItem
                    key={entity.id}
                    entity={entity}
                    titleLanguage={effectiveTitleLanguage}
                    labelsByType={fieldLabels}
                  />
                ))
              )}
              {!list.loading && list.entities.length === 0 ? (
                <div className="p-8 text-center text-sm text-muted-foreground">No entries</div>
              ) : null}
            </div>
            <PaginationBar
              page={list.page}
              totalPages={list.totalPages}
              total={list.total}
              pageSize={list.pageSize}
              onPageChange={goToPage}
            />
          </div>
        </section>
      </div>
    </AppFrame>
  );
}

function hasPreferenceParams(params: URLSearchParams) {
  return ["refs", "cover", "sort", "direction", "view", "titleLanguage"].some((key) =>
    params.has(key),
  );
}
