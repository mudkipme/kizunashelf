import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import type { EntitySummary } from "@/types/api";

export type PlanningMode = "year" | "seasons" | "planning";

type DatePoint = {
  entity: EntitySummary;
  field: string;
  value: string;
  year: number;
  month: number;
  sortKey: string;
  season?: SeasonKey;
};

type SeasonKey = "winter" | "spring" | "summer" | "autumn";

type PlanningViewsProps = {
  mode: PlanningMode;
  year: number;
  entities: EntitySummary[];
  dateRolesByType: Map<string, DateRoles>;
  loading: boolean;
  onOpenMonth: (month: number) => void;
};

type DateRoles = {
  planning?: string[];
  completed?: string[];
};

const monthNames = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

const seasons: Array<{ key: SeasonKey; label: string; months: string }> = [
  { key: "winter", label: "Winter", months: "Jan-Mar" },
  { key: "spring", label: "Spring", months: "Apr-Jun" },
  { key: "summer", label: "Summer", months: "Jul-Sep" },
  { key: "autumn", label: "Autumn", months: "Oct-Dec" },
];

export function CalendarPlanningViews({
  mode,
  year,
  entities,
  dateRolesByType,
  loading,
  onOpenMonth,
}: PlanningViewsProps) {
  const points = entityDatePoints(entities);

  if (loading) {
    return <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>;
  }

  if (mode === "year") {
    return <YearPlanningView year={year} points={points} onOpenMonth={onOpenMonth} />;
  }

  if (mode === "seasons") {
    return <SeasonPlanningView year={year} points={points} />;
  }

  return <PlanningBoard entities={entities} points={points} dateRolesByType={dateRolesByType} />;
}

export function countEntityDatePoints(entities: EntitySummary[]) {
  return entityDatePoints(entities).length;
}

