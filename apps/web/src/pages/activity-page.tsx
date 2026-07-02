import { useEffect, useMemo, useRef } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { ImageIcon } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { activityFeedQuery, configQuery } from "@/api/queries";
import { AssetImage } from "@/components/assets/asset-image";
import { StatusBadge } from "@/components/entities/status-badge";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { todayLocal } from "@/lib/date";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { coverTypeIds, entityFieldLabel, fieldLabelsByType } from "@/lib/type-config";
import type { ActivityEntry, ActivityItem } from "@/types/api";

type FieldLabels = ReadonlyMap<string, ReadonlyMap<string, string>>;

const sources = [
  { value: "all", label: "All sources" },
  { value: "taxonomy", label: "Dates & episodes" },
  { value: "daily-note", label: "Daily notes" },
] as const;

// Recent leads — it's the everyday "what happened" view; Up next is already
// surfaced on Home; Catch up is the reverse-chron "released, still on my list"
// backlog; All is the full ledger. Recent is also the default (below).
const modes = [
  { value: "recent", label: "Recent" },
  { value: "up-next", label: "Up next" },
  { value: "catch-up", label: "Catch up" },
  { value: "all", label: "All" },
] as const;

const DEFAULT_MODE = "recent";

const dateRoleLabels: Record<string, string> = {
  started: "Started",
  completed: "Completed",
  planning: "Planned",
  event: "Event",
};

