import { useCallback, useEffect, useMemo, useState } from "react";
import type { I18n, MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon, SlidersHorizontalIcon, SparklesIcon } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { allTagsQuery, configQuery, entitiesQuery, statsQuery } from "@/api/queries";
import { useRelationSearch } from "@/api/use-relation-search";
import { AssetToolbar } from "@/components/assets/asset-toolbar";
import type { FieldFilter, FieldFilterOption } from "@/components/assets/asset-toolbar";
import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { smartListFromLibraryState } from "@/components/smart-lists/from-library";
import { SaveSmartListDialog } from "@/components/smart-lists/save-smart-list-dialog";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import {
  allOptions,
  allTypes,
  defaultDirection,
  defaultSort,
  defaultView,
  pageSize,
  defaultTagsField,
  relevanceSort,
} from "@/lib/constants";
import {
  fieldDisplayLabel,
  fieldLabelAcrossTypes,
  fieldLabelsByType,
  typeHasCoverField,
} from "@/lib/type-config";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";
import { useNumberFormat } from "@/lib/locale";
import { entityTitle } from "@/lib/title-language";
import type { TypeConfig } from "@/types/api";
import {
  applyPreferencesToSearchParams,
  preferencesFromSearchParams,
  readAssetListPreferences,
  writeAssetListPreferences,
} from "@/lib/asset-list-preferences";

