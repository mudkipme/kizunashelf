import { useMemo, useState } from "react";
import { Link } from "react-router-dom";

import { Select } from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { AnalyticsActivity } from "@/types/api";

const MONTH_INITIALS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];
const MONTH_NAMES = [
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

// Discrete intensity levels (index 0 = empty). Listed as literals so Tailwind's
// JIT keeps these classes in the build.
const CELL_LEVELS = ["bg-muted", "bg-primary/30", "bg-primary/55", "bg-primary/80", "bg-primary"];

function cellLevel(count: number, max: number) {
  if (count <= 0) return 0;
  const ratio = count / max;
  if (ratio <= 0.25) return 1;
  if (ratio <= 0.5) return 2;
  if (ratio <= 0.75) return 3;
  return 4;
}

type Row = { year: number; total: number; months: number[] };

/**
 * Year × month heatmap of dated entities, filterable by type, with a per-year
 * total bar. Answers "how much this year vs past years" at a glance. The web
 * client mirrors the calendar's date derivation; this is the cross-year compare
 * view, not a browse-by-period view.
 */
export function ActivityHeatmap({ activity }: { activity: AnalyticsActivity }) {
  const [selectedType, setSelectedType] = useState("all");

  const rows: Row[] = useMemo(() => {
    if (selectedType === "all") {
      return activity.years.map((year) => ({
        year: year.year,
        total: year.total,
        months: year.months ?? [],
      }));
    }
    return activity.years.map((year) => {
      const match = year.byType?.find((item) => item.typeId === selectedType);
      return {
        year: year.year,
        total: match?.total ?? 0,
        months: match?.months ?? [],
      };
    });
  }, [activity.years, selectedType]);

  // Global scales so a dark cell/long bar means the same across every year.
  const maxMonth = Math.max(1, ...rows.flatMap((row) => row.months));
  const maxTotal = Math.max(1, ...rows.map((row) => row.total));
  const selectedTotal =
    selectedType === "all"
      ? activity.totalDated
      : (activity.types.find((item) => item.id === selectedType)?.total ?? 0);

  if (activity.years.length === 0) {
    return <p className="text-xs text-muted-foreground">No dated entities yet.</p>;
  }

  // A cell opens that month in the calendar, carrying the active type filter.
  function calendarHref(year: number, monthIndex: number) {
    const params = new URLSearchParams({ year: String(year), month: String(monthIndex + 1) });
    if (selectedType !== "all") params.set("type", selectedType);
    return `/calendar?${params.toString()}`;
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Select
          value={selectedType}
          onChange={(event) => setSelectedType(event.target.value)}
          aria-label="Filter by type"
        >
          <option value="all">All types</option>
          {activity.types.map((type) => (
            <option key={type.id} value={type.id}>
              {type.label} ({type.total.toLocaleString()})
            </option>
          ))}
        </Select>
        <span className="text-xs tabular-nums text-muted-foreground">
          {selectedTotal.toLocaleString()} dated
        </span>
      </div>

      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
          <span className="w-10 shrink-0" />
          <div className="grid flex-1 grid-cols-12 gap-1 sm:w-[360px] sm:flex-none">
            {MONTH_INITIALS.map((month, index) => (
              <span key={index} className="text-center">
                {month}
              </span>
            ))}
          </div>
          <span className="w-24 shrink-0 sm:w-auto sm:flex-1" />
        </div>

        {rows.map((row) => {
          const barWidth = `${Math.max(2, Math.round((row.total / maxTotal) * 100))}%`;
          return (
            <div key={row.year} className="flex items-center gap-2">
              <span className="w-10 shrink-0 text-xs font-medium tabular-nums">{row.year}</span>
              <div className="grid flex-1 grid-cols-12 gap-1 sm:w-[360px] sm:flex-none">
                {MONTH_INITIALS.map((_, index) => {
                  const count = row.months[index] ?? 0;
                  return (
                    <Link
                      key={index}
                      to={calendarHref(row.year, index)}
                      title={`${MONTH_NAMES[index]} ${row.year}: ${count}`}
                      aria-label={`${MONTH_NAMES[index]} ${row.year}: ${count} — open in calendar`}
                      className={cn(
                        "block aspect-square rounded-sm transition-[outline] hover:outline hover:outline-1 hover:outline-offset-1 hover:outline-ring focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring",
                        CELL_LEVELS[cellLevel(count, maxMonth)],
                      )}
                    />
                  );
                })}
              </div>
              <div className="flex w-24 shrink-0 items-center gap-2 sm:w-auto sm:flex-1">
                <div className="h-2 flex-1 rounded-sm bg-muted">
                  <div className="h-2 rounded-sm bg-primary" style={{ width: barWidth }} />
                </div>
                <span className="w-8 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
                  {row.total.toLocaleString()}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      <div className="flex items-center justify-end gap-1 text-[10px] text-muted-foreground">
        <span>less</span>
        {CELL_LEVELS.map((level, index) => (
          <span key={index} className={cn("size-3 rounded-sm", level)} />
        ))}
        <span>more</span>
      </div>
    </div>
  );
}
