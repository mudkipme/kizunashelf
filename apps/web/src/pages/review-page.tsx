import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowRightIcon, SearchIcon } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { cleanupQueuesQuery, configQuery } from "@/api/queries";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  allEntityFilter,
  compareEntitiesByTypeThenTitle,
  entityDateOptions,
  entityMatchesDate,
  entityMatchesQuery,
  entityTypeOptions,
} from "@/lib/entity-filters";
import { entityFieldLabel, fieldLabelsByType } from "@/lib/type-config";
import { cn } from "@/lib/utils";
import type {
  CleanupQueueSummary,
  CleanupQueuesResponse,
  CleanupUnresolvedRelation,
  EntitySummary,
} from "@/types/api";

type QueueDefinition = {
  id: string;
  label: string;
  kind: "entity" | "relation";
};

type FilterableItem =
  | { kind: "entity"; entity: EntitySummary }
  | { kind: "relation"; item: CleanupUnresolvedRelation; entity: EntitySummary };

const queueDefinitions: QueueDefinition[] = [
  { id: "missing-cover", label: "Missing Cover", kind: "entity" },
  { id: "missing-refs", label: "Missing External Refs", kind: "entity" },
  { id: "isolated", label: "Isolated Nodes", kind: "entity" },
  { id: "unresolved-relations", label: "Unresolved Relations", kind: "relation" },
];

export function ReviewPage() {
  const { queueId } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const cleanup = useQuery(cleanupQueuesQuery());
  const config = useQuery(configQuery());
  const query = searchParams.get("q") ?? "";
  const selectedType = searchParams.get("type") ?? allEntityFilter;
  const selectedDate = searchParams.get("date") ?? allEntityFilter;
  const [queryInput, setQueryInput] = useState(query);

  useEffect(() => {
    setQueryInput(query);
  }, [query]);

  useEffect(() => {
    if (queryInput === query) return;
    const timeout = window.setTimeout(() => {
      setFilter("q", queryInput.trim(), allEntityFilter, true);
    }, 180);
    return () => window.clearTimeout(timeout);
  }, [queryInput, query, searchParams]);

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
        .sort(compareItems),
    [items, query, selectedType, selectedDate],
  );

  return (
    <AppFrame error={cleanup.error || config.error ? errorMessage(cleanup.error ?? config.error) : undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="text-xl font-semibold">{activeQueue?.label ?? "Metadata Review"}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {cleanup.isPending || config.isPending
                ? "Loading"
                : cleanup.data
                  ? `Updated ${cleanup.data.generatedAt.slice(0, 10)}`
                  : "No review data"}
            </p>
          </div>
          {activeSummary ? <ProgressPill summary={activeSummary} /> : null}
        </header>

        {cleanup.isPending || config.isPending ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : null}

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
                    placeholder="Search queue"
                  />
                </div>
                <Select value={selectedType} onChange={(event) => setFilter("type", event.target.value)}>
                  <option value={allEntityFilter}>All types</option>
                  {typeOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label} ({option.count})
                    </option>
                  ))}
                </Select>
                <Select value={selectedDate} onChange={(event) => setFilter("date", event.target.value)}>
                  <option value={allEntityFilter}>All dates</option>
                  <option value="dated">Has date</option>
                  <option value="undated">No date</option>
                  {dateOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label} ({option.count})
                    </option>
                  ))}
                </Select>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <Badge variant="secondary">{filteredItems.length} shown</Badge>
                <span>{items.length} total queue items</span>
              </div>
            </section>

            {filteredItems.length === 0 ? (
              <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
                No queue items match the current filters
              </div>
            ) : (
              <section className="rounded-md border">
                <header className="flex items-center gap-2 border-b px-3 py-2">
                  <h2 className="text-sm font-semibold">{activeQueue.label}</h2>
                  <Badge variant="secondary">{filteredItems.length}</Badge>
                </header>
                <div>
                  {filteredItems.map((item) =>
                    item.kind === "entity" ? (
                      <CleanupEntityRow
                        key={item.entity.id}
                        entity={item.entity}
                        labelsByType={labelsByType}
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
      </div>
    </AppFrame>
  );
}

function ReviewOverview({ summaries }: { summaries: CleanupQueueSummary[] }) {
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
                <h2 className="truncate text-sm font-semibold">{definition.label}</h2>
                <p className="mt-1 text-xs text-muted-foreground">
                  {summary.remaining.toLocaleString()} remaining
                </p>
              </div>
              <ArrowRightIcon className="text-muted-foreground" />
            </div>
            <ProgressBar summary={summary} />
            <div className="mt-auto flex flex-wrap gap-2 text-xs text-muted-foreground">
              <Badge variant="secondary">{completeCount(summary).toLocaleString()} complete</Badge>
              <Badge variant="outline">{summary.total.toLocaleString()} total</Badge>
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
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <div className="min-w-0 border-b px-3 py-2 last:border-b-0">
      <EntitySummaryCell entity={entity} labelsByType={labelsByType} />
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
}: {
  entity: EntitySummary;
  compact?: boolean;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <Link
          to={`/entities/${encodeURIComponent(entity.id)}`}
          className={cn("min-w-0 truncate font-medium hover:underline", compact ? "text-xs" : "text-sm")}
        >
          {entity.title}
        </Link>
        <Badge variant="outline">{entity.typeLabel}</Badge>
      </div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-muted-foreground">
        <EntityDateList entity={entity} compact labelsByType={labelsByType} />
        <span className="min-w-0 truncate">{entity.path}</span>
      </div>
      {!compact && entity.summary ? (
        <div className="mt-1 line-clamp-1 text-xs text-muted-foreground">{entity.summary}</div>
      ) : null}
    </div>
  );
}

function ProgressPill({ summary }: { summary: CleanupQueueSummary }) {
  return (
    <div className="min-w-48 rounded-md border px-3 py-2">
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-muted-foreground">Progress</span>
        <span className="tabular-nums">
          {completeCount(summary).toLocaleString()} / {summary.total.toLocaleString()}
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
      <div className="h-2 rounded-sm bg-primary" style={{ width: `${percent}%` }} />
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

function compareItems(a: FilterableItem, b: FilterableItem) {
  return compareEntitiesByTypeThenTitle(a.entity, b.entity);
}
