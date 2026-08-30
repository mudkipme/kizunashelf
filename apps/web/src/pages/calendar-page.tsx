import { useMemo } from "react";
import { plural } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { calendarQuery, configQuery } from "@/api/queries";
import { CalendarDayCell } from "@/components/calendar/calendar-day-cell";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { ActivityCard } from "@/pages/activity-page";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { useDateFormat, useIsoDateFormat } from "@/lib/locale";
import { coverTypeIds, fieldLabelsByType } from "@/lib/type-config";
import type { CalendarDay } from "@/types/api";

export function CalendarPage() {
  const { t } = useLingui();
  const [searchParams, setSearchParams] = useSearchParams();
  const now = new Date();
  const year = readYear(searchParams.get("year"), now.getFullYear());
  const month = readMonth(searchParams.get("month"), now.getMonth() + 1);
  const selectedDate = searchParams.get("date") ?? todayInMonth(year, month, now);
  const type = searchParams.get("type") ?? "all";
  const calendarParams = {
    year,
    month,
    ...(type !== "all" ? { type } : {}),
  };
  const config = useQuery(configQuery());
  const calendar = useQuery(calendarQuery(calendarParams));

  // Locale-aware weekday header, Monday-first (2024-01-01 is a Monday).
  const formatWeekday = useDateFormat({ weekday: "short" });
  const weekdays = useMemo(
    () => Array.from({ length: 7 }, (_, index) => formatWeekday(new Date(2024, 0, 1 + index))),
    [formatWeekday],
  );
  // The visible month title and the selected-day heading, localized (storage
  // and URL params stay ISO `YYYY-MM` / `YYYY-MM-DD`).
  const formatMonthTitle = useDateFormat({ month: "long", year: "numeric" });
  const formatSelectedDate = useIsoDateFormat({ year: "numeric", month: "long", day: "numeric" });
  const gridDays = useMemo(
    () => monthGridDays(calendar.data?.days ?? [], year, month),
    [calendar.data, year, month],
  );
  const selectedDay =
    calendar.data?.days.find((day) => day.date === selectedDate) ?? calendar.data?.days[0];
  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const coverTypes = useMemo(() => coverTypeIds(config.data?.types), [config.data]);

  function setParam(key: string, value: string, defaultValue?: string, options?: { replace?: boolean }) {
    const next = new URLSearchParams(searchParams);
    if (defaultValue !== undefined && value === defaultValue) next.delete(key);
    else next.set(key, value);
    setSearchParams(next, { replace: options?.replace ?? false });
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
    setSearchParams(next, { replace: true });
  }

  return (
    <AppFrame error={calendar.error ? errorMessage(calendar.error) : undefined}>
      <PageContainer width="wide">
        {/* One row: which month you're looking at, how you move between months,
            and the only filter left. The month title leads it — it's the answer
            to "where am I", not a caption for a separate band below. */}
        <header className="flex flex-wrap items-center gap-2">
          <h1 className="text-sm font-medium">{formatMonthTitle(new Date(year, month - 1, 1))}</h1>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" onClick={() => moveMonth(-1)}>
              <ChevronLeftIcon data-icon="inline-start" />
              <Trans comment="Calendar navigation button for the previous month">Prev</Trans>
            </Button>
            <Button variant="outline" size="sm" onClick={goToday}>
              <Trans>Today</Trans>
            </Button>
            <Button variant="outline" size="sm" onClick={() => moveMonth(1)}>
              <Trans comment="Calendar navigation button for the next month">Next</Trans>
              <ChevronRightIcon data-icon="inline-end" />
            </Button>
          </div>
          <Select
            value={type}
            onChange={(event) => setParam("type", event.target.value, "all")}
            aria-label={t`Filter by type`}
            className="ml-auto max-w-56"
          >
            <option value="all">{t`All types`}</option>
            {(config.data?.types ?? []).map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </Select>
        </header>

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
                  // Match the day cell's responsive height, or the leading blanks
                  // (min-h-28) would inflate the first week's row on mobile, where
                  // day cells are only aspect-square tall.
                  <div
                    key={`blank-${index}`}
                    className="aspect-square border-b border-r bg-muted/30 sm:aspect-auto sm:min-h-28"
                  />
                ),
              )}
            </div>
          </section>

          <aside className="min-w-0 rounded-md border">
            <header className="border-b px-3 py-2">
              <h2 className="text-sm font-semibold">
                {selectedDay ? formatSelectedDate(selectedDay.date) : t`No date selected`}
              </h2>
              <p className="mt-1 text-xs text-muted-foreground">
                {selectedDay
                  ? plural(selectedDay.items.length, { one: "# entry", other: "# entries" })
                  : t`No entries`}
              </p>
            </header>
            <div className="flex max-h-[720px] flex-col gap-2 overflow-auto p-3">
              {selectedDay && selectedDay.items.length > 0 ? (
                selectedDay.items.map((item) => (
                  <ActivityCard
                    key={`${item.date}-${item.entity.id}`}
                    item={item}
                    labels={fieldLabels}
                    hasCover={coverTypes.has(item.entity.type)}
                  />
                ))
              ) : (
                <div className="py-8 text-center text-sm text-muted-foreground">
                  <Trans>No entries</Trans>
                </div>
              )}
            </div>
          </aside>
        </div>
      </PageContainer>
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
