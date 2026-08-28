import { Plural, Trans } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { useIsoDateFormat } from "@/lib/locale";
import { fieldLabelForKey } from "@/lib/type-config";
import type { EntityDatesResponse, TypeConfig } from "@/types/api";

/**
 * The inspector's dates: schema date fields, then the daily-note mentions.
 *
 * Rows, not boxes. Every item used to be its own bordered card inside a
 * bordered pane inside a bordered page; here the label/value pairing and the
 * gaps carry it, and the only fill left is the one on a quoted snippet — which
 * is quoted text, not a container.
 */
export function EntityDates({ dates, typeConfig }: { dates?: EntityDatesResponse; typeConfig?: TypeConfig }) {
  const formatDate = useIsoDateFormat();
  // The parent gates this section on the same emptiness check, so this is a
  // defensive guard rather than a visible empty state.
  if (!dates || dates.totals.metadata + dates.totals.dailyNotes === 0) return null;

  return (
    <div className="flex flex-col gap-5">
      {dates.metadata.length > 0 ? (
        <div className="flex flex-col gap-1.5">
          <div className="text-xs font-medium">
            <Trans>Metadata</Trans>
          </div>
          <dl className="flex flex-col gap-1">
            {dates.metadata.map((item) => (
              <div key={item.id} className="flex min-w-0 items-baseline justify-between gap-3 text-xs">
                <dt className="min-w-0 truncate text-muted-foreground">
                  {fieldLabelForKey(typeConfig, item.field)}
                </dt>
                <dd className="min-w-0 shrink-0">
                  {item.date ? (
                    <Link to={calendarDateHref(item.date)} className="tabular-nums hover:underline">
                      {formatDate(item.value)}
                    </Link>
                  ) : (
                    <span className="break-words text-muted-foreground">{item.value}</span>
                  )}
                </dd>
              </div>
            ))}
          </dl>
        </div>
      ) : null}

      {dates.dailyNotes.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="text-xs font-medium">
            <Plural
              value={dates.totals.snippets}
              one="Daily Notes (# snippet)"
              other="Daily Notes (# snippets)"
            />
          </div>
          <div className="flex flex-col gap-4">
            {dates.dailyNotes.map((item) => (
              <article key={item.id} className="flex flex-col gap-1.5">
                <div className="flex min-w-0 items-baseline gap-2 text-xs">
                  <Link
                    to={calendarDateHref(item.date)}
                    className="shrink-0 font-medium tabular-nums hover:underline"
                  >
                    {formatDate(item.date)}
                  </Link>
                  <span className="min-w-0 truncate text-muted-foreground" title={item.notePath}>
                    {item.notePath}
                  </span>
                </div>
                <div className="flex flex-col gap-1.5">
                  {item.snippets.map((snippet) => (
                    <figure
                      key={`${item.id}-${snippet.line}-${snippet.text}`}
                      className="rounded-md bg-muted/60 px-2.5 py-2"
                    >
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
