import { useCallback, useEffect, useMemo, useState } from "react";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { Grid2X2Icon, ListIcon, PlusIcon, SlidersHorizontalIcon, SparklesIcon } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import {
  allTagsQuery,
  configQuery,
  providerCatalogQuery,
  smartListPreviewQuery,
  statsQuery,
} from "@/api/queries";
import { EntityResults } from "@/components/assets/entity-results";
import { AppFrame } from "@/components/layout/app-frame";
import { CriteriaSummary } from "@/components/smart-lists/criteria-summary";
import {
  criteriaParam,
  criteriaRuleCount,
  decodeCriteria,
  encodeCriteria,
  hasCriteria,
} from "@/components/smart-lists/criteria-url";
import {
  RuleBuilder,
  pruneIncompleteRules,
  ruleFieldMetas,
} from "@/components/smart-lists/rule-builder";
import { SaveSmartListDialog } from "@/components/smart-lists/save-smart-list-dialog";
import { SortPicker } from "@/components/smart-lists/sort-picker";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { allTypes, defaultDirection, defaultSort, defaultView, pageSize } from "@/lib/constants";
import { fieldLabelsByType, typeExternalRefs, typeHasCoverField, typeSupportsQuickCapture } from "@/lib/type-config";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useDebouncedValue } from "@/hooks/use-debounce";
import { useTitleLanguage } from "@/lib/language";
import { useNumberFormat } from "@/lib/locale";
import type { SmartFilterGroup, SmartListView, SmartSortSpec } from "@/types/api";
import {
  applyPreferencesToSearchParams,
  preferencesFromSearchParams,
  readAssetListPreferences,
  writeAssetListPreferences,
} from "@/lib/asset-list-preferences";