export function LibraryPage() {
  const { t, i18n } = useLingui();
  const formatNumber = useNumberFormat();
  const [searchParams, setSearchParams] = useSearchParams();
  const globalStats = useQuery(statsQuery());
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  const firstType = globalStats.data?.byType[0]?.id ?? allTypes;
  const selectedType = searchParams.get("type") ?? allTypes;
  const isGlobalType = selectedType === allTypes;
  const categoryStats = useQuery({
    ...statsQuery({ type: selectedType }),
    enabled: !isGlobalType,
  });
  const direction = searchParams.get("direction") === "desc" ? "desc" : defaultDirection;
  const view = searchParams.get("view") === "grid" ? "grid" : defaultView;
  const language = useTitleLanguage();
  const onRelationSearch = useRelationSearch();
  const query = searchParams.get("q") ?? "";
  // While a search is active, ranking by match quality is the sensible default;
  // an explicit `sort` in the URL (the user picking one) always wins. `relevance`
  // is only ever an implicit default, so it's never written to the URL or saved.
  const hasQuery = query.trim().length > 0;
  const contextDefaultSort = hasQuery ? relevanceSort : defaultSort;
  const sort = searchParams.get("sort") ?? contextDefaultSort;
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [mobileFiltersOpen, setMobileFiltersOpen] = useState(false);
  const [saveSmartOpen, setSaveSmartOpen] = useState(false);
  const scopeStats = isGlobalType ? globalStats.data : categoryStats.data;
  const selectedTypeConfig = config.data?.types.find((type) => type.id === selectedType);
  // Covers show for "all types" and for any concrete type that declares an
  // image/imageList field; a type without one shows no cover slot at all.
  const showCovers = isGlobalType || typeHasCoverField(selectedTypeConfig);
  const scopeTypeConfigs = useMemo(
    () =>
      isGlobalType
        ? (config.data?.types ?? [])
        : selectedTypeConfig
          ? [selectedTypeConfig]
          : [],
    [isGlobalType, config.data, selectedTypeConfig],
  );
  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  // The built-in tags filter is universal (not schema-derived). Its selected
  // values are read straight from the URL — not restricted to the current
  // vocabulary — so they survive while `allTags` loads and serialize through the
  // same `filters` param as schema field filters (server OR-matches them).
  const tagsFieldName = config.data?.tagsField ?? defaultTagsField;
  const tagFilter: FieldFilter = useMemo(
    () => ({
      field: tagsFieldName,
      label: t`Tags`,
      kind: "multi",
      options: allTags.map((value) => ({ value })),
      values: uniqueStrings(searchParams.getAll(fieldFilterParamKey(tagsFieldName))),
    }),
    [allTags, searchParams, tagsFieldName, t],
  );
  // Relation fields (per selected type) become dynamically-loaded multi-selects,
  // like enum lists but with suggestions searched on demand. Hidden for "all
  // types" for the same reason as enum fields — they're type-specific.
  // Per-field suggestion loaders, memoized WITHOUT `searchParams` so their
  // identity stays stable as chips are added/removed — otherwise the open
  // dropdown re-fetches the same suggestions after every selection.
  const relationLoadOptions = useMemo(() => {
    const loaders = new Map<string, NonNullable<FieldFilter["loadOptions"]>>();
    if (isGlobalType || !selectedTypeConfig) return loaders;
    for (const field of selectedTypeConfig.fields ?? []) {
      if (field.fieldType !== "relation") continue;
      loaders.set(field.field, (search, signal) =>
        onRelationSearch({ relationType: field.relationType, query: search, signal }).then((items) =>
          items
            .map((item) => ({ value: item.basename, label: entityTitle(item, language) }))
            .filter((option) => option.value),
        ),
      );
    }
    return loaders;
  }, [isGlobalType, selectedTypeConfig, onRelationSearch, language]);
  const relationFilters = useMemo<FieldFilter[]>(() => {
    if (isGlobalType || !selectedTypeConfig) return [];
    return (selectedTypeConfig.fields ?? [])
      .filter((field) => field.fieldType === "relation")
      .map((field) => ({
        field: field.field,
        label: fieldDisplayLabel(field),
        kind: "relation" as const,
        options: [],
        values: uniqueStrings(searchParams.getAll(fieldFilterParamKey(field.field))),
        loadOptions: relationLoadOptions.get(field.field),
      }));
  }, [isGlobalType, selectedTypeConfig, searchParams, relationLoadOptions]);
  const fieldFilters = useMemo(() => {
    // Enum/enumList/bool field filters are type-specific, so only "all types"
    // keeps the universal Tags filter; a concrete type adds its schema fields.
    const schemaFilters = isGlobalType ? [] : fieldFiltersForTypes(scopeTypeConfigs, searchParams, i18n);
    // Hide the tags filter only when the vault has no tags and none are selected.
    const showTags = tagFilter.options.length > 0 || tagFilter.values.length > 0;
    const base = showTags ? [tagFilter, ...schemaFilters] : schemaFilters;
    return [...base, ...relationFilters];
  }, [tagFilter, scopeTypeConfigs, searchParams, isGlobalType, relationFilters, i18n]);
  const activeFieldFilters = fieldFilters.filter((filter) => filter.values.length > 0);
  const effectiveSort =
    scopeStats &&
    sort.startsWith("date:") &&
    !scopeStats.dateFields.includes(sort.slice("date:".length))
      ? contextDefaultSort
      : sort;
  const createHref = isGlobalType
    ? "/entities/new"
    : `/entities/new?type=${encodeURIComponent(selectedType)}`;
  const filtersActive =
    activeFieldFilters.length > 0 ||
    effectiveSort !== contextDefaultSort ||
    direction !== defaultDirection ||
    view !== defaultView;

  const list = useQuery(
    entitiesQuery({
      type: selectedType,
      page,
      pageSize,
      sort: effectiveSort,
      direction,
      titleLanguage: language,
      ...entityFiltersParam(activeFieldFilters),
      ...(query.trim() ? { q: query.trim() } : {}),
    }),
  );
  const entities = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  const totalPages = list.data?.totalPages ?? 1;
  const error = globalStats.error ?? categoryStats.error ?? config.error ?? capabilities.error ?? list.error;

  // Stable across renders for a given URL/type so the normalization effects below
  // can depend on it without re-running every render.
  const setQueryParam = useCallback(
    (key: string, value: string, defaultValue = allOptions, resetPage = true) => {
      const next = new URLSearchParams(searchParams);
      if (value === defaultValue) next.delete(key);
      else next.set(key, value);
      if (resetPage) next.set("page", "1");
      writeAssetListPreferences(selectedType, preferencesFromSearchParams(next));
      setSearchParams(next, { replace: true });
    },
    [searchParams, selectedType, setSearchParams],
  );

  // Bootstraps the type + saved preferences when the type or query changes.
  // Intentionally keyed on those transitions only — `searchParams`/`firstType`
  // are read but must not re-trigger it, or every filter/page change would
  // re-run the bootstrap.
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [globalStats.data, selectedType, query]);

  useEffect(() => {
    if (!scopeStats || !sort.startsWith("date:")) return;
    if (scopeStats.dateFields.includes(sort.slice("date:".length))) return;
    setQueryParam("sort", defaultSort, defaultSort);
  }, [scopeStats, sort, setQueryParam]);

  useEffect(() => {
    if (!config.data) return;
    const next = cleanUnsupportedFieldFilterParams(searchParams, fieldFilters);
    if (!next) return;
    next.set("page", "1");
    setSearchParams(next, { replace: true });
  }, [config.data, fieldFilters, searchParams, setSearchParams]);

  useEffect(() => {
    if (!globalStats.data || !selectedType || !searchParams.has("type")) return;
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(searchParams));
  }, [globalStats.data, selectedType, searchParams, sort, direction, view]);

  useEffect(() => {
    // Only re-sync the URL to the server's (clamped) page once the real response
    // settles. While `keepPreviousData` shows the previous page during a page
    // change, `list.data.page` still reflects the OLD page — syncing then would
    // snap the URL back and make pagination impossible.
    if (!list.data || list.isPlaceholderData || list.data.page === page) return;
    const next = new URLSearchParams(searchParams);
    next.set("page", String(list.data.page));
    setSearchParams(next, { replace: true });
  }, [list.data, list.isPlaceholderData, page, searchParams, setSearchParams]);

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

  function setFieldFilterParam(field: string, values: string[]) {
    const next = new URLSearchParams(searchParams);
    const key = fieldFilterParamKey(field);
    next.delete(key);
    for (const value of uniqueStrings(values)) {
      next.append(key, value);
    }
    next.set("page", "1");
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
                  aria-label={t`Type`}
                >
                  <option value={allTypes}>
                    {t`All types (${formatNumber(globalStats.data?.total ?? 0)})`}
                  </option>
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
                  aria-label={t`Toggle filters`}
                  aria-expanded={mobileFiltersOpen}
                >
                  <SlidersHorizontalIcon />
                </Button>
              </div>
              {/* The entry count, page, and Add all live in the shared row below —
                  a mobile-only summary here just repeated the type and count. */}
              {mobileFiltersOpen ? (
                <AssetToolbar
                  compact
                  showLabel={false}
                  className="mt-2"
                  stats={scopeStats}
                  sort={effectiveSort}
                  direction={direction}
                  view={view}
                  fieldFilters={fieldFilters}
                  showRelevanceSort={hasQuery}
                  dateFieldLabel={(field) => fieldLabelAcrossTypes(scopeTypeConfigs, field)}
                  onSortChange={(value) => setQueryParam("sort", value, contextDefaultSort)}
                  onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
                  onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
                  onFieldFilterChange={setFieldFilterParam}
                />
              ) : null}
            </div>

            <AssetToolbar
              className="hidden md:flex"
              stats={scopeStats}
              sort={effectiveSort}
              direction={direction}
              view={view}
              fieldFilters={fieldFilters}
              showRelevanceSort={hasQuery}
              dateFieldLabel={(field) => fieldLabelAcrossTypes(scopeTypeConfigs, field)}
              onSortChange={(value) => setQueryParam("sort", value, contextDefaultSort)}
              onDirectionChange={(value) => setQueryParam("direction", value, defaultDirection)}
              onViewChange={(value) => setQueryParam("view", value, defaultView, false)}
              onFieldFilterChange={setFieldFilterParam}
            />

            <div className="flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2 text-xs text-muted-foreground">
              <span>
                {total > 0 ? (
                  <Trans>
                    <Plural value={total} one="# entry" other="# entries" /> · page{" "}
                    {list.data?.page ?? page}/{totalPages}
                  </Trans>
                ) : (
                  <Plural value={total} one="# entry" other="# entries" />
                )}
              </span>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!contentWritable}
                  title={!contentWritable ? CONTENT_WRITES_DISABLED : t`Save as smart list`}
                  onClick={() => setSaveSmartOpen(true)}
                >
                  <SparklesIcon data-icon="inline-start" />
                  <Trans>Save as smart list</Trans>
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!contentWritable}
                  title={!contentWritable ? CONTENT_WRITES_DISABLED : t`Add entity`}
                  asChild={contentWritable}
                >
                  {!contentWritable ? (
                    <span>
                      <PlusIcon data-icon="inline-start" />
                      <Trans>Add</Trans>
                    </span>
                  ) : (
                    <Link to={createHref}>
                      <PlusIcon data-icon="inline-start" />
                      <Trans>Add</Trans>
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
                      labelsByType={fieldLabels}
                      showCover={showCovers}
                      showType={isGlobalType}
                    />
                  ))}
                </div>
              ) : (
                entities.map((entity) => (
                  <EntityListItem
                    key={entity.id}
                    entity={entity}
                    labelsByType={fieldLabels}
                    showCover={showCovers}
                    showType={isGlobalType}
                  />
                ))
              )}
              {!list.isFetching && entities.length === 0 ? (
                <div className="p-8 text-center text-sm text-muted-foreground">
                  <Trans>No entries</Trans>
                </div>
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

      <SaveSmartListDialog
        open={saveSmartOpen}
        onOpenChange={setSaveSmartOpen}
        definition={smartListFromLibraryState({
          selectedType,
          typeConfig: selectedTypeConfig,
          tagsField: tagsFieldName,
          activeFilters: activeFieldFilters,
          sort: effectiveSort,
          direction,
        })}
      />
    </AppFrame>
  );
}

