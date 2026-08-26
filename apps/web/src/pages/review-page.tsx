import { useEffect, useMemo, useState } from "react";
import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ArrowRightIcon, SearchIcon } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { cleanupQueuesQuery, configQuery, healthQuery } from "@/api/queries";
import { AssetDownloadPanel } from "@/components/assets/asset-download-panel";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityTitle } from "@/components/entities/entity-title";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Placeholder } from "@/components/ui/placeholder";
import { Select } from "@/components/ui/select";
import {
  allEntityFilter,
  compareEntitiesByTypeThenTitle,
  entityDateOptions,
  entityMatchesDate,
  entityMatchesQuery,
  entityTypeOptions,
} from "@/lib/entity-filters";
import { useDebouncedCallback } from "@/hooks/use-debounce";
import { useLanguagePreference, useTitleLanguage } from "@/lib/language";
import { useNumberFormat } from "@/lib/locale";
import { entityFieldLabel, fieldLabelsByType } from "@/lib/type-config";
import { cn } from "@/lib/utils";
import type {
  CleanupQueueSummary,
  CleanupQueuesResponse,
  CleanupUnresolvedRelation,
  EntitySummary,
  HealthResponse,
} from "@/types/api";

type QueueDefinition = {
  id: string;
  label: MessageDescriptor;
  kind: "entity" | "relation";
};

type FilterableItem =
  | { kind: "entity"; entity: EntitySummary }
  | { kind: "relation"; item: CleanupUnresolvedRelation; entity: EntitySummary };

const queueDefinitions: QueueDefinition[] = [
  { id: "missing-cover", label: msg`Missing Cover`, kind: "entity" },
  { id: "broken-asset", label: msg`Broken Assets`, kind: "entity" },
  { id: "missing-refs", label: msg`Unmatched`, kind: "entity" },
  { id: "isolated", label: msg`Unlinked entities`, kind: "entity" },
  { id: "unresolved-relations", label: msg`Unresolved Relations`, kind: "relation" },
  { id: "status-mismatch", label: msg`Status Mismatch`, kind: "entity" },
  { id: "duplicate-filename", label: msg`Duplicate Filenames`, kind: "entity" },
];

const assetQueueIds = new Set(["missing-cover", "broken-asset"]);

