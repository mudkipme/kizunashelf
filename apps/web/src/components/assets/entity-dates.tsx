import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { fieldLabelForKey } from "@/lib/type-config";
import type { EntityDatesResponse, TypeConfig } from "@/types/api";

export function EntityDates({ dates, typeConfig }: { dates?: EntityDatesResponse; typeConfig?: TypeConfig }) {
  if (!dates || dates.totals.metadata + dates.totals.dailyNotes === 0) {
    return <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">No dates</div>;
  }

  return (
    <div className="flex flex-col gap-3">
      {dates.metadata.length > 0 ? (
        <div className="flex flex-col gap-1">
          <div className="text-xs font-medium text-muted-foreground">Metadata</div>
          {dates.metadata.map((item) => (
            <div key={item.id} className="rounded-md border px-2 py-1.5 text-xs">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Badge variant="outline">{fieldLabelForKey(typeConfig, item.field)}</Badge>
                {item.date ? (
                  <Button variant="ghost" size="sm" className="h-6 px-1.5" asChild>
                    <Link to={calendarDateHref(item.date, "taxonomy")}>{item.value}</Link>
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
            Daily Notes ({dates.totals.snippets} snippets)
          </div>
          {dates.dailyNotes.map((item) => (
            <article key={item.id} className="rounded-md border p-2">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Button variant="ghost" size="sm" className="h-6 px-1.5" asChild>
                  <Link to={calendarDateHref(item.date, "daily-note")}>{item.date}</Link>
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
                    <div className="mt-1 text-xs text-muted-foreground">line {snippet.line}</div>
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

function calendarDateHref(date: string, source: "taxonomy" | "daily-note") {
  const [year, month] = date.split("-");
  return `/calendar?year=${year}&month=${Number(month)}&date=${date}&source=${source}`;
}
