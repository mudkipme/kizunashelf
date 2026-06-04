import { useEffect, useMemo, useState } from "react";
import { getCleanupQueues } from "@kizunashelf/api-contract";
import {
  ArrowLeftIcon,
  ArrowRightIcon,
  BarChart3Icon,
  ExternalLinkIcon,
  SearchIcon,
} from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  allEntityFilter,
  compareEntitiesByTypeThenTitle,
  entityDateOptions,
  entityMatchesDate,
  entityMatchesQuery,
  entityMatchesStatus,
  entityStatusOptions,
  entityTypeOptions,
} from "@/lib/entity-filters";
import { relationFieldHref } from "@/lib/relations";
import { cn } from "@/lib/utils";
import type {
  CleanupQueueSummary,
  CleanupQueuesResponse,
  CleanupUnresolvedRelation,
  EntitySummary,
} from "@/types/api";

type CleanupState = {
  data?: CleanupQueuesResponse;
  loading: boolean;
  error?: string;
};

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
  { id: "missing-summary", label: "Missing Summary", kind: "entity" },
  { id: "isolated", label: "Isolated Nodes", kind: "entity" },
  { id: "unresolved-relations", label: "Unresolved Relations", kind: "relation" },
];

export function ReviewPage() {
  const { queueId } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const [state, setState] = useState<CleanupState>({ loading: true });
  const query = searchParams.get("q") ?? "";
  const selectedType = searchParams.get("type") ?? allEntityFilter;
  const selectedStatus = searchParams.get("status") ?? allEntityFilter;
  const selectedDate = searchParams.get("date") ?? allEntityFilter;
  const [queryInput, setQueryInput] = useState(query);

  useEffect(() => {
    void loadQueues();
  }, []);

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

  async function loadQueues() {
    setState({ loading: true });
    try {
      const data = await getCleanupQueues(undefined, apiFetch);
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  function setFilter(key: string, value: string, defaultValue = allEntityFilter, replace = false) {
    const next = new URLSearchParams(searchParams);
    if (!value || value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace });
  }

  const activeQueue = queueDefinitions.find((queue) => queue.id === queueId);
  const summaries = state.data?.queues ?? [];
  const activeSummary = summaries.find((queue) => queue.id === activeQueue?.id);
  const items = useMemo(
    () => (state.data && activeQueue ? queueItems(state.data, activeQueue) : []),
    [state.data, activeQueue],
  );
  const itemEntities = useMemo(() => items.map((item) => item.entity), [items]);
  const typeOptions = useMemo(() => entityTypeOptions(itemEntities), [itemEntities]);
  const statusOptions = useMemo(() => entityStatusOptions(itemEntities), [itemEntities]);
  const dateOptions = useMemo(() => entityDateOptions(itemEntities), [itemEntities]);
  const filteredItems = useMemo(
    () =>
      items
        .filter((item) => matchesQuery(item, query))
        .filter((item) => selectedType === allEntityFilter || item.entity.type === selectedType)
        .filter((item) => entityMatchesStatus(item.entity, selectedStatus))
        .filter((item) => entityMatchesDate(item.entity, selectedDate))
        .sort(compareItems),
    [items, query, selectedType, selectedStatus, selectedDate],
  );

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              {activeQueue ? (
                <Button asChild variant="ghost" size="sm">
                  <Link to="/review">
                    <ArrowLeftIcon data-icon="inline-start" />
                    Review
                  </Link>
                </Button>
              ) : (
                <Badge variant="secondary">Review</Badge>
              )}
              <Button asChild variant="ghost" size="sm">
                <Link to="/statistics">
                  <BarChart3Icon data-icon="inline-start" />
                  Statistics
                </Link>
              </Button>
            </div>
            <h1 className="mt-2 text-xl font-semibold">
              {activeQueue?.label ?? "Metadata Review"}
            </h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {state.loading
                ? "Loading"
                : state.data
                  ? `Updated ${state.data.generatedAt.slice(0, 10)}`
                  : "No review data"}
            </p>
          </div>
          {activeSummary ? <ProgressPill summary={activeSummary} /> : null}
        </header>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : null}

        {state.data && !activeQueue ? <ReviewOverview summaries={summaries} /> : null}

        {state.data && activeQueue ? (
          <>
            <section className="flex flex-col gap-3 rounded-md border px-3 py-3">
              <div className="grid grid-cols-1 gap-2 lg:grid-cols-[minmax(220px,1fr)_repeat(3,auto)]">
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
                <Select
                  value={selectedStatus}
                  onChange={(event) => setFilter("status", event.target.value)}
                >
                  <option value={allEntityFilter}>All statuses</option>
                  {statusOptions.map((option) => (
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
                      <CleanupEntityRow key={item.entity.id} entity={item.entity} />
                    ) : (
                      <UnresolvedRelationRow
                        key={`${item.item.relation.sourceId}-${item.item.relation.field}-${item.item.relation.targetTitle}`}
                        item={item.item}
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

function CleanupEntityRow({ entity }: { entity: EntitySummary }) {
  return (
    <div className="grid min-w-0 gap-3 border-b px-3 py-2 last:border-b-0 md:grid-cols-[minmax(0,1fr)_auto]">
      <EntitySummaryCell entity={entity} />
      <div className="flex items-center justify-end gap-2">
        <Button asChild variant="ghost" size="sm">
          <Link to={`/library?q=${encodeURIComponent(entity.path)}&type=${encodeURIComponent(entity.type)}`}>
            Source
          </Link>
        </Button>
        <Button asChild variant="outline" size="sm">
          <Link to={`/entities/${encodeURIComponent(entity.id)}`}>
            Entity
            <ExternalLinkIcon data-icon="inline-end" />
          </Link>
        </Button>
      </div>
    </div>
  );
}

function UnresolvedRelationRow({ item }: { item: CleanupUnresolvedRelation }) {
  return (
    <div className="grid min-w-0 gap-3 border-b px-3 py-2 last:border-b-0 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Badge variant="outline">{item.relation.field}</Badge>
          {item.relation.targetType ? <Badge variant="secondary">{item.relation.targetType}</Badge> : null}
          <span className="min-w-0 truncate text-sm font-medium">{item.relation.targetTitle}</span>
        </div>
        <div className="mt-1">
          <EntitySummaryCell entity={item.source} compact />
        </div>
      </div>
      <div className="flex items-center justify-end gap-2">
        <Button asChild variant="ghost" size="sm">
          <Link to={relationFieldHref(item.relation.field)}>Field</Link>
        </Button>
        <Button asChild variant="outline" size="sm">
          <Link to={`/entities/${encodeURIComponent(item.source.id)}`}>
            Source
            <ExternalLinkIcon data-icon="inline-end" />
          </Link>
        </Button>
      </div>
    </div>
  );
}

function EntitySummaryCell({
  entity,
  compact = false,
}: {
  entity: EntitySummary;
  compact?: boolean;
}) {
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <span className={cn("min-w-0 truncate font-medium", compact ? "text-xs" : "text-sm")}>
          {entity.title}
        </span>
        <Badge variant="outline">{entity.typeLabel}</Badge>
        {entity.status ? <Badge variant="secondary">{entity.status}</Badge> : null}
      </div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-muted-foreground">
        <EntityDateList entity={entity} compact />
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
  if (queue.id === "missing-summary") {
    return data.missingSummary.map((entity) => ({ kind: "entity", entity }));
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
