import { useEffect, useState } from "react";
import { getEntities, getStats } from "@kizunashelf/api-contract";
import { SearchIcon, SlidersHorizontalIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { AssetToolbar } from "@/components/assets/asset-toolbar";
import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  allOptions,
  allStatuses,
  defaultCategory,
  defaultDirection,
  defaultSort,
  defaultView,
  pageSize,
} from "@/lib/constants";
import {
  applyPreferencesToSearchParams,
  preferencesFromSearchParams,
  readAssetListPreferences,
  writeAssetListPreferences,
} from "@/lib/asset-list-preferences";
import type { EntitySummary, StatsResponse } from "@/types/api";

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
  const selectedType = searchParams.get("type") ?? firstType;
  const selectedStatus = searchParams.get("status") ?? allStatuses;
  const refs = searchParams.get("refs") ?? allOptions;
  const cover = searchParams.get("cover") ?? allOptions;
  const sort = searchParams.get("sort") ?? defaultSort;
  const direction = searchParams.get("direction") === "desc" ? "desc" : defaultDirection;
  const view = searchParams.get("view") === "grid" ? "grid" : defaultView;
  const query = searchParams.get("q") ?? "";
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [queryInput, setQueryInput] = useState(query);
  const [mobileFiltersOpen, setMobileFiltersOpen] = useState(false);
  const selectedTypeStats = stats.global?.byType.find((type) => type.id === selectedType);
  const effectiveSort =
    stats.category &&
    sort.startsWith("date:") &&
    !stats.category.dateFields.includes(sort.slice("date:".length))
      ? defaultSort
      : sort;
  const effectiveStatus =
    stats.category &&
    selectedStatus !== allStatuses &&
    !stats.category.byStatus.some((item) => item.name === selectedStatus)
      ? allStatuses
      : selectedStatus;

  useEffect(() => {
    const controller = new AbortController();
    void loadGlobalStats(controller.signal);
    return () => controller.abort();
  }, []);

  useEffect(() => {
    if (!stats.global) return;
    const validTypes = new Set(stats.global.byType.map((type) => type.id));
    const hasType = searchParams.has("type");

    if (!validTypes.has(selectedType)) {
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
      applyPreferencesToSearchParams(next, readAssetListPreferences(selectedType));
      if (next.toString() !== searchParams.toString()) {
        setSearchParams(next, { replace: true });
      }
    }
  }, [stats.global, selectedType]);

  useEffect(() => {
    if (!selectedType) return;
    const controller = new AbortController();
    void loadCategoryStats(selectedType, controller.signal);
    return () => controller.abort();
  }, [selectedType]);

  useEffect(() => {
    if (!stats.category || !sort.startsWith("date:")) return;
    if (stats.category.dateFields.includes(sort.slice("date:".length))) return;
    setQueryParam("sort", defaultSort, defaultSort);
  }, [stats.category, sort]);

  useEffect(() => {
    if (!stats.category || selectedStatus === allStatuses) return;
    if (stats.category.byStatus.some((item) => item.name === selectedStatus)) return;
    setQueryParam("status", allStatuses, allStatuses);
  }, [stats.category, selectedStatus]);

  useEffect(() => {
    if (!stats.global || !selectedType || !searchParams.has("type")) return;
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(searchParams));
  }, [stats.global, selectedType, selectedStatus, refs, cover, sort, direction, view]);

  useEffect(() => {
    setQueryInput(query);
  }, [query]);

  useEffect(() => {
    if (queryInput === query) return;
    const timeout = window.setTimeout(() => {
      const next = new URLSearchParams(searchParams);
      if (queryInput.trim()) next.set("q", queryInput.trim());
      else next.delete("q");
      next.set("page", "1");
      setSearchParams(next, { replace: true });
    }, 180);
    return () => window.clearTimeout(timeout);
  }, [queryInput]);

  useEffect(() => {
    const controller = new AbortController();
    void loadEntities({
      type: selectedType,
      status: effectiveStatus,
      refs,
      cover,
      sort: effectiveSort,
      direction,
      q: query,
      page,
    }, controller.signal);
    return () => controller.abort();
  }, [selectedType, effectiveStatus, refs, cover, effectiveSort, direction, query, page]);

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

  async function loadEntities(filters: {
    type: string;
    status: string;
    refs: string;
    cover: string;
    sort: string;
    direction: string;
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
          ...(filters.status !== allStatuses ? { status: filters.status } : {}),
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
    setSearchParams(next);
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
    setSearchParams(next);
  }

  return (
    <AppFrame error={stats.error ?? list.error}>
      <div className="h-full min-h-full overflow-hidden">
        <section className="h-full min-w-0">
          <div className="flex h-full flex-col">
            <div className="border-b px-3 py-2 md:hidden">
              <div className="grid grid-cols-2 gap-2">
                <div className="rounded-md border bg-card px-3 py-2">
                  <div className="text-[11px] uppercase text-muted-foreground">Entries</div>
                  <div className="mt-1 text-lg font-semibold tabular-nums">
                    {(selectedTypeStats?.count ?? list.total).toLocaleString()}
                  </div>
                </div>
                <div className="rounded-md border bg-card px-3 py-2">
                  <div className="text-[11px] uppercase text-muted-foreground">Relations</div>
                  <div className="mt-1 text-lg font-semibold tabular-nums">
                    {(stats.category?.relations ?? stats.global?.relations ?? 0).toLocaleString()}
                  </div>
                </div>
              </div>
              <div className="mt-2 flex items-center gap-2">
                <Select
                  value={selectedType}
                  onChange={(event) => selectType(event.target.value)}
                  className="min-w-0 flex-1"
                  aria-label="Type"
                >
                  {stats.global?.byType.map((type) => (
                    <option key={type.id} value={type.id}>
                      {type.label} ({type.count})
                    </option>
                  ))}
                </Select>
                {selectedTypeStats ? (
                  <Badge variant="secondary" className="shrink-0">
                    {selectedTypeStats.label}
                  </Badge>
                ) : null}
              </div>
            </div>

            <div className="flex items-center gap-2 border-b p-3">
              <SearchIcon className="text-muted-foreground" />
              <Input
                value={queryInput}
                onChange={(event) => setQueryInput(event.target.value)}
                placeholder="Search title, summary, path"
                className="min-w-0"
              />
            </div>

            <AssetToolbar
              className="hidden md:flex"
              stats={stats.category}
              status={effectiveStatus}
              refs={refs}
              cover={cover}
              sort={effectiveSort}
              direction={direction}
              view={view}
              onStatusChange={(value) => setQueryParam("status", value, allStatuses)}
              onRefsChange={(value) => setQueryParam("refs", value)}
              onCoverChange={(value) => setQueryParam("cover", value)}
              onSortChange={(value) => setQueryParam("sort", value, defaultSort)}
              onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
              onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
            />

            <div className="border-b px-3 py-2 md:hidden">
              <Button
                variant="outline"
                size="sm"
                className="w-full justify-between"
                onClick={() => setMobileFiltersOpen((open) => !open)}
                aria-expanded={mobileFiltersOpen}
              >
                <span className="flex items-center gap-2">
                  <SlidersHorizontalIcon data-icon="inline-start" />
                  Filters
                </span>
                <span className="text-muted-foreground">
                  {effectiveStatus !== allStatuses || refs !== allOptions || cover !== allOptions
                    ? "Active"
                    : "Default"}
                </span>
              </Button>
              {mobileFiltersOpen ? (
                <div className="mt-2 grid grid-cols-2 gap-2">
                  <Select
                    value={effectiveStatus}
                    onChange={(event) => setQueryParam("status", event.target.value, allStatuses)}
                    aria-label="Status"
                    className="min-w-0"
                  >
                    <option value={allStatuses}>All statuses</option>
                    {stats.category?.byStatus.map((item) => (
                      <option key={item.name} value={item.name}>
                        {item.name} ({item.count})
                      </option>
                    ))}
                  </Select>
                  <Select
                    value={refs}
                    onChange={(event) => setQueryParam("refs", event.target.value)}
                    aria-label="Refs"
                    className="min-w-0"
                  >
                    <option value={allOptions}>Any refs</option>
                    <option value="with">With refs</option>
                    <option value="without">Without refs</option>
                  </Select>
                  <Select
                    value={cover}
                    onChange={(event) => setQueryParam("cover", event.target.value)}
                    aria-label="Cover"
                    className="min-w-0"
                  >
                    <option value={allOptions}>Any cover</option>
                    <option value="with">With cover</option>
                    <option value="without">Without cover</option>
                  </Select>
                  <Select
                    value={effectiveSort}
                    onChange={(event) => setQueryParam("sort", event.target.value, defaultSort)}
                    aria-label="Sort"
                    className="min-w-0"
                  >
                    <option value={defaultSort}>Sort by title</option>
                    {stats.category?.dateFields.map((field) => (
                      <option key={field} value={`date:${field}`}>
                        Sort by {field}
                      </option>
                    ))}
                    <option value="status">Sort by status</option>
                    <option value="relations">Sort by links</option>
                    <option value="path">Sort by path</option>
                  </Select>
                  <Select
                    value={direction}
                    onChange={(event) => setQueryParam("direction", event.target.value, defaultDirection)}
                    aria-label="Direction"
                    className="min-w-0"
                  >
                    <option value={defaultDirection}>Ascending</option>
                    <option value="desc">Descending</option>
                  </Select>
                  <Select
                    value={view}
                    onChange={(event) => setQueryParam("view", event.target.value, defaultView, false)}
                    aria-label="View"
                    className="min-w-0"
                  >
                    <option value="list">List view</option>
                    <option value="grid">Grid view</option>
                  </Select>
                </div>
              ) : null}
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2 text-xs text-muted-foreground">
              <span>
                {list.total} entries
                {list.total > 0 ? ` · page ${list.page}/${list.totalPages}` : ""}
              </span>
              <span>
                {list.loading || stats.loading ? "Loading" : stats.global?.generatedAt.slice(0, 10)}
              </span>
            </div>
            <div className="min-h-0 flex-1 overflow-auto">
              {view === "grid" ? (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 p-3">
                  {list.entities.map((entity) => (
                    <EntityGridItem key={entity.id} entity={entity} />
                  ))}
                </div>
              ) : (
                list.entities.map((entity) => <EntityListItem key={entity.id} entity={entity} />)
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
  return ["status", "refs", "cover", "sort", "direction", "view"].some((key) => params.has(key));
}