function hasPreferenceParams(params: URLSearchParams) {
  return ["sort", "direction", "view"].some((key) => params.has(key));
}

function fieldFiltersForTypes(typeConfigs: TypeConfig[], params: URLSearchParams, i18n: I18n) {
  const byField = new Map<
    string,
    { field: string; label: string; kind: FieldFilter["kind"]; options: FieldFilterOption[] }
  >();
  for (const typeConfig of typeConfigs) {
    for (const field of typeConfig.fields ?? []) {
      const filter = fieldFilterMetadata(field, i18n);
      if (!filter) continue;
      const { kind, options } = filter;
      if (options.length === 0) continue;
      const current = byField.get(field.field);
      if (current) {
        current.options = uniqueFilterOptions([...current.options, ...options]);
      } else {
        byField.set(field.field, {
          field: field.field,
          label: fieldDisplayLabel(field),
          kind,
          options,
        });
      }
    }
  }
  return [...byField.values()].map((filter) => ({
    ...filter,
    values: readFieldFilterValues(params, filter.field, filter.options, filter.kind),
  }));
}

const boolFilterLabels: Record<"true" | "false", MessageDescriptor> = {
  true: msg`Yes`,
  false: msg`No`,
};

function fieldFilterMetadata(
  field: NonNullable<TypeConfig["fields"]>[number],
  i18n: I18n,
): Pick<FieldFilter, "kind" | "options"> | undefined {
  if ((field.fieldType === "enum" || field.fieldType === "enumList") && field.enumOptions?.length) {
    return {
      kind: "multi",
      options: uniqueStrings(field.enumOptions).map((value) => ({ value })),
    };
  }
  if (field.fieldType === "bool") {
    return {
      kind: "bool",
      options: [
        { value: "true", label: i18n._(boolFilterLabels.true) },
        { value: "false", label: i18n._(boolFilterLabels.false) },
      ],
    };
  }
  return undefined;
}

