import { useEffect, useState } from "react";
import { SearchIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { fetchJson, errorMessage } from "@/api/client";
import { AssetToolbar } from "@/components/assets/asset-toolbar";
import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { LibrarySidebar } from "@/components/assets/library-sidebar";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { Input } from "@/components/ui/input";
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
import type { EntityListResponse, EntitySummary, StatsResponse } from "@/types/api";

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

  useEffect(() => {
    void loadGlobalStats();
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
    void loadCategoryStats(selectedType);
  }, [selectedType]);

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
    void loadEntities({
      type: selectedType,
      status: selectedStatus,
      refs,
      cover,
      sort,
      direction,
      q: query,
      page,
    });
  }, [selectedType, selectedStatus, refs, cover, sort, direction, query, page]);

  async function loadGlobalStats() {
    setStats((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const global = await fetchJson<StatsResponse>("/api/stats");
      setStats((current) => ({ ...current, global, loading: false }));
    } catch (error) {
      setStats((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  async function loadCategoryStats(type: string) {
    try {
      const category = await fetchJson<StatsResponse>(`/api/stats?type=${encodeURIComponent(type)}`);
      setStats((current) => ({ ...current, category, error: undefined }));
    } catch (error) {
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
  }) {
    setList((current) => ({ ...current, loading: true, error: undefined }));
    const params = new URLSearchParams({
      type: filters.type,
      page: String(filters.page),
      pageSize: String(pageSize),
      sort: filters.sort,
      direction: filters.direction,
    });
    if (filters.status !== allStatuses) params.set("status", filters.status);
    if (filters.refs !== allOptions) params.set("refs", filters.refs);
    if (filters.cover !== allOptions) params.set("cover", filters.cover);
    if (filters.q.trim()) params.set("q", filters.q.trim());

    try {
      const result = await fetchJson<EntityListResponse>(`/api/entities?${params}`);
      setList({ ...result, entities: result.items, loading: false });
      if (result.page !== filters.page) {
        const next = new URLSearchParams(searchParams);
        next.set("page", String(result.page));
        setSearchParams(next, { replace: true });
      }
    } catch (error) {
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
      <div className="grid min-h-[calc(100vh-3rem)] grid-cols-1 md:grid-cols-[220px_minmax(420px,1fr)]">
        <LibrarySidebar
          stats={stats.global}
          selectedType={selectedType}
          onSelectType={selectType}
        />

        <section className="min-h-[520px]">
          <div className="flex h-full flex-col">
            <div className="flex items-center gap-2 border-b p-3">
              <SearchIcon className="text-muted-foreground" />
              <Input
                value={queryInput}
                onChange={(event) => setQueryInput(event.target.value)}
                placeholder="Search title, summary, path"
              />
            </div>
            <AssetToolbar
              stats={stats.category}
              status={selectedStatus}
              refs={refs}
              cover={cover}
              sort={sort}
              direction={direction}
              view={view}
              onStatusChange={(value) => setQueryParam("status", value, allStatuses)}
              onRefsChange={(value) => setQueryParam("refs", value)}
              onCoverChange={(value) => setQueryParam("cover", value)}
              onSortChange={(value) => setQueryParam("sort", value, defaultSort)}
              onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
              onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
            />
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