export function ReviewPage() {
  const { t, i18n } = useLingui();
  const { queueId } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const cleanup = useQuery(cleanupQueuesQuery());
  const config = useQuery(configQuery());
  const health = useQuery(healthQuery());
  const query = searchParams.get("q") ?? "";
  const selectedType = searchParams.get("type") ?? allEntityFilter;
  const selectedDate = searchParams.get("date") ?? allEntityFilter;
  const [queryInput, setQueryInput] = useState(query);

  useEffect(() => {
    setQueryInput(query);
  }, [query]);

  const { schedule: scheduleQueryUpdate, cancel: cancelQueryUpdate } = useDebouncedCallback(
    (value: string) => setFilter("q", value, allEntityFilter, true),
    180,
  );

  // Debounce the search box into the URL without giving the URL its own timer.
  useEffect(() => {
    if (queryInput === query) {
      cancelQueryUpdate();
      return;
    }
    scheduleQueryUpdate(queryInput.trim());
    return cancelQueryUpdate;
  }, [queryInput, query, searchParams, scheduleQueryUpdate, cancelQueryUpdate]);

  function setFilter(key: string, value: string, defaultValue = allEntityFilter, replace = false) {
    const next = new URLSearchParams(searchParams);
    if (!value || value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace });
  }

  const activeQueue = queueDefinitions.find((queue) => queue.id === queueId);
  const summaries = cleanup.data?.queues ?? [];
  const activeSummary = summaries.find((queue) => queue.id === activeQueue?.id);
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  // The full preference: the comparator derives the title language itself and
  // collates with the full tag (zh-Hant sorts as Traditional).
  const language = useLanguagePreference();
  const items = useMemo(
    () => (cleanup.data && activeQueue ? queueItems(cleanup.data, activeQueue) : []),
    [cleanup.data, activeQueue],
  );
  const itemEntities = useMemo(() => items.map((item) => item.entity), [items]);
  const typeOptions = useMemo(() => entityTypeOptions(itemEntities), [itemEntities]);
  const dateOptions = useMemo(() => entityDateOptions(itemEntities), [itemEntities]);
  const filteredItems = useMemo(
    () =>
      items
        .filter((item) => matchesQuery(item, query))
        .filter((item) => selectedType === allEntityFilter || item.entity.type === selectedType)
        .filter((item) => entityMatchesDate(item.entity, selectedDate))
        .sort((a, b) => compareItems(a, b, language, activeQueue?.id)),
    [items, query, selectedType, selectedDate, language, activeQueue?.id],
  );

  return (
    <AppFrame
      error={
        cleanup.error
          ? errorMessage(cleanup.error)
          : health.error
            ? errorMessage(health.error)
            : undefined
      }
    >
      <PageContainer width="wide">
        <header className="flex flex-wrap items-center justify-between gap-2">
          <h1 className="min-w-0 truncate text-base font-semibold">
            {activeQueue ? i18n._(activeQueue.label) : null}
          </h1>
          {activeSummary ? <ProgressPill summary={activeSummary} /> : null}
        </header>

        {cleanup.isPending ? (
          <Placeholder><Trans>Loading…</Trans></Placeholder>
        ) : null}

        {cleanup.data && (!activeQueue || assetQueueIds.has(activeQueue.id)) ? (
          <AssetDownloadPanel />
        ) : null}

        {health.data && !activeQueue ? <LibraryIssuesPanel health={health.data} /> : null}

        {cleanup.data && !activeQueue ? <ReviewOverview summaries={summaries} /> : null}

        {cleanup.data && activeQueue ? (
          <>
            <section className="flex flex-col gap-3 rounded-md border px-3 py-3">
              <div className="grid grid-cols-1 gap-2 lg:grid-cols-[minmax(220px,1fr)_repeat(2,auto)]">
                <div className="flex min-w-0 items-center gap-2">
                  <SearchIcon className="text-muted-foreground" />
                  <Input
                    value={queryInput}
                    onChange={(event) => setQueryInput(event.target.value)}
                    placeholder={t`Search queue`}
                  />
                </div>
                <Select value={selectedType} onChange={(event) => setFilter("type", event.target.value)}>
                  <option value={allEntityFilter}>{t`All types`}</option>
                  {typeOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label} ({option.count})
                    </option>
                  ))}
                </Select>
                <Select value={selectedDate} onChange={(event) => setFilter("date", event.target.value)}>
                  <option value={allEntityFilter}>{t`All dates`}</option>
                  <option value="dated">{t`Has date`}</option>
                  <option value="undated">{t`No date`}</option>
                  {dateOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label} ({option.count})
                    </option>
                  ))}
                </Select>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <Badge variant="secondary">
                  <Plural value={filteredItems.length} one="# shown" other="# shown" />
                </Badge>
                <span>
                  <Plural value={items.length} one="# total queue item" other="# total queue items" />
                </span>
              </div>
            </section>

            {filteredItems.length === 0 ? (
              <Placeholder>
                <Trans>No queue items match the current filters</Trans>
              </Placeholder>
            ) : (
              <section className="rounded-md border">
                <header className="flex items-center gap-2 border-b px-3 py-2">
                  <h2 className="text-sm font-semibold">{i18n._(activeQueue.label)}</h2>
                  <Badge variant="secondary">{filteredItems.length}</Badge>
                </header>
                <div>
                  {filteredItems.map((item) =>
                    item.kind === "entity" ? (
                      <CleanupEntityRow
                        key={item.entity.id}
                        entity={item.entity}
                        labelsByType={labelsByType}
                        showBasename={activeQueue.id === "duplicate-filename"}
                      />
                    ) : (
                      <UnresolvedRelationRow
                        key={`${item.item.relation.sourceId}-${item.item.relation.field}-${item.item.relation.targetTitle}`}
                        item={item.item}
                        labelsByType={labelsByType}
                      />
                    ),
                  )}
                </div>
              </section>
            )}
          </>
        ) : null}
      </PageContainer>
    </AppFrame>
  );
}

