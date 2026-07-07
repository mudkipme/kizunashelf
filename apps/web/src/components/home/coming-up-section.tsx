import { useMemo } from "react";
import type { MessageDescriptor } from "@lingui/core";
import { msg, plural } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ImageIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { configQuery, upcomingQuery } from "@/api/queries";
import { AssetImage } from "@/components/assets/asset-image";
import { SectionHeader } from "@/components/home/section-header";
import { todayLocal } from "@/lib/date";
import { useTitleLanguage } from "@/lib/language";
import { EntityTitle } from "@/components/entities/entity-title";
import { coverTypeIds, entityFieldLabel, fieldLabelsByType } from "@/lib/type-config";
import type { ActivityItem } from "@/types/api";

type FieldLabels = ReadonlyMap<string, ReadonlyMap<string, string>>;

type Translate = ReturnType<typeof useLingui>["t"];

/// The number of upcoming items to surface on Home — the soonest first, since the
/// endpoint returns them ascending. Deeper browsing is the Activity "Up next" mode.
const MAX_ITEMS = 18;

/// The Home "Coming up" widget: future release/planning dates + scheduled episodes
/// (from `/api/upcoming`), grouped by urgency with a countdown. It's set apart from
/// the configured sections as a bordered panel — a glanceable dashboard widget
/// rather than a content shelf — but shares their header. Self-hides when there's
/// nothing ahead, so an empty vault stays clean.
export function ComingUpSection() {
  const { t, i18n } = useLingui();
  const today = todayLocal();
  const upcoming = useQuery(upcomingQuery({ today, months: 6 }));
  const config = useQuery(configQuery());
  const labels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  // Type ids that declare an image/imageList field — only those get a cover slot.
  const coverTypes = useMemo(() => coverTypeIds(config.data?.types), [config.data]);
  const groups = useMemo(
    () => groupByUrgency(keepNearestPerEntity(upcoming.data?.items ?? []), today),
    [upcoming.data, today],
  );

  if (upcoming.isPending || groups.every((group) => group.items.length === 0)) return null;

  const total = groups.reduce((sum, group) => sum + group.items.length, 0);

  return (
    <section className="rounded-xl border bg-muted/30 p-4">
      <SectionHeader
        title={t`Coming up`}
        count={total}
        viewHref="/activity?mode=up-next"
        viewLabel={t`View all`}
      />
      <div className="flex flex-col gap-4">
        {groups.map((group) =>
          group.items.length ? (
            <div key={group.id} className="flex flex-col gap-1.5">
              <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {i18n._(group.label)}
              </h3>
              <div className="grid grid-cols-1 gap-1 sm:grid-cols-2 xl:grid-cols-3">
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
  const { t } = useLingui();
  const language = useTitleLanguage();
  const days = daysUntil(item.date, today);
  return (
    <Link
      to={`/entities/${encodeURIComponent(item.entity.id)}`}
      className="flex min-w-0 items-center gap-3 rounded-lg px-2 py-1.5 transition-colors hover:bg-accent"
    >
      {/* Cover slot only for types that declare an image field; a placeholder fills
          it when this entity has no cover value. `bg-background` so the tile reads
          against the panel. */}
      {hasCover ? (
        <div className="flex h-14 w-10 shrink-0 items-center justify-center overflow-hidden rounded bg-background">
          <AssetImage
            src={item.entity.image}
            className="size-full object-cover"
            fallback={<ImageIcon className="size-4 text-muted-foreground" />}
          />
        </div>
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col">
        <EntityTitle
          as="span"
          entity={item.entity}
          language={language}
          className="truncate text-sm font-medium"
        />
        <span className="truncate text-xs text-muted-foreground">{sourceLabel(item, labels, t)}</span>
      </div>
      <div className="shrink-0 text-right">
        <div className="text-xs font-medium">{countdown(days, t)}</div>
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

function countdown(days: number, t: Translate): string {
  if (days <= 0) return t`Today`;
  if (days === 1) return t`Tomorrow`;
  if (days <= 7) return t`in ${plural(days, { one: "# day", other: "# days" })}`;
  if (days <= 30) {
    const weeks = Math.round(days / 7);
    return t`in ${weeks} wk`;
  }
  const months = Math.round(days / 30);
  return t`in ${months} mo`;
}

/// The human label for what's happening on this date — the date field's schema
/// label ("Release date") or the episodes airing ("Episodes 12").
function sourceLabel(item: ActivityItem, labels: FieldLabels, t: Translate): string {
  const entry = item.entries[0];
  if (!entry) return "";
  if (entry.source === "episode") {
    const keys = (entry.episodes ?? []).map((episode) => episode.key || episode.title).filter(Boolean);
    const heading = entry.heading || t`Episode`;
    const keyList = keys.join(", ");
    return keys.length ? t`${heading} ${keyList}` : heading;
  }
  if (entry.source === "taxonomy" && entry.dateField) {
    return entityFieldLabel(labels, item.entity.type, entry.dateField);
  }
  return "";
}

/// Collapse repeated entities to their soonest occurrence. An entity with several
/// scheduled episodes (each airing on its own date) would otherwise fill the widget
/// with near-duplicate rows. The endpoint returns items ascending by date, so the
/// first time we see an entity is its nearest one; later occurrences are dropped
/// here — the Activity "Up next" page shows a separate query and still lists them all.
function keepNearestPerEntity(items: ActivityItem[]): ActivityItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.entity.id)) return false;
    seen.add(item.entity.id);
    return true;
  });
}

function groupByUrgency(items: ActivityItem[], today: string) {
  const groups: { id: string; label: MessageDescriptor; items: ActivityItem[] }[] = [
    { id: "week", label: msg`This week`, items: [] },
    { id: "month", label: msg`This month`, items: [] },
    { id: "later", label: msg`Later`, items: [] },
  ];
  for (const item of items.slice(0, MAX_ITEMS)) {
    const days = daysUntil(item.date, today);
    if (days <= 7) groups[0].items.push(item);
    else if (days <= 31) groups[1].items.push(item);
    else groups[2].items.push(item);
  }
  return groups;
}