function YearPlanningView({
  year,
  points,
  onOpenMonth,
}: {
  year: number;
  points: DatePoint[];
  onOpenMonth: (month: number) => void;
}) {
  const buckets = Array.from({ length: 12 }, (_, index) => {
    const month = index + 1;
    return uniqueByEntity(points.filter((point) => point.year === year && point.month === month));
  });
  const total = buckets.reduce((sum, bucket) => sum + bucket.length, 0);

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold">{year}</h2>
        <Badge variant="secondary">{total} dated entries</Badge>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {buckets.map((bucket, index) => {
          const month = index + 1;
          return (
            <button
              key={month}
              type="button"
              onClick={() => onOpenMonth(month)}
              className="flex min-h-36 flex-col gap-3 rounded-md border bg-background p-3 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm font-medium">{monthNames[index]}</span>
                <Badge variant={bucket.length > 0 ? "secondary" : "outline"}>{bucket.length}</Badge>
              </div>
              <div className="flex min-w-0 flex-col gap-2">
                {bucket.slice(0, 4).map((point) => (
                  <DatePointSummary key={`${point.entity.id}-${point.field}-${point.value}`} point={point} />
                ))}
                {bucket.length > 4 ? (
                  <span className="text-xs text-muted-foreground">+{bucket.length - 4}</span>
                ) : null}
              </div>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function SeasonPlanningView({ year, points }: { year: number; points: DatePoint[] }) {
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      {seasons.map((season) => {
        const bucket = uniqueByEntity(
          points.filter((point) => point.year === year && seasonForMonth(point.month) === season.key),
        );
        return (
          <section key={season.key} className="rounded-md border">
            <header className="flex items-center gap-2 border-b px-3 py-2">
              <h2 className="text-sm font-semibold">{season.label}</h2>
              <Badge variant="outline">{season.months}</Badge>
              <Badge variant="secondary" className="ml-auto">
                {bucket.length}
              </Badge>
            </header>
            <div className="flex flex-col">
              {bucket.length > 0 ? (
                bucket.map((point) => (
                  <PlanningEntityRow key={`${point.entity.id}-${point.field}-${point.value}`} point={point} />
                ))
              ) : (
                <div className="p-6 text-center text-sm text-muted-foreground">No entries</div>
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function PlanningBoard({
  entities,
  points,
  dateRolesByType,
}: {
  entities: EntitySummary[];
  points: DatePoint[];
  dateRolesByType: Map<string, DateRoles>;
}) {
  const today = todayKey();
  const futurePlanningEntityIds = new Set(
    points
      .filter((point) => point.sortKey >= today && hasDateRole(point, dateRolesByType, "planning"))
      .map((point) => point.entity.id),
  );
  const upcoming = uniqueByEntity(
    points
      .filter((point) => point.sortKey >= today && hasDateRole(point, dateRolesByType, "planning"))
      .sort(compareDatePointsAsc),
  ).slice(0, 12);
  const recentlyCompleted = uniqueByEntity(
    points
      .filter((point) => point.sortKey <= today && hasDateRole(point, dateRolesByType, "completed"))
      .sort(compareDatePointsDesc),
  ).slice(0, 12);
  const unscheduled = entities
    .filter((entity) => !futurePlanningEntityIds.has(entity.id))
    .sort(compareEntitiesAsc)
    .slice(0, 12);

  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
      <PlanningList title="Upcoming" count={upcoming.length} points={upcoming} />
      <PlanningList title="Recently Completed" count={recentlyCompleted.length} points={recentlyCompleted} />
      <EntityPlanningList title="Unscheduled" count={unscheduled.length} entities={unscheduled} />
    </div>
  );
}

function PlanningList({
  title,
  count,
  points,
}: {
  title: string;
  count: number;
  points: DatePoint[];
}) {
  return (
    <section className="rounded-md border">
      <header className="flex items-center gap-2 border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Badge variant="secondary" className="ml-auto">
          {count}
        </Badge>
      </header>
      <div className="flex flex-col md:max-h-[720px] md:overflow-auto">
        {points.length > 0 ? (
          points.map((point) => (
            <PlanningEntityRow key={`${point.entity.id}-${point.field}-${point.value}`} point={point} />
          ))
        ) : (
          <div className="p-6 text-center text-sm text-muted-foreground">No entries</div>
        )}
      </div>
    </section>
  );
}

function PlanningEntityRow({ point }: { point: DatePoint }) {
  return (
    <Link
      to={`/entities/${encodeURIComponent(point.entity.id)}`}
      className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <span className="min-w-0">
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="min-w-0 truncate text-sm font-medium">{point.entity.title}</span>
          <Badge variant="outline">{point.entity.typeLabel}</Badge>
        </span>
        {point.entity.summary ? (
          <span className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
            {point.entity.summary}
          </span>
        ) : null}
      </span>
      <span className="flex flex-col items-end gap-1 text-xs text-muted-foreground">
        <Badge variant="outline" className="font-normal">
          {point.field}
        </Badge>
        <span className="tabular-nums">{point.value}</span>
      </span>
    </Link>
  );
}

function EntityPlanningList({
  title,
  count,
  entities,
}: {
  title: string;
  count: number;
  entities: EntitySummary[];
}) {
  return (
    <section className="rounded-md border">
      <header className="flex items-center gap-2 border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Badge variant="secondary" className="ml-auto">
          {count}
        </Badge>
      </header>
      <div className="flex flex-col md:max-h-[720px] md:overflow-auto">
        {entities.length > 0 ? (
          entities.map((entity) => <PlanningEntitySummaryRow key={entity.id} entity={entity} />)
        ) : (
          <div className="p-6 text-center text-sm text-muted-foreground">No entries</div>
        )}
      </div>
    </section>
  );
}

function PlanningEntitySummaryRow({ entity }: { entity: EntitySummary }) {
  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="flex min-w-0 flex-col gap-1 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <span className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="min-w-0 truncate text-sm font-medium">{entity.title}</span>
        <Badge variant="outline">{entity.typeLabel}</Badge>
      </span>
      {entity.summary ? (
        <span className="line-clamp-2 text-xs leading-5 text-muted-foreground">{entity.summary}</span>
      ) : null}
    </Link>
  );
}

function DatePointSummary({ point }: { point: DatePoint }) {
  return (
    <span className="min-w-0">
      <span className="block truncate text-xs font-medium">{point.entity.title}</span>
      <span className="mt-0.5 flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
        <span className="truncate">{point.entity.typeLabel}</span>
        <span className="shrink-0 tabular-nums">{point.value}</span>
      </span>
    </span>
  );
}

function entityDatePoints(entities: EntitySummary[]) {
  return entities.flatMap((entity) =>
    entity.dates.flatMap((date) => {
      const parsed = parseDateValue(date.value);
      if (!parsed) return [];
      return [{ entity, field: date.field, value: date.value, ...parsed }];
    }),
  );
}

function parseDateValue(value: string): Omit<DatePoint, "entity" | "field" | "value"> | null {
  const iso = /^(?<year>\d{4})(?:-(?<month>\d{2})(?:-(?<day>\d{2}))?)?$/.exec(value);
  if (iso?.groups) {
    const year = Number(iso.groups.year);
    const month = Number(iso.groups.month ?? "1");
    const day = Number(iso.groups.day ?? "1");
    if (year >= 1970 && year <= 2100 && month >= 1 && month <= 12 && day >= 1 && day <= 31) {
      return {
        year,
        month,
        sortKey: `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`,
      };
    }
  }

  const season =
    /^(?:(?<yearPrefix>\d{4})年(?<seasonZh>春季|夏季|秋季|冬季)|(?<yearBefore>\d{4})\s*(?<seasonAfter>Spring|Summer|Autumn|Fall|Winter)|(?<seasonBefore>Spring|Summer|Autumn|Fall|Winter)\s+(?<yearAfter>\d{4}))$/i.exec(
      value.trim(),
    );
  if (season?.groups) {
    const year = Number(season.groups.yearPrefix ?? season.groups.yearBefore ?? season.groups.yearAfter);
    const seasonValue = season.groups.seasonZh ?? season.groups.seasonAfter ?? season.groups.seasonBefore;
    const seasonKey = seasonKeyFromValue(seasonValue);
    const month = seasonMonth(seasonKey);
    return {
      year,
      month,
      season: seasonKey,
      sortKey: `${year}-${String(month).padStart(2, "0")}-01`,
    };
  }

  return null;
}

function seasonKeyFromValue(value: string): SeasonKey {
  const normalized = value.trim().toLowerCase();
  if (value === "春季" || normalized === "spring") return "spring";
  if (value === "夏季" || normalized === "summer") return "summer";
  if (value === "秋季" || normalized === "autumn" || normalized === "fall") return "autumn";
  return "winter";
}

function seasonMonth(season: SeasonKey) {
  if (season === "spring") return 4;
  if (season === "summer") return 7;
  if (season === "autumn") return 10;
  return 1;
}

function seasonForMonth(month: number): SeasonKey {
  if (month >= 4 && month <= 6) return "spring";
  if (month >= 7 && month <= 9) return "summer";
  if (month >= 10 && month <= 12) return "autumn";
  return "winter";
}

function hasDateRole(
  point: DatePoint,
  dateRolesByType: Map<string, DateRoles>,
  role: keyof DateRoles,
) {
  return dateRolesByType.get(point.entity.type)?.[role]?.includes(point.field) ?? false;
}

function uniqueByEntity(points: DatePoint[]) {
  const seen = new Set<string>();
  return points.filter((point) => {
    if (seen.has(point.entity.id)) return false;
    seen.add(point.entity.id);
    return true;
  });
}

function compareDatePointsAsc(a: DatePoint, b: DatePoint) {
  if (a.sortKey !== b.sortKey) return a.sortKey.localeCompare(b.sortKey);
  return a.entity.title.localeCompare(b.entity.title);
}

function compareDatePointsDesc(a: DatePoint, b: DatePoint) {
  if (a.sortKey !== b.sortKey) return b.sortKey.localeCompare(a.sortKey);
  return a.entity.title.localeCompare(b.entity.title);
}

function compareEntitiesAsc(a: EntitySummary, b: EntitySummary) {
  if (a.typeLabel !== b.typeLabel) return a.typeLabel.localeCompare(b.typeLabel);
  return a.title.localeCompare(b.title);
}

function todayKey() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(
    date.getDate(),
  ).padStart(2, "0")}`;
}
