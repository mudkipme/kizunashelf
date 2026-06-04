import { useEffect, useMemo, useState, type ReactNode } from "react";
import { getRelationTarget } from "@kizunashelf/api-contract";
import { ArrowLeftIcon, ExternalLinkIcon, ListIcon, NetworkIcon, SearchIcon } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { AppFrame } from "@/components/layout/app-frame";
import { RelationLocalGraph } from "@/components/relations/relation-local-graph";
import { RelationSourceRow } from "@/components/relations/relation-source-row";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { EntitySummary, RelationTargetResponse } from "@/types/api";

type TargetState = {
  data?: RelationTargetResponse;
  loading: boolean;
  error?: string;
};

type TargetView = "list" | "graph";

const allFilter = "all";
const noStatusFilter = "__none";

export function RelationTargetPage() {
  const { field = "", target = "" } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const [state, setState] = useState<TargetState>({ loading: true });
  const query = searchParams.get("q") ?? "";
  const selectedType = searchParams.get("type") ?? allFilter;
  const selectedStatus = searchParams.get("status") ?? allFilter;
  const selectedDate = searchParams.get("date") ?? allFilter;
  const view = readView(searchParams.get("view"));
  const [queryInput, setQueryInput] = useState(query);

  useEffect(() => {
    if (!field || !target) return;
    void loadTarget();
  }, [field, target]);

  useEffect(() => {
    setQueryInput(query);
  }, [query]);

  useEffect(() => {
    if (queryInput === query) return;
    const timeout = window.setTimeout(() => {
      setFilter("q", queryInput.trim(), allFilter, true);
    }, 180);
    return () => window.clearTimeout(timeout);
  }, [queryInput, query, searchParams]);

  async function loadTarget() {
    setState({ loading: true });
    try {
      const data = await getRelationTarget(field, target, undefined, apiFetch);
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  function setFilter(key: string, value: string, defaultValue = allFilter, replace = false) {
    const next = new URLSearchParams(searchParams);
    if (!value || value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace });
  }

  const relationTarget = state.data?.target;
  const sourceItems = useMemo(() => flattenSources(state.data), [state.data]);
  const entitySourceTypeLabels = useMemo(
    () => new Set(sourceItems.map((entity) => entity.typeLabel)),
    [sourceItems],
  );
  const nonEntitySourceTypes = useMemo(
    () => relationTarget?.sourceTypes.filter((type) => !entitySourceTypeLabels.has(type.name)) ?? [],
    [entitySourceTypeLabels, relationTarget],
  );
  const nonEntityLinkCount = useMemo(
    () => nonEntitySourceTypes.reduce((total, type) => total + type.count, 0),
    [nonEntitySourceTypes],
  );
  const typeOptions = useMemo(() => sourceTypeOptions(sourceItems), [sourceItems]);
  const statusOptions = useMemo(() => sourceStatusOptions(sourceItems), [sourceItems]);
  const dateOptions = useMemo(() => sourceDateOptions(sourceItems), [sourceItems]);
  const filteredSources = useMemo(
    () =>
      sourceItems
        .filter((entity) => matchesQuery(entity, query))
        .filter((entity) => selectedType === allFilter || entity.type === selectedType)
        .filter((entity) => matchesStatus(entity, selectedStatus))
        .filter((entity) => matchesDate(entity, selectedDate))
        .sort(compareSources),
    [sourceItems, query, selectedType, selectedStatus, selectedDate],
  );
  const filteredGroups = useMemo(() => groupSourcesByType(filteredSources), [filteredSources]);

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start gap-3">
          <Button asChild variant="ghost" size="sm">
            <Link to={`/relations/${encodeURIComponent(field)}`}>
              <ArrowLeftIcon data-icon="inline-start" />
              {field}
            </Link>
          </Button>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">{field}</Badge>
              {relationTarget?.targetTypeLabel ? (
                <Badge variant="outline">{relationTarget.targetTypeLabel}</Badge>
              ) : null}
              {relationTarget ? <Badge variant="secondary">{relationTarget.count} links</Badge> : null}
            </div>
            <h1 className="mt-2 break-words text-xl font-semibold leading-snug">
              {relationTarget?.targetTitle ?? target}
            </h1>
            {relationTarget?.sourceTypes.length ? (
              <div className="mt-2 flex flex-wrap gap-1">
                {relationTarget.sourceTypes.map((type) => (
                  <Badge key={type.name} variant="outline">
                    {type.name} {type.count}
                  </Badge>
                ))}
              </div>
            ) : null}
          </div>
          {relationTarget?.targetId ? (
            <Button asChild variant="outline" size="sm">
              <Link to={`/entities/${encodeURIComponent(relationTarget.targetId)}`}>
                Entity
                <ExternalLinkIcon data-icon="inline-end" />
              </Link>
            </Button>
          ) : null}
        </header>

        {state.data ? (
          <section className="flex flex-col gap-3 rounded-md border px-3 py-3">
            <div className="grid grid-cols-1 gap-2 lg:grid-cols-[minmax(220px,1fr)_repeat(3,auto)_auto]">
              <div className="flex min-w-0 items-center gap-2">
                <SearchIcon className="text-muted-foreground" />
                <Input
                  value={queryInput}
                  onChange={(event) => setQueryInput(event.target.value)}
                  placeholder="Search linked sources"
                />
              </div>
              <Select value={selectedType} onChange={(event) => setFilter("type", event.target.value)}>
                <option value={allFilter}>All types</option>
                {typeOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label} ({option.count})
                  </option>
                ))}
              </Select>
              <Select value={selectedStatus} onChange={(event) => setFilter("status", event.target.value)}>
                <option value={allFilter}>All statuses</option>
                {statusOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label} ({option.count})
                  </option>
                ))}
              </Select>
              <Select value={selectedDate} onChange={(event) => setFilter("date", event.target.value)}>
                <option value={allFilter}>All dates</option>
                <option value="dated">Has date</option>
                <option value="undated">No date</option>
                {dateOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label} ({option.count})
                  </option>
                ))}
              </Select>
              <div className="flex items-center gap-2">
                <ViewButton active={view === "list"} onClick={() => setFilter("view", "list", "list")}>
                  <ListIcon data-icon="inline-start" />
                  List
                </ViewButton>
                <ViewButton active={view === "graph"} onClick={() => setFilter("view", "graph", "list")}>
                  <NetworkIcon data-icon="inline-start" />
                  Graph
                </ViewButton>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <Badge variant="secondary">{filteredSources.length} entity sources shown</Badge>
              <span>{sourceItems.length} total entity sources</span>
              {nonEntityLinkCount > 0 ? (
                <span>
                  {nonEntityLinkCount} links from {formatSourceTypeList(nonEntitySourceTypes)}
                </span>
              ) : null}
            </div>
          </section>
        ) : null}

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {!state.loading && !state.data ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Relation target not found
          </div>
        ) : null}

        {!state.loading && state.data && sourceItems.length === 0 && nonEntityLinkCount > 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            This target is linked from {formatSourceTypeList(nonEntitySourceTypes)}, which are counted in the map
            but do not have entity rows.
          </div>
        ) : null}

        {!state.loading && state.data && sourceItems.length > 0 && filteredSources.length === 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            No linked entity sources match the current filters
          </div>
        ) : null}

        {state.data && relationTarget && filteredSources.length > 0 && view === "graph" ? (
          <RelationLocalGraph target={relationTarget} sources={filteredSources} />
        ) : null}

        {state.data && filteredSources.length > 0 && view === "list"
          ? filteredGroups.map((group) => (
              <section key={group.typeLabel} className="rounded-md border">
                <header className="flex items-center gap-2 border-b px-3 py-2">
                  <h2 className="text-sm font-semibold">{group.typeLabel}</h2>
                  <Badge variant="secondary">{group.items.length}</Badge>
                </header>
                <div>
                  {group.items.map((entity) => (
                    <RelationSourceRow key={entity.id} entity={entity} />
                  ))}
                </div>
              </section>
            ))
          : null}
      </div>
    </AppFrame>
  );
}

function ViewButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Button
      type="button"
      variant={active ? "secondary" : "outline"}
      size="sm"
      className={cn("min-w-20", active && "border-transparent")}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

function readView(value: string | null): TargetView {
  return value === "graph" ? "graph" : "list";
}

function flattenSources(data: RelationTargetResponse | undefined) {
  const seen = new Set<string>();
  const entities: EntitySummary[] = [];
  for (const group of data?.groups ?? []) {
    for (const entity of group.items) {
      if (seen.has(entity.id)) continue;
      seen.add(entity.id);
      entities.push(entity);
    }
  }
  return entities;
}

function sourceTypeOptions(entities: EntitySummary[]) {
  const counts = countBy(entities, (entity) => entity.type);
  const labels = new Map(entities.map((entity) => [entity.type, entity.typeLabel]));
  return [...counts.entries()]
    .map(([value, count]) => ({ value, label: labels.get(value) ?? value, count }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

function sourceStatusOptions(entities: EntitySummary[]) {
  const counts = countBy(entities, (entity) => entity.status ?? noStatusFilter);
  return [...counts.entries()]
    .map(([value, count]) => ({ value, label: value === noStatusFilter ? "No status" : value, count }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

function sourceDateOptions(entities: EntitySummary[]) {
  const counts = new Map<string, number>();
  for (const entity of entities) {
    const years = new Set(entity.dates.map((date) => dateYear(date.value)).filter(Boolean));
    for (const year of years) counts.set(year, (counts.get(year) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([year, count]) => ({ value: `year:${year}`, label: year, count }))
    .sort((a, b) => b.label.localeCompare(a.label));
}

function matchesQuery(entity: EntitySummary, query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  const values = [
    entity.title,
    entity.subtitle ?? "",
    entity.summary ?? "",
    entity.basename,
    ...Object.values(entity.titles),
  ];
  return values.some((value) => value.toLowerCase().includes(normalized));
}

function matchesStatus(entity: EntitySummary, selectedStatus: string) {
  if (selectedStatus === allFilter) return true;
  if (selectedStatus === noStatusFilter) return !entity.status;
  return entity.status === selectedStatus;
}

function matchesDate(entity: EntitySummary, selectedDate: string) {
  if (selectedDate === allFilter) return true;
  if (selectedDate === "dated") return entity.dates.length > 0;
  if (selectedDate === "undated") return entity.dates.length === 0;
  if (selectedDate.startsWith("year:")) {
    const year = selectedDate.slice("year:".length);
    return entity.dates.some((date) => dateYear(date.value) === year);
  }
  return true;
}

function dateYear(value: string) {
  const match = /^(\d{4})/.exec(value);
  return match?.[1] ?? "";
}

function groupSourcesByType(entities: EntitySummary[]) {
  const groups = new Map<string, EntitySummary[]>();
  for (const entity of entities) {
    const items = groups.get(entity.typeLabel) ?? [];
    items.push(entity);
    groups.set(entity.typeLabel, items);
  }
  return [...groups.entries()].map(([typeLabel, items]) => ({ typeLabel, items }));
}

function compareSources(a: EntitySummary, b: EntitySummary) {
  if (a.typeLabel !== b.typeLabel) return a.typeLabel.localeCompare(b.typeLabel);
  return a.title.localeCompare(b.title);
}

function formatSourceTypeList(sourceTypes: { name: string; count: number }[]) {
  return sourceTypes.map((type) => type.name).join(", ");
}

function countBy<T>(items: T[], key: (item: T) => string) {
  const counts = new Map<string, number>();
  for (const item of items) {
    const value = key(item);
    counts.set(value, (counts.get(value) ?? 0) + 1);
  }
  return counts;
}
