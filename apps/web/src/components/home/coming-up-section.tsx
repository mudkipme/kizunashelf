import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowRightIcon, ImageIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { configQuery, upcomingQuery } from "@/api/queries";
import { AssetImage } from "@/components/assets/asset-image";
import { Button } from "@/components/ui/button";
import { todayLocal } from "@/lib/date";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { coverTypeIds, entityFieldLabel, fieldLabelsByType } from "@/lib/type-config";
import type { ActivityItem } from "@/types/api";

type FieldLabels = ReadonlyMap<string, ReadonlyMap<string, string>>;

/// The number of upcoming items to surface on Home — the soonest first, since the
/// endpoint returns them ascending. Deeper browsing is the Activity "Up next" mode.
const MAX_ITEMS = 18;

/// The Home "Coming up" widget: future release/planning dates + scheduled episodes
/// (from `/api/upcoming`), grouped by urgency with a countdown. Self-hides when
/// there's nothing ahead, so it adds no clutter to an empty vault. Localizes titles
/// per the content-language setting and resolves the date's source label from the
/// schema.
export function ComingUpSection() {
  const today = todayLocal();
  const upcoming = useQuery(upcomingQuery({ today, months: 6 }));
  const config = useQuery(configQuery());
  const labels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  // Type ids that declare an image/imageList field — only those get a cover slot.
  const coverTypes = useMemo(() => coverTypeIds(config.data?.types), [config.data]);
  const groups = useMemo(
    () => groupByUrgency(upcoming.data?.items ?? [], today),
    [upcoming.data, today],
  );

  if (upcoming.isPending || groups.every((group) => group.items.length === 0)) return null;

  return (
    <section className="min-w-0">
      <header className="flex min-h-12 items-center gap-3">
        <h2 className="text-sm font-semibold">Coming up</h2>
        <Button asChild variant="ghost" size="sm" className="ml-auto">
          <Link to="/activity?mode=up-next">
            View all
            <ArrowRightIcon />
          </Link>
        </Button>
      </header>

      <div className="flex flex-col gap-4">
        {groups.map((group) =>
          group.items.length ? (
            <div key={group.label} className="flex flex-col gap-2">
              <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {group.label}
              </h3>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3">
                {group.items.map((item) => (
                  <ComingUpCard
                    key={`${item.date}-${item.entity.id}`}
                    item={item}
                    today={today}
                    labels={labels}
                    hasCover={coverTypes.has(item.entity.type)}
                  />
                ))}
              </div>
            </div>
          ) : null,
        )}
      </div>
    </section>
  );
}

function ComingUpCard({
  item,
  today,
  labels,
  hasCover,
}: {
  item: ActivityItem;
  today: string;
  labels: FieldLabels;
  hasCover: boolean;
}) {
  const language = useTitleLanguage();
  const days = daysUntil(item.date, today);
  return (
    <Link
      to={`/entities/${encodeURIComponent(item.entity.id)}`}
      className="flex min-w-0 items-center gap-3 rounded-md border p-2 transition-colors hover:bg-accent"
    >
      {/* Cover slot only for types that declare an image field; a placeholder
          fills it when this entity has no cover value. */}
      {hasCover ? (
        <div className="flex h-14 w-10 shrink-0 items-center justify-center overflow-hidden rounded bg-muted">
          <AssetImage
            src={item.entity.image}
            className="size-full object-cover"
            fallback={<ImageIcon className="size-4 text-muted-foreground" />}
          />
        </div>
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-medium">{entityTitle(item.entity, language)}</span>
        <span className="truncate text-xs text-muted-foreground">
          {sourceLabel(item, labels)}
        </span>
      </div>
      <div className="shrink-0 text-right">
        <div className="text-xs font-medium">{countdown(days)}</div>
        <div className="text-[11px] tabular-nums text-muted-foreground">{item.date}</div>
      </div>
    </Link>
  );
}

/// Whole-day difference between two `YYYY-MM-DD` local dates.
function daysUntil(date: string, today: string): number {
  const target = Date.parse(`${date}T00:00:00`);
  const now = Date.parse(`${today}T00:00:00`);
  if (Number.isNaN(target) || Number.isNaN(now)) return 0;
  return Math.round((target - now) / 86_400_000);
}

function countdown(days: number): string {
  if (days <= 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days <= 7) return `in ${days} days`;
  if (days <= 30) return `in ${Math.round(days / 7)} wk`;
  return `in ${Math.round(days / 30)} mo`;
}

/// The human label for what's happening on this date — the date field's schema
/// label ("Release date") or the episodes airing ("Episodes 12").
function sourceLabel(item: ActivityItem, labels: FieldLabels): string {
  const entry = item.entries[0];
  if (!entry) return "";
  if (entry.source === "episode") {
    const keys = (entry.episodes ?? []).map((episode) => episode.key || episode.title).filter(Boolean);
    const heading = entry.heading || "Episode";
    return keys.length ? `${heading} ${keys.join(", ")}` : heading;
  }
  if (entry.source === "taxonomy" && entry.dateField) {
    return entityFieldLabel(labels, item.entity.type, entry.dateField);
  }
  return "";
}

function groupByUrgency(items: ActivityItem[], today: string) {
  const groups = [
    { label: "This week", items: [] as ActivityItem[] },
    { label: "This month", items: [] as ActivityItem[] },
    { label: "Later", items: [] as ActivityItem[] },
  ];
  for (const item of items.slice(0, MAX_ITEMS)) {
    const days = daysUntil(item.date, today);
    if (days <= 7) groups[0].items.push(item);
    else if (days <= 31) groups[1].items.push(item);
    else groups[2].items.push(item);
  }
  return groups;
}