function entityFiltersParam(filters: FieldFilter[]) {
  if (filters.length === 0) return {};
  return {
    filters: JSON.stringify(
      filters.map((filter) => ({
        field: filter.field,
        values: filter.values,
      })),
    ),
  };
}

function fieldFilterParamKey(field: string) {
  return `filter:${field}`;
}

function readFieldFilterValues(
  params: URLSearchParams,
  field: string,
  options: FieldFilterOption[],
  kind: FieldFilter["kind"],
) {
  const allowed = new Set(options.map((option) => option.value));
  const values = uniqueStrings(params.getAll(fieldFilterParamKey(field))).filter((value) => allowed.has(value));
  return kind === "bool" ? values.slice(0, 1) : values;
}

function cleanUnsupportedFieldFilterParams(params: URLSearchParams, filters: FieldFilter[]) {
  const cleanValuesByKey = new Map(
    filters.map((filter) => [fieldFilterParamKey(filter.field), filter.values]),
  );
  const next = new URLSearchParams(params);
  let changed = false;

  for (const key of [...params.keys()].filter((item) => item.startsWith("filter:"))) {
    const cleanValues = cleanValuesByKey.get(key);
    if (!cleanValues) {
      next.delete(key);
      changed = true;
      continue;
    }

    const currentValues = params.getAll(key);
    if (arraysEqual(currentValues, cleanValues)) continue;
    next.delete(key);
    for (const value of cleanValues) next.append(key, value);
    changed = true;
  }

  return changed ? next : undefined;
}

function uniqueStrings(values: string[]) {
  return values.filter((value, index, items) => value.trim() && items.indexOf(value) === index);
}

function uniqueFilterOptions(options: FieldFilterOption[]) {
  const seen = new Set<string>();
  return options.filter((option) => {
    const value = option.value.trim();
    if (!value || seen.has(value)) return false;
    seen.add(value);
    return true;
  });
}

function arraysEqual(a: string[], b: string[]) {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}
