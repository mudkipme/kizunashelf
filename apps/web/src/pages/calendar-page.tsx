import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { CalendarDaysIcon, ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { calendarPlanningQuery, calendarQuery, configQuery } from "@/api/queries";
import { CalendarDayCell } from "@/components/calendar/calendar-day-cell";
import { CalendarEntryItem } from "@/components/calendar/calendar-entry-item";
import {
  CalendarPlanningViews,
  type PlanningMode,
} from "@/components/calendar/planning-views";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { fieldLabelsByType } from "@/lib/type-config";
import { cn } from "@/lib/utils";
import type {
  CalendarDay,
} from "@/types/api";

type CalendarMode = "month" | PlanningMode;
const weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function CalendarPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const now = new Date();
  const year = readYear(searchParams.get("year"), now.getFullYear());
  const month = readMonth(searchParams.get("month"), now.getMonth() + 1);
  const selectedDate = searchParams.get("date") ?? todayInMonth(year, month, now);
  const source = readSource(searchParams.get("source"));
  const type = searchParams.get("type") ?? "all";
  const mode = readMode(searchParams.get("view"));
  const calendarParams = {
    year,
    month,
    source,
    ...(type !== "all" ? { type } : {}),
  };
  const planningParams = {
    year,
    ...(type !== "all" ? { type } : {}),
  };
  const config = useQuery(configQuery());
  const calendar = useQuery({
    ...calendarQuery(calendarParams),
    enabled: mode === "month",
  });
  const planning = useQuery({
    ...calendarPlanningQuery(planningParams),
    enabled: mode !== "month",
  });

  const gridDays = useMemo(() => monthGridDays(calendar.data?.days ?? [], year, month), [
    calendar.data,
    year,
    month,
  ]);
  const selectedDay =
    calendar.data?.days.find((day) => day.date === selectedDate) ?? calendar.data?.days[0];
  const planningTypeOptions = planning.data?.typeOptions ?? [];
  const effectiveType =
    mode !== "month" &&
    type !== "all" &&
    planning.data &&
    !planningTypeOptions.some((item) => item.id === type)
      ? "all"
      : type;
  const activeLoading = mode === "month" ? calendar.isPending : planning.isPending;
  // Config only provides type labels for the filter dropdown; a config failure
  // shouldn't blank the calendar. The calendar/planning query is load-bearing.
  const activeError = mode === "month" ? calendar.error : planning.error;
  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);

  function setParam(key: string, value: string, defaultValue?: string, options?: { replace?: boolean }) {
    const next = new URLSearchParams(searchParams);
    if (defaultValue !== undefined && value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace: options?.replace ?? false });
  }

  function setMode(nextMode: CalendarMode) {
    setParam("view", nextMode, "month");
  }

  function moveMonth(delta: number) {
    const nextDate = new Date(Date.UTC(year, month - 1 + delta, 1));
    const next = new URLSearchParams(searchParams);
    next.set("year", String(nextDate.getUTCFullYear()));
    next.set("month", String(nextDate.getUTCMonth() + 1));
    next.delete("date");
    setSearchParams(next);
  }

  function moveYear(delta: number) {
    const next = new URLSearchParams(searchParams);
    next.set("year", String(year + delta));
    setSearchParams(next);
  }

  function movePeriod(delta: number) {
    if (mode === "month") moveMonth(delta);
    else moveYear(delta);
  }

  function goToday() {
    const today = new Date();
    const next = new URLSearchParams(searchParams);
    next.set("year", String(today.getFullYear()));
    next.set("month", String(today.getMonth() + 1));
    next.set("date", todayInMonth(today.getFullYear(), today.getMonth() + 1, today));
    setSearchParams(next, { replace: true });
  }

  function openMonth(nextMonth: number) {
    const next = new URLSearchParams(searchParams);
    next.delete("view");
    next.set("year", String(year));
    next.set("month", String(nextMonth));
    next.delete("date");
    setSearchParams(next);
  }

  return (
    <AppFrame error={activeError ? errorMessage(activeError) : undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <h1 className="flex items-center gap-2 text-base font-semibold">
              <CalendarDaysIcon />
              Calendar
            </h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {mode === "month"
                ? activeLoading
                  ? "Loading"
                  : calendar.data
                    ? `${calendar.data.totals.entries} entries across ${calendar.data.totals.daysWithEntries} days`
                    : "No calendar data"
                : activeLoading
                  ? "Loading"
                  : planning.data
                    ? `${planning.data.totals.entities} entities - ${planning.data.totals.datedEntries} dated entries`
                    : "No planning data"}
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" onClick={() => movePeriod(-1)}>
              <ChevronLeftIcon data-icon="inline-start" />
              Prev
            </Button>
            <Button variant="outline" size="sm" onClick={goToday}>
              Today
            </Button>
            <Button variant="outline" size="sm" onClick={() => movePeriod(1)}>
              Next
              <ChevronRightIcon data-icon="inline-end" />
            </Button>
          </div>
        </header>

        <div className="flex flex-wrap gap-2">
          <ModeButton active={mode === "month"} onClick={() => setMode("month")}>
            Month
          </ModeButton>
          <ModeButton active={mode === "year"} onClick={() => setMode("year")}>
            Year
          </ModeButton>
          <ModeButton active={mode === "seasons"} onClick={() => setMode("seasons")}>
            Seasons
          </ModeButton>
          <ModeButton active={mode === "planning"} onClick={() => setMode("planning")}>
            Planning
          </ModeButton>
        </div>

        <section className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2">
          <div className="text-sm font-medium">{mode === "month" ? monthTitle(year, month) : year}</div>
          {mode === "month" && calendar.data ? (
            <div className="flex flex-wrap gap-1">
              <Badge variant="outline">Taxonomy {calendar.data.totals.taxonomy}</Badge>
              <Badge variant="outline">Items {calendar.data.totals.episodes}</Badge>
              <Badge variant="outline">Daily Notes {calendar.data.totals.dailyNotes}</Badge>
            </div>
          ) : null}
          {mode !== "month" ? <Badge variant="outline">Taxonomy dates</Badge> : null}
          <div className="ml-auto flex flex-wrap items-center gap-2">
            {mode === "month" ? (
              <Select value={source} onChange={(event) => setParam("source", event.target.value, "all")}>
                <option value="all">All sources</option>
                <option value="taxonomy">Taxonomy dates</option>
                <option value="daily-note">Daily note mentions</option>
              </Select>
            ) : null}
            <Select value={effectiveType} onChange={(event) => setParam("type", event.target.value, "all")}>
              <option value="all">All types</option>
              {(mode === "month" ? config.data?.types ?? [] : planningTypeOptions).map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </Select>
          </div>
        </section>

        {mode === "month" ? (
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
            <section className="overflow-hidden rounded-md border">
              <div className="grid grid-cols-7 border-b bg-muted text-xs font-medium text-muted-foreground">
                {weekdays.map((weekday) => (
                  <div key={weekday} className="px-2 py-2">
                    {weekday}
                  </div>
                ))}
              </div>
              <div className="grid grid-cols-7">
                {gridDays.map((day, index) =>
                  day ? (
                    <CalendarDayCell
                      key={day.date}
                      day={day}
                      selected={day.date === selectedDay?.date}
                      onSelect={(date) => setParam("date", date, undefined, { replace: true })}
                    />
                  ) : (
                    <div key={`blank-${index}`} className="min-h-28 border-b border-r bg-muted/30" />
                  ),
                )}
              </div>
            </section>

            <aside className="min-w-0 rounded-md border">
              <header className="border-b px-3 py-2">
                <h2 className="text-sm font-semibold">{selectedDay?.date ?? "No date selected"}</h2>
                <p className="mt-1 text-xs text-muted-foreground">
                  {selectedDay ? `${selectedDay.entries.length} entries` : "No entries"}
                </p>
              </header>
              <div className="flex max-h-[720px] flex-col gap-2 overflow-auto p-3">
                {selectedDay && selectedDay.entries.length > 0 ? (
                  selectedDay.entries.map((entry) => (
                    <CalendarEntryItem key={entry.id} entry={entry} labelsByType={fieldLabels} />
                  ))
                ) : (
                  <div className="py-8 text-center text-sm text-muted-foreground">No entries</div>
                )}
              </div>
            </aside>
          </div>
        ) : (
          <CalendarPlanningViews
            mode={mode}
            data={planning.data}
            loading={planning.isPending}
            onOpenMonth={openMonth}
          />
        )}
      </div>
    </AppFrame>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: string;
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

function readYear(value: string | null, fallback: number) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 1970 && parsed <= 2100 ? parsed : fallback;
}

function readMonth(value: string | null, fallback: number) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 1 && parsed <= 12 ? parsed : fallback;
}

function readSource(value: string | null): "all" | "taxonomy" | "daily-note" {
  return value === "taxonomy" || value === "daily-note" ? value : "all";
}

function readMode(value: string | null): CalendarMode {
  if (value === "year" || value === "seasons" || value === "planning") return value;
  return "month";
}

function todayInMonth(year: number, month: number, date: Date) {
  if (date.getFullYear() === year && date.getMonth() + 1 === month) {
    return `${year}-${String(month).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  }
  return `${year}-${String(month).padStart(2, "0")}-01`;
}

function monthGridDays(days: CalendarDay[], year: number, month: number) {
  const offset = (new Date(Date.UTC(year, month - 1, 1)).getUTCDay() + 6) % 7;
  return [...Array.from<undefined>({ length: offset }), ...days];
}

function monthTitle(year: number, month: number) {
  return `${year}-${String(month).padStart(2, "0")}`;
}
