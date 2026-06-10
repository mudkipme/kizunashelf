import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon, SlidersHorizontalIcon } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { capabilitiesQuery, configQuery, entitiesQuery, statsQuery } from "@/api/queries";
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

export function LibraryPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const globalStats = useQuery(statsQuery());
  const config = useQuery(configQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const firstType = globalStats.data?.byType[0]?.id ?? allTypes;
  const selectedType = searchParams.get("type") ?? allTypes;
  const isGlobalType = selectedType === allTypes;
  const categoryStats = useQuery({
    ...statsQuery({ type: selectedType }),
    enabled: !isGlobalType,
  });
  const refs = searchParams.get("refs") ?? allOptions;
  const cover = searchParams.get("cover") ?? allOptions;
  const sort = searchParams.get("sort") ?? defaultSort;
  const direction = searchParams.get("direction") === "desc" ? "desc" : defaultDirection;
  const view = searchParams.get("view") === "grid" ? "grid" : defaultView;
  const titleLanguage = searchParams.get("titleLanguage") ?? defaultTitleOptionId;
  const query = searchParams.get("q") ?? "";
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [mobileFiltersOpen, setMobileFiltersOpen] = useState(false);
  const selectedTypeStats = globalStats.data?.byType.find((type) => type.id === selectedType);
  const scopeStats = isGlobalType ? globalStats.data : categoryStats.data;
  const selectedTypeConfig = config.data?.types.find((type) => type.id === selectedType);
  const scopeTypeConfigs = isGlobalType
    ? (config.data?.types ?? [])
    : selectedTypeConfig
      ? [selectedTypeConfig]
      : [];
  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const supportsRefsFilter =
    !config.data || scopeTypeConfigs.some((typeConfig) => hasAnyFieldType(typeConfig, ["externalRef"]));
  const supportsCoverFilter =
    !config.data ||
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
    ? (globalStats.data?.total ?? 0)
    : (selectedTypeStats?.count ?? 0);
  const filtersActive =
    effectiveRefs !== allOptions ||
    effectiveCover !== allOptions ||
    effectiveSort !== defaultSort ||
    direction !== defaultDirection ||
    view !== defaultView ||
    effectiveTitleLanguage !== defaultTitleOptionId;

  const list = useQuery(
    entitiesQuery({
      type: selectedType,
      page,
      pageSize,
      sort: effectiveSort,
      direction,
      titleLanguage: effectiveTitleLanguage,
      ...(effectiveRefs !== allOptions ? { refs: effectiveRefs } : {}),
      ...(effectiveCover !== allOptions ? { cover: effectiveCover } : {}),
      ...(query.trim() ? { q: query.trim() } : {}),
    }),
  );
  const entities = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  const totalPages = list.data?.totalPages ?? 1;
  const loading = globalStats.isPending || config.isPending || capabilities.isPending || list.isPending;
  const error = globalStats.error ?? categoryStats.error ?? config.error ?? capabilities.error ?? list.error;

  useEffect(() => {
    if (!globalStats.data) return;
    const validTypes = new Set(globalStats.data.byType.map((type) => type.id));
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
  }, [globalStats.data, selectedType, query]);

  useEffect(() => {
    if (!scopeStats || !sort.startsWith("date:")) return;
    if (scopeStats.dateFields.includes(sort.slice("date:".length))) return;
    setQueryParam("sort", defaultSort, defaultSort);
  }, [scopeStats, sort]);

  useEffect(() => {
    if (!config.data || supportsRefsFilter || refs === allOptions) return;
    setQueryParam("refs", allOptions);
  }, [config.data, supportsRefsFilter, refs]);

  useEffect(() => {
    if (!config.data || supportsCoverFilter || cover === allOptions) return;
    setQueryParam("cover", allOptions);
  }, [config.data, supportsCoverFilter, cover]);

  useEffect(() => {
    if (!config.data || titleLanguage === defaultTitleOptionId) return;
    if (titleLanguages.includes(titleLanguage)) return;
    setQueryParam("titleLanguage", defaultTitleOptionId, defaultTitleOptionId, false);
  }, [config.data, titleLanguages, titleLanguage]);

  useEffect(() => {
    if (!globalStats.data || !selectedType || !searchParams.has("type")) return;
    if ((!supportsRefsFilter && refs !== allOptions) || (!supportsCoverFilter && cover !== allOptions)) {
      return;
    }
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(searchParams));
  }, [
    globalStats.data,
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
    if (!list.data || list.data.page === page) return;
    const next = new URLSearchParams(searchParams);
    next.set("page", String(list.data.page));
    setSearchParams(next, { replace: true });
  }, [list.data, page, searchParams, setSearchParams]);

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
    <AppFrame error={error ? errorMessage(error) : undefined}>
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
                  <option value={allTypes}>All types ({globalStats.data?.total ?? 0})</option>
                  {globalStats.data?.byType.map((type) => (
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
                  disabled={capabilities.data?.contentWritable === false}
                  aria-label="Add entity"
                  title={
                    capabilities.data?.contentWritable === false
                      ? "Content writes are disabled"
                      : "Add entity"
                  }
                  asChild={capabilities.data?.contentWritable !== false}
                >
                  {capabilities.data?.contentWritable === false ? (
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
                {total} entries
                {total > 0 ? ` · page ${list.data?.page ?? page}/${totalPages}` : ""}
              </span>
              <div className="flex items-center gap-2">
                <span>
                  {loading ? "Loading" : globalStats.data?.generatedAt.slice(0, 10)}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={capabilities.data?.contentWritable === false}
                  title={
                    capabilities.data?.contentWritable === false
                      ? "Content writes are disabled"
                      : "Add entity"
                  }
                  asChild={capabilities.data?.contentWritable !== false}
                >
                  {capabilities.data?.contentWritable === false ? (
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
                  {entities.map((entity) => (
                    <EntityGridItem
                      key={entity.id}
                      entity={entity}
                      titleLanguage={effectiveTitleLanguage}
                      labelsByType={fieldLabels}
                    />
                  ))}
                </div>
              ) : (
                entities.map((entity) => (
                  <EntityListItem
                    key={entity.id}
                    entity={entity}
                    titleLanguage={effectiveTitleLanguage}
                    labelsByType={fieldLabels}
                  />
                ))
              )}
              {!list.isPending && entities.length === 0 ? (
                <div className="p-8 text-center text-sm text-muted-foreground">No entries</div>
              ) : null}
            </div>
            <PaginationBar
              page={list.data?.page ?? page}
              totalPages={totalPages}
              total={total}
              pageSize={pageSize}
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