function LibraryIssuesPanel({ health }: { health: HealthResponse }) {
  const shownCount = health.diagnostics.length;

  return (
    <section className="rounded-md border">
      <header className="flex items-start justify-between gap-3 border-b px-3 py-3">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold"><Trans>Library Issues</Trans></h2>
          <p className="mt-1 text-xs text-muted-foreground">
            <Trans>Files the library could not parse cleanly.</Trans>
          </p>
        </div>
        <Badge variant={health.diagnosticCount > 0 ? "secondary" : "outline"}>
          <Plural value={health.diagnosticCount} one="# issue" other="# issues" />
        </Badge>
      </header>

      {health.diagnosticCount === 0 ? (
        <p className="px-3 py-3 text-sm text-muted-foreground">
          <Trans>No parsing issues found.</Trans>
        </p>
      ) : (
        <>
          <div className="divide-y">
            {health.diagnostics.map((diagnostic, index) => (
              <div
                key={`${diagnostic.path}-${diagnostic.kind}-${index}`}
                className="min-w-0 px-3 py-3"
              >
                <div className="flex min-w-0 flex-wrap items-start justify-between gap-2">
                  <code className="min-w-0 break-all text-xs font-medium">{diagnostic.path}</code>
                  <Badge variant="outline">
                    {diagnostic.kind === "frontmatter" ? (
                      <Trans>Frontmatter</Trans>
                    ) : diagnostic.kind === "file" ? (
                      <Trans>File</Trans>
                    ) : (
                      diagnostic.kind
                    )}
                  </Badge>
                </div>
                <p className="mt-1 break-words text-xs text-muted-foreground">
                  {diagnostic.message}
                </p>
              </div>
            ))}
          </div>
          {health.diagnosticCount > shownCount ? (
            <p className="border-t px-3 py-2 text-xs text-muted-foreground">
              <Trans>
                Showing {shownCount} of {health.diagnosticCount} issues.
              </Trans>
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}

function ReviewOverview({ summaries }: { summaries: CleanupQueueSummary[] }) {
  const { i18n } = useLingui();
  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
      {queueDefinitions.map((definition) => {
        const summary = summaries.find((item) => item.id === definition.id);
        if (!summary) return null;
        return (
          <Link
            key={definition.id}
            to={`/review/${definition.id}`}
            className="flex min-h-36 flex-col gap-3 rounded-md border p-3 transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h2 className="truncate text-sm font-semibold">{i18n._(definition.label)}</h2>
                <p className="mt-1 text-xs text-muted-foreground">
                  <Plural value={summary.remaining} one="# remaining" other="# remaining" />
                </p>
              </div>
              <ArrowRightIcon className="text-muted-foreground" />
            </div>
            <ProgressBar summary={summary} />
            <div className="mt-auto flex flex-wrap gap-2 text-xs text-muted-foreground">
              <Badge variant="secondary">
                <Plural value={completeCount(summary)} one="# complete" other="# complete" />
              </Badge>
              <Badge variant="outline">
                <Plural value={summary.total} one="# total" other="# total" />
              </Badge>
            </div>
          </Link>
        );
      })}
    </div>
  );
}

function CleanupEntityRow({
  entity,
  labelsByType,
  showBasename = false,
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  showBasename?: boolean;
}) {
  return (
    <div className="min-w-0 border-b px-3 py-2 last:border-b-0">
      <EntitySummaryCell entity={entity} labelsByType={labelsByType} showBasename={showBasename} />
    </div>
  );
}

function UnresolvedRelationRow({
  item,
  labelsByType,
}: {
  item: CleanupUnresolvedRelation;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <div className="min-w-0 border-b px-3 py-2 last:border-b-0">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Badge variant="outline">
            {entityFieldLabel(labelsByType, item.source.type, item.relation.field)}
          </Badge>
          {item.relation.targetType ? <Badge variant="secondary">{item.relation.targetType}</Badge> : null}
          <span className="min-w-0 truncate text-sm font-medium">{item.relation.targetTitle}</span>
        </div>
        <div className="mt-1">
          <EntitySummaryCell entity={item.source} compact labelsByType={labelsByType} />
        </div>
      </div>
    </div>
  );
}

function EntitySummaryCell({
  entity,
  compact = false,
  labelsByType,
  showBasename = false,
}: {
  entity: EntitySummary;
  compact?: boolean;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  showBasename?: boolean;
}) {
  const language = useTitleLanguage();
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <Link
          to={`/entities/${encodeURIComponent(entity.id)}`}
          className={cn("min-w-0 truncate font-medium hover:underline", compact ? "text-xs" : "text-sm")}
        >
          <EntityTitle entity={entity} language={language} />
        </Link>
        <Badge variant="outline">{entity.typeLabel}</Badge>
        {showBasename ? <Badge variant="secondary" className="font-mono">{entity.basename}</Badge> : null}
      </div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-muted-foreground">
        <EntityDateList entity={entity} compact labelsByType={labelsByType} />
      </div>
      {!compact && entity.summary ? (
        <div className="mt-1 line-clamp-1 text-xs text-muted-foreground">{entity.summary}</div>
      ) : null}
    </div>
  );
}

function ProgressPill({ summary }: { summary: CleanupQueueSummary }) {
  const formatNumber = useNumberFormat();
  return (
    <div className="min-w-48 rounded-md border px-3 py-2">
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-muted-foreground"><Trans>Progress</Trans></span>
        <span className="tabular-nums">
          {formatNumber(completeCount(summary))} / {formatNumber(summary.total)}
        </span>
      </div>
      <ProgressBar summary={summary} />
    </div>
  );
}

function ProgressBar({ summary }: { summary: CleanupQueueSummary }) {
  const percent = summary.total > 0 ? Math.round((completeCount(summary) / summary.total) * 100) : 100;
  return (
    <div className="mt-2 h-2 rounded-sm bg-muted">
      <div className="h-2 rounded-sm bg-emerald-500" style={{ width: `${percent}%` }} />
    </div>
  );
}

function completeCount(summary: CleanupQueueSummary) {
  return Math.max(0, summary.total - summary.remaining);
}

function queueItems(data: CleanupQueuesResponse, queue: QueueDefinition): FilterableItem[] {
  if (queue.id === "missing-cover") {
    return data.missingCover.map((entity) => ({ kind: "entity", entity }));
  }
  if (queue.id === "missing-refs") {
    return data.missingExternalRefs.map((entity) => ({ kind: "entity", entity }));
  }
  if (queue.id === "isolated") {
    return data.isolated.map((entity) => ({ kind: "entity", entity }));
  }
  if (queue.id === "broken-asset") {
    return data.brokenAssets.map((entity) => ({ kind: "entity", entity }));
  }
  if (queue.id === "status-mismatch") {
    return data.statusMismatch.map((entity) => ({ kind: "entity", entity }));
  }
  if (queue.id === "duplicate-filename") {
    return data.duplicateFilenames.map((entity) => ({ kind: "entity", entity }));
  }
  return data.unresolvedRelations.map((item) => ({ kind: "relation", item, entity: item.source }));
}

function matchesQuery(item: FilterableItem, query: string) {
  const extraValues: string[] = [];
  if (item.kind === "relation") {
    extraValues.push(
      item.item.relation.field,
      item.item.relation.targetTitle,
      item.item.relation.targetType ?? "",
    );
  }
  return entityMatchesQuery(item.entity, query, extraValues);
}

function compareItems(a: FilterableItem, b: FilterableItem, language: string, queueId?: string) {
  // The duplicate-filename queue is about the colliding names themselves, so sort
  // by basename first — this keeps each cluster of collisions adjacent, then
  // orders within a cluster by type/title.
  if (queueId === "duplicate-filename") {
    const byName = a.entity.basename.localeCompare(b.entity.basename, undefined, { sensitivity: "base" });
    if (byName !== 0) return byName;
  }
  return compareEntitiesByTypeThenTitle(a.entity, b.entity, language);
}
