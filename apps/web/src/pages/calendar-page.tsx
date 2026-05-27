import { useEffect, useMemo, useState } from "react";
import { getCalendar, getConfig } from "@kizunashelf/api-contract";
import { CalendarDaysIcon, ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { CalendarDayCell } from "@/components/calendar/calendar-day-cell";
import { CalendarEntryItem } from "@/components/calendar/calendar-entry-item";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { CalendarDay, CalendarResponse, ConfigResponse } from "@/types/api";

type CalendarState = {
  data?: CalendarResponse;
  config?: ConfigResponse;
  loading: boolean;
  error?: string;
};

const weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function CalendarPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const now = new Date();
  const year = readYear(searchParams.get("year"), now.getFullYear());
  const month = readMonth(searchParams.get("month"), now.getMonth() + 1);
  const selectedDate = searchParams.get("date") ?? todayInMonth(year, month, now);
  const source = readSource(searchParams.get("source"));
  const type = searchParams.get("type") ?? "all";
  const [state, setState] = useState<CalendarState>({ loading: true });

  useEffect(() => {
    void loadConfig();
  }, []);

  useEffect(() => {
    void loadCalendar();
  }, [year, month, source, type]);

  async function loadConfig() {
    try {
      const config = await getConfig(undefined, apiFetch);
      setState((current) => ({ ...current, config }));
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    }
  }

  async function loadCalendar() {
    setState((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const data = await getCalendar(
        {
          year,
          month,
          source,
          ...(type !== "all" ? { type } : {}),
        },
        undefined,
        apiFetch,
      );
      setState((current) => ({ ...current, data, loading: false }));
    } catch (error) {
      setState((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  const gridDays = useMemo(() => monthGridDays(state.data?.days ?? [], year, month), [
    state.data,
    year,
    month,
  ]);
  const selectedDay =
    state.data?.days.find((day) => day.date === selectedDate) ?? state.data?.days[0];

  function setParam(key: string, value: string, defaultValue?: string) {
    const next = new URLSearchParams(searchParams);
    if (defaultValue !== undefined && value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next);
  }

  function moveMonth(delta: number) {
    const nextDate = new Date(Date.UTC(year, month - 1 + delta, 1));
    const next = new URLSearchParams(searchParams);
    next.set("year", String(nextDate.getUTCFullYear()));
    next.set("month", String(nextDate.getUTCMonth() + 1));
    next.delete("date");
    setSearchParams(next);
  }

  function goToday() {
    const today = new Date();
    const next = new URLSearchParams(searchParams);
    next.set("year", String(today.getFullYear()));
    next.set("month", String(today.getMonth() + 1));
    next.set("date", todayInMonth(today.getFullYear(), today.getMonth() + 1, today));
    setSearchParams(next);
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <h1 className="flex items-center gap-2 text-base font-semibold">
              <CalendarDaysIcon />
              Calendar
            </h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {state.loading
                ? "Loading"
                : state.data
                  ? `${state.data.totals.entries} entries across ${state.data.totals.daysWithEntries} days`
                  : "No calendar data"}
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" onClick={() => moveMonth(-1)}>
              <ChevronLeftIcon data-icon="inline-start" />
              Prev
            </Button>
            <Button variant="outline" size="sm" onClick={goToday}>
              Today
            </Button>
            <Button variant="outline" size="sm" onClick={() => moveMonth(1)}>
              Next
              <ChevronRightIcon data-icon="inline-end" />
            </Button>
          </div>
        </header>

        <section className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2">
          <div className="text-sm font-medium">{monthTitle(year, month)}</div>
          {state.data ? (
            <div className="flex flex-wrap gap-1">
              <Badge variant="outline">Taxonomy {state.data.totals.taxonomy}</Badge>
              <Badge variant="outline">Daily Notes {state.data.totals.dailyNotes}</Badge>
            </div>
          ) : null}
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <Select value={source} onChange={(event) => setParam("source", event.target.value, "all")}>
              <option value="all">All sources</option>
              <option value="taxonomy">Taxonomy dates</option>
              <option value="daily-note">Daily note mentions</option>
            </Select>
            <Select value={type} onChange={(event) => setParam("type", event.target.value, "all")}>
              <option value="all">All types</option>
              {state.config?.types.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </Select>
          </div>
        </section>

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
                    onSelect={(date) => setParam("date", date)}
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
                  <CalendarEntryItem key={entry.id} entry={entry} />
                ))
              ) : (
                <div className="py-8 text-center text-sm text-muted-foreground">No entries</div>
              )}
            </div>
          </aside>
        </div>
      </div>
    </AppFrame>
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