export function ActivityPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const type = searchParams.get("type") ?? "all";
  const source = readSource(searchParams.get("source"));
  const mode = readMode(searchParams.get("mode"));

  const config = useQuery(configQuery());
  const feed = useInfiniteQuery(
    activityFeedQuery({
      months: 1,
      ...(type !== "all" ? { type } : {}),
      ...(source !== "all" ? { source } : {}),
      // Always sent: the API defaults to "all", but the page defaults to "recent".
      mode,
    }),
  );

  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  // Type ids that declare an image/imageList field — only those get a cover slot.
  const coverTypes = useMemo(() => coverTypeIds(config.data?.types), [config.data]);
  const days = useMemo(
    () => groupByDay(feed.data?.pages.flatMap((page) => page.items) ?? []),
    [feed.data],
  );

  // Auto-load the next month when the sentinel scrolls into view.
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = sentinel.current;
    if (!node || !feed.hasNextPage) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries[0]?.isIntersecting && !feed.isFetchingNextPage) {
        void feed.fetchNextPage();
      }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [feed.hasNextPage, feed.isFetchingNextPage, feed]);

  function setParam(key: string, value: string, defaultValue = "all") {
    const next = new URLSearchParams(searchParams);
    if (value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace: true });
  }

  return (
    <AppFrame error={feed.error ? errorMessage(feed.error) : undefined}>
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-4 sm:p-6">
        <header className="flex flex-wrap items-center gap-3">
          <h1 className="text-lg font-semibold">Activity</h1>
          <div className="flex flex-wrap gap-1">
            {modes.map((item) => (
              <Button
                key={item.value}
                type="button"
                variant={mode === item.value ? "secondary" : "outline"}
                size="sm"
                onClick={() => setParam("mode", item.value, DEFAULT_MODE)}
              >
                {item.label}
              </Button>
            ))}
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <Select
              value={type}
              onChange={(event) => setParam("type", event.target.value)}
              aria-label="Filter by type"
            >
              <option value="all">All types</option>
              {config.data?.types.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </Select>
            <Select
              value={source}
              onChange={(event) => setParam("source", event.target.value)}
              aria-label="Filter by source"
            >
              {sources.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </Select>
          </div>
        </header>

        {feed.isPending ? (
          <p className="text-sm text-muted-foreground">Loading activity…</p>
        ) : days.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No activity yet. Dated entities, episode air/completion dates, and daily-note mentions
            show up here.
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            {days.map((day) => (
              <section key={day.date} className="flex flex-col gap-2">
                <h2 className="sticky top-0 z-10 bg-background/90 py-1 text-sm font-medium text-muted-foreground backdrop-blur">
                  {formatDay(day.date)}
                </h2>
                <div className="flex flex-col gap-2">
                  {day.items.map((item) => (
                    <ActivityCard
                      key={`${item.date}-${item.entity.id}`}
                      item={item}
                      labels={fieldLabels}
                      hasCover={coverTypes.has(item.entity.type)}
                    />
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}

        <div ref={sentinel} />
        {feed.hasNextPage ? (
          <Button
            type="button"
            variant="outline"
            className="self-center"
            onClick={() => void feed.fetchNextPage()}
            disabled={feed.isFetchingNextPage}
          >
            {feed.isFetchingNextPage ? "Loading…" : "Load more"}
          </Button>
        ) : null}
      </div>
    </AppFrame>
  );
}

function ActivityCard({
  item,
  labels,
  hasCover,
}: {
  item: ActivityItem;
  labels: FieldLabels;
  hasCover: boolean;
}) {
  const language = useTitleLanguage();
  const href = `/entities/${encodeURIComponent(item.entity.id)}`;
  return (
    <article className="flex items-start gap-3 rounded-md border p-3">
      {/* Cover only for types with an image field; a placeholder fills the slot
          when this entity has no cover value. */}
      {hasCover ? (
        <Link to={href} className="shrink-0">
          <div className="flex h-16 w-12 items-center justify-center overflow-hidden rounded bg-muted">
            <AssetImage
              src={item.entity.image}
              className="size-full object-cover"
              fallback={<ImageIcon className="size-4 text-muted-foreground" />}
            />
          </div>
        </Link>
      ) : null}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Badge variant="outline">{item.entity.typeLabel}</Badge>
          <StatusBadge status={item.entity.status} />
          <Link to={href} className="min-w-0 break-words text-sm font-medium hover:underline">
            {entityTitle(item.entity, language)}
          </Link>
        </div>
        <div className="mt-2 flex flex-col gap-2">
          {item.entries.map((entry, index) => (
            <ActivityEntryRow
              key={`${entry.source}-${index}`}
              entry={entry}
              entityType={item.entity.type}
              labels={labels}
              // A past event still marked "planning" was likely attended but never
              // checked off — flag it so the diary nudges a status update.
              missed={
                entry.source === "taxonomy" &&
                entry.role === "event" &&
                item.date < todayLocal() &&
                item.entity.status?.canonical === "planning"
              }
            />
          ))}
        </div>
      </div>
    </article>
  );
}

function ActivityEntryRow({
  entry,
  entityType,
  labels,
  missed = false,
}: {
  entry: ActivityEntry;
  entityType: string;
  labels: FieldLabels;
  missed?: boolean;
}) {
  if (entry.source === "taxonomy") {
    const role = entry.role ? dateRoleLabels[entry.role] ?? entry.role : "Date";
    const field = entry.dateField ? entityFieldLabel(labels, entityType, entry.dateField) : "date";
    return (
      <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        <span>
          <span className="font-medium text-foreground">{role}</span>
          {" · "}
          {field}
          {entry.rawDate ? `: ${entry.rawDate}` : ""}
        </span>
        {missed ? (
          <Badge
            variant="outline"
            className="border-transparent bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300"
          >
            Missed?
          </Badge>
        ) : null}
      </div>
    );
  }

  if (entry.source === "episode") {
    const verb = entry.episodeRole === "completed" ? "✅ Completed" : "📅 Scheduled";
    const items = (entry.episodes ?? []).map((episode) => episode.key || episode.title).join(", ");
    return (
      <div className="text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{verb}</span>
        {" · "}
        {entry.heading || "Episodes"}
        {items ? `: ${items}` : ""}
      </div>
    );
  }

  const snippets = entry.snippets ?? [];
  return (
    <div className="flex flex-col gap-2">
      {snippets.map((snippet) => (
        <figure
          key={`${snippet.line}-${snippet.text}`}
          className="rounded-md bg-muted p-2"
        >
          {snippet.heading ? (
            <figcaption className="mb-1 text-xs font-medium text-muted-foreground">
              {snippet.heading}
            </figcaption>
          ) : null}
          <blockquote className="break-words text-sm leading-6">{snippet.text}</blockquote>
        </figure>
      ))}
    </div>
  );
}

function groupByDay(items: ActivityItem[]): { date: string; items: ActivityItem[] }[] {
  const days: { date: string; items: ActivityItem[] }[] = [];
  for (const item of items) {
    const last = days[days.length - 1];
    if (last && last.date === item.date) last.items.push(item);
    else days.push({ date: item.date, items: [item] });
  }
  return days;
}

function readSource(value: string | null): "all" | "taxonomy" | "daily-note" {
  return value === "taxonomy" || value === "daily-note" ? value : "all";
}

function readMode(value: string | null): "all" | "recent" | "up-next" | "catch-up" {
  return value === "all" || value === "up-next" || value === "catch-up" ? value : DEFAULT_MODE;
}

function formatDay(date: string): string {
  const [year, month, day] = date.split("-").map(Number);
  if (!year || !month || !day) return date;
  return new Date(year, month - 1, day).toLocaleDateString(undefined, {
    weekday: "short",
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