/// The library browser: a smart list you haven't named yet. Its whole state —
/// type scope, criteria, sort, layout, search — lives in the URL and is
/// evaluated by the same endpoint the smart-list editor previews with, so
/// "Save as smart list" is a rename, not a conversion, and a Home section can
/// link here with its criteria intact.
export function LibraryPage() {
  const { t } = useLingui();
  const formatNumber = useNumberFormat();
  const [searchParams, setSearchParams] = useSearchParams();
  const globalStats = useQuery(statsQuery());
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  const language = useTitleLanguage();

  const firstType = globalStats.data?.byType[0]?.id ?? allTypes;
  const selectedType = searchParams.get("type") ?? allTypes;
  const isGlobalType = selectedType === allTypes;
  const encodedCriteria = searchParams.get(criteriaParam);
  const criteria = useMemo(() => decodeCriteria(encodedCriteria), [encodedCriteria]);
  const view = searchParams.get("view") === "grid" ? "grid" : defaultView;
  const query = searchParams.get("q") ?? "";
  const hasQuery = query.trim().length > 0;
  // Relevance is only ever an *implicit* default: while searching with no sort
  // of their own, results rank by match quality (no sort key goes to the
  // server). An explicit sort in the URL — the user picking one — always wins,
  // so relevance is never written to the URL or remembered as a preference.
  const sortParam = searchParams.get("sort");
  const sort = useMemo<SmartSortSpec | undefined>(
    () =>
      hasQuery && !sortParam
        ? undefined
        : {
            property: sortParam ?? defaultSort,
            direction: searchParams.get("direction") === "desc" ? "desc" : defaultDirection,
          },
    [hasQuery, sortParam, searchParams],
  );
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [saveSmartOpen, setSaveSmartOpen] = useState(false);

  const selectedTypeConfig = config.data?.types.find((type) => type.id === selectedType);
  const configTypes = config.data?.types;
  const scopeTypeConfigs = useMemo(
    () => (isGlobalType ? (configTypes ?? []) : selectedTypeConfig ? [selectedTypeConfig] : []),
    [isGlobalType, configTypes, selectedTypeConfig],
  );
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  const fieldMetas = useMemo(
    () => ruleFieldMetas(scopeTypeConfigs, config.data?.tagsField ?? undefined, allTags, t),
    [scopeTypeConfigs, config.data?.tagsField, allTags, t],
  );
  const fieldLabels = useMemo(() => fieldLabelsByType(configTypes), [configTypes]);
  // Covers show for "all types" and for any concrete type that declares an
  // image/imageList field; a type without one shows no cover slot at all.
  const showCovers = isGlobalType || typeHasCoverField(selectedTypeConfig);

  // Rules are edited straight into the URL (`replace`, so the back button still
  // steps between real navigations) and only the criteria trail a beat behind,
  // so typing inside a rule doesn't fire a request per keystroke while paging
  // and sorting stay immediate.
  const debouncedCriteria = useDebouncedValue(criteria, 250);
  const list = useQuery(
    smartListPreviewQuery({
      scope: isGlobalType ? undefined : selectedType,
      filters: pruneIncompleteRules(debouncedCriteria),
      sort: sort ? [sort] : [],
      page,
      pageSize,
      titleLanguage: language,
      ...(query.trim() ? { q: query.trim() } : {}),
    }),
  );
  const entities = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  const totalPages = list.data?.totalPages ?? 1;
  const error = globalStats.error ?? config.error ?? capabilities.error ?? list.error;

  const providerIds = useMemo(
    () => new Set((providerCatalog.data?.providers ?? []).map((item) => item.id.toLowerCase())),
    [providerCatalog.data],
  );
  const selectedTypeSupportsQuickCapture = selectedTypeConfig
    ? providerCatalog.data
      ? typeSupportsQuickCapture(selectedTypeConfig, providerIds)
      : typeExternalRefs(selectedTypeConfig).length > 0
    : true;
  const createHref = isGlobalType
    ? "/entities/new"
    : selectedTypeSupportsQuickCapture
      ? `/entities/new?type=${encodeURIComponent(selectedType)}`
      : `/entities/new/manual?type=${encodeURIComponent(selectedType)}`;

  // Writes are `replace` so editing rules doesn't fill the history stack; the
  // back button still steps between real navigations. Preferences aren't
  // written here — the effect below persists them off the settled URL.
  const setParams = useCallback(
    (mutate: (params: URLSearchParams) => void, { resetPage = true } = {}) => {
      const next = new URLSearchParams(searchParams);
      mutate(next);
      if (resetPage) next.set("page", "1");
      setSearchParams(next, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  const setCriteria = useCallback(
    (next: SmartFilterGroup) => {
      setParams((params) => {
        const encoded = encodeCriteria(next);
        if (encoded === undefined) params.delete(criteriaParam);
        else params.set(criteriaParam, encoded);
      });
    },
    [setParams],
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
    if (!globalStats.data || !selectedType || !searchParams.has("type")) return;
    writeAssetListPreferences(selectedType, preferencesFromSearchParams(searchParams));
  }, [globalStats.data, selectedType, searchParams]);

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
    // Criteria are written against one type's schema, so switching scope drops
    // them rather than carrying rules the new type doesn't have.
    next.delete(criteriaParam);
    applyPreferencesToSearchParams(next, readAssetListPreferences(type));
    next.set("page", "1");
    setSearchParams(next, { replace: true });
  }

  function goToPage(nextPage: number) {
    const next = new URLSearchParams(searchParams);
    next.set("page", String(nextPage));
    setSearchParams(next);
  }

  const ruleCount = criteriaRuleCount(criteria);

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <div className="h-full min-h-full overflow-hidden">
        <section className="h-full min-w-0">
          <div className="flex h-full flex-col">
            <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
              <Select
                value={selectedType}
                onChange={(event) => selectType(event.target.value)}
                className="min-w-0 max-w-56 flex-1 md:hidden"
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
                variant={filtersOpen || ruleCount > 0 ? "secondary" : "outline"}
                size="sm"
                onClick={() => setFiltersOpen((open) => !open)}
                aria-expanded={filtersOpen}
              >
                <SlidersHorizontalIcon data-icon="inline-start" />
                <Trans>Filter</Trans>
                {ruleCount > 0 ? <span className="tabular-nums">({ruleCount})</span> : null}
              </Button>
              <Separator orientation="vertical" className="mx-1 hidden h-6 sm:block" />
              <SortPicker
                typeConfigs={scopeTypeConfigs}
                sort={sort}
                unsortedLabel={hasQuery ? t`Relevance` : undefined}
                onChange={(next) =>
                  setParams((params) => {
                    if (!next) {
                      params.delete("sort");
                      params.delete("direction");
                      return;
                    }
                    params.set("sort", next.property);
                    if (next.direction === defaultDirection) params.delete("direction");
                    else params.set("direction", next.direction);
                  })
                }
              />
              <div className="ml-auto flex items-center gap-1">
                {(["list", "grid"] as const).map((layout) => (
                  <Button
                    key={layout}
                    variant={view === layout ? "secondary" : "ghost"}
                    size="sm"
                    onClick={() =>
                      setParams(
                        (params) => {
                          if (layout === defaultView) params.delete("view");
                          else params.set("view", layout);
                        },
                        { resetPage: false },
                      )
                    }
                  >
                    {layout === "list" ? (
                      <ListIcon data-icon="inline-start" />
                    ) : (
                      <Grid2X2Icon data-icon="inline-start" />
                    )}
                    {layout === "list" ? <Trans>List</Trans> : <Trans>Grid</Trans>}
                  </Button>
                ))}
              </div>
            </div>

            {filtersOpen ? (
              <div className="border-b px-3 py-2">
                <RuleBuilder
                  fieldMetas={fieldMetas}
                  value={criteria}
                  onChange={setCriteria}
                />
              </div>
            ) : hasCriteria(criteria) ? (
              <div className="border-b px-3 py-2">
                <CriteriaSummary group={criteria} />
              </div>
            ) : null}

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
            <EntityResults
              entities={entities}
              layout={view}
              labelsByType={fieldLabels}
              showCover={showCovers}
              showType={isGlobalType}
              loading={list.isFetching}
              page={list.data?.page ?? page}
              totalPages={totalPages}
              total={total}
              empty={<Trans>No entries</Trans>}
              onPageChange={goToPage}
            />
          </div>
        </section>
      </div>

      <SaveSmartListDialog
        open={saveSmartOpen}
        onOpenChange={setSaveSmartOpen}
        definition={{
          scope: isGlobalType ? undefined : selectedType,
          filters: pruneIncompleteRules(criteria),
          views: smartListViews(sort),
        }}
        searchActive={query.trim().length > 0}
      />
    </AppFrame>
  );
}

function hasPreferenceParams(params: URLSearchParams) {
  return ["sort", "direction", "view"].some((key) => params.has(key));
}

/// The two views a browse state saves as: the layouts the app itself offers,
/// both carrying the browse sort. A relevance ranking has no sort key (and no
/// meaning without the search, which isn't saved), so the views go unsorted.
function smartListViews(sort: SmartSortSpec | undefined): SmartListView[] {
  const keys = sort ? [sort] : [];
  return [
    { name: "List", layout: "list", sort: keys },
    { name: "Grid", layout: "grid", sort: keys },
  ];
}
