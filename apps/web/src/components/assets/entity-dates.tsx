import { Plural, Trans } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useIsoDateFormat } from "@/lib/locale";
import { fieldLabelForKey } from "@/lib/type-config";
import type { EntityDatesResponse, TypeConfig } from "@/types/api";

export function EntityDates({ dates, typeConfig }: { dates?: EntityDatesResponse; typeConfig?: TypeConfig }) {
  const formatDate = useIsoDateFormat();
  // The parent gates this section on the same emptiness check, so this is a
  // defensive guard rather than a visible empty state.
  if (!dates || dates.totals.metadata + dates.totals.dailyNotes === 0) return null;

  return (
    <div className="flex flex-col gap-3">
      {dates.metadata.length > 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-xs font-medium text-muted-foreground">
            <Trans>Metadata</Trans>
          </div>
          {dates.metadata.map((item) => (
            <div key={item.id} className="rounded-md border px-2 py-1.5 text-xs">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Badge variant="outline">{fieldLabelForKey(typeConfig, item.field)}</Badge>
                {item.date ? (
                  <Button variant="ghost" size="sm" className="h-6 px-1.5" asChild>
                    <Link to={calendarDateHref(item.date)}>{formatDate(item.value)}</Link>
                  </Button>
                ) : (
                  <span className="break-words text-muted-foreground">{item.value}</span>
                )}
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {dates.dailyNotes.length > 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-xs font-medium text-muted-foreground">
            <Plural
              value={dates.totals.snippets}
              one="Daily Notes (# snippet)"
              other="Daily Notes (# snippets)"
            />
          </div>
          {dates.dailyNotes.map((item) => (
            <article key={item.id} className="rounded-md border p-2">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Button variant="ghost" size="sm" className="h-6 px-1.5" asChild>
                  <Link to={calendarDateHref(item.date)}>{formatDate(item.date)}</Link>
                </Button>
                <span className="min-w-0 truncate text-xs text-muted-foreground">{item.notePath}</span>
              </div>
              <div className="mt-2 flex flex-col gap-2">
                {item.snippets.map((snippet) => (
                  <figure key={`${item.id}-${snippet.line}-${snippet.text}`} className="rounded-md bg-muted p-2">
                    {snippet.heading ? (
                      <figcaption className="mb-1 text-xs font-medium text-muted-foreground">
                        {snippet.heading}
                      </figcaption>
                    ) : null}
                    <blockquote className="break-words text-xs leading-5">{snippet.text}</blockquote>
                    <div className="mt-1 text-xs text-muted-foreground">
                      <Trans>line {snippet.line}</Trans>
                    </div>
                  </figure>
                ))}
              </div>
            </article>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function calendarDateHref(date: string) {
  const [year, month] = date.split("-");
  // Deep-link to the day itself and leave the source filter at "all": pinning it
  // to the clicked date's source would hide that day's other entries (a taxonomy
  // date would drop its daily-note/episode entries, and vice versa).
  return `/calendar?year=${year}&month=${Number(month)}&date=${date}`;
}
