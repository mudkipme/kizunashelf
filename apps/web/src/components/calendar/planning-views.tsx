import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import type {
  CalendarPlanningDatePoint,
  CalendarPlanningResponse,
  EntitySummary,
} from "@/types/api";

export type PlanningMode = "year" | "seasons" | "planning";

type PlanningViewsProps = {
  mode: PlanningMode;
  data?: CalendarPlanningResponse;
  loading: boolean;
  onOpenMonth: (month: number) => void;
};

export function CalendarPlanningViews({
  mode,
  data,
  loading,
  onOpenMonth,
}: PlanningViewsProps) {
  if (loading) {
    return <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>;
  }

  if (!data) {
    return <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">No planning data</div>;
  }

  if (mode === "year") {
    return <YearPlanningView data={data} onOpenMonth={onOpenMonth} />;
  }

  if (mode === "seasons") {
    return <SeasonPlanningView data={data} />;
  }

  return <PlanningBoard data={data} />;
}

function YearPlanningView({
  data,
  onOpenMonth,
}: {
  data: CalendarPlanningResponse;
  onOpenMonth: (month: number) => void;
}) {
  const total = data.yearMonths.reduce((sum, bucket) => sum + bucket.entries.length, 0);

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold">{data.filters.year}</h2>
        <Badge variant="secondary">{total} dated entries</Badge>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {data.yearMonths.map((bucket) => {
          return (
            <button
              key={bucket.month}
              type="button"
              onClick={() => onOpenMonth(bucket.month)}
              className="flex min-h-36 flex-col gap-3 rounded-md border bg-background p-3 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm font-medium">{bucket.label}</span>
                <Badge variant={bucket.entries.length > 0 ? "secondary" : "outline"}>
                  {bucket.entries.length}
                </Badge>
              </div>
              <div className="flex min-w-0 flex-col gap-2">
                {bucket.entries.slice(0, 4).map((point) => (
                  <DatePointSummary key={`${point.entity.id}-${point.field}-${point.value}`} point={point} />
                ))}
                {bucket.entries.length > 4 ? (
                  <span className="text-xs text-muted-foreground">+{bucket.entries.length - 4}</span>
                ) : null}
              </div>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function SeasonPlanningView({
  data,
}: {
  data: CalendarPlanningResponse;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      {data.seasons.map((season) => (
        <section key={season.key} className="rounded-md border">
          <header className="flex items-center gap-2 border-b px-3 py-2">
            <h2 className="text-sm font-semibold">{season.label}</h2>
            <Badge variant="outline">{season.months}</Badge>
            <Badge variant="secondary" className="ml-auto">
              {season.entries.length}
            </Badge>
          </header>
          <div className="flex flex-col">
            {season.entries.length > 0 ? (
              season.entries.map((point) => (
                <PlanningEntityRow
                  key={`${point.entity.id}-${point.field}-${point.value}`}
                  point={point}
                />
              ))
            ) : (
              <div className="p-6 text-center text-sm text-muted-foreground">No entries</div>
            )}
          </div>
        </section>
      ))}
    </div>
  );
}

function PlanningBoard({
  data,
}: {
  data: CalendarPlanningResponse;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
      <PlanningList title="Upcoming" points={data.board.upcoming} />
      <PlanningList
        title="Recently Completed"
        points={data.board.recentlyCompleted}
      />
      <EntityPlanningList title="Unscheduled" entities={data.board.unscheduled} />
    </div>
  );
}

function PlanningList({
  title,
  points,
}: {
  title: string;
  points: CalendarPlanningDatePoint[];
}) {
  return (
    <section className="rounded-md border">
      <header className="flex items-center gap-2 border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Badge variant="secondary" className="ml-auto">
          {points.length}
        </Badge>
      </header>
      <div className="flex flex-col md:max-h-[720px] md:overflow-auto">
        {points.length > 0 ? (
          points.map((point) => (
            <PlanningEntityRow
              key={`${point.entity.id}-${point.field}-${point.value}`}
              point={point}
            />
          ))
        ) : (
          <div className="p-6 text-center text-sm text-muted-foreground">No entries</div>
        )}
      </div>
    </section>
  );
}

function PlanningEntityRow({
  point,
}: {
  point: CalendarPlanningDatePoint;
}) {
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
          {point.fieldLabel}
        </Badge>
        <span className="tabular-nums">{point.value}</span>
      </span>
    </Link>
  );
}

function EntityPlanningList({
  title,
  entities,
}: {
  title: string;
  entities: EntitySummary[];
}) {
  return (
    <section className="rounded-md border">
      <header className="flex items-center gap-2 border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Badge variant="secondary" className="ml-auto">
          {entities.length}
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

function DatePointSummary({ point }: { point: CalendarPlanningDatePoint }) {
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
