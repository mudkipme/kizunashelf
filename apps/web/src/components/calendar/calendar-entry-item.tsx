import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { entityFieldLabel } from "@/lib/type-config";
import type { CalendarEntry } from "@/types/api";

export function CalendarEntryItem({
  entry,
  labelsByType,
}: {
  entry: CalendarEntry;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  const language = useTitleLanguage();
  return (
    <article className="rounded-md border p-3">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <Badge variant={entry.source === "taxonomy" ? "secondary" : "outline"}>
          {entry.source === "taxonomy"
            ? "Taxonomy"
            : entry.source === "episode"
              ? entry.episode?.heading || "Item"
              : "Daily Note"}
        </Badge>
        <Badge variant="outline">{entry.entity.typeLabel}</Badge>
        <Link
          to={`/entities/${encodeURIComponent(entry.entity.id)}`}
          className="min-w-0 break-words text-sm font-medium hover:underline"
        >
          {entityTitle(entry.entity, language)}
        </Link>
      </div>

      {entry.episode ? (
        <div className="mt-2 break-words text-xs text-muted-foreground">
          {entry.episode.role === "completed" ? "✅ Completed" : "📅 Scheduled"}:{" "}
          {entry.episode.key ? `${entry.episode.key} · ` : ""}
          {entry.episode.title || "—"}
        </div>
      ) : null}

      {entry.rawDate ? (
        <div className="mt-2 text-xs text-muted-foreground">
          {entry.dateField ? entityFieldLabel(labelsByType, entry.entity.type, entry.dateField) : "date"}: {entry.rawDate}
        </div>
      ) : null}

      {entry.notePath ? (
        <div className="mt-2 text-xs text-muted-foreground">{entry.notePath}</div>
      ) : null}

      {entry.snippets && entry.snippets.length > 0 ? (
        <div className="mt-3 flex flex-col gap-2">
          {entry.snippets.map((snippet) => (
            <figure key={`${entry.id}-${snippet.line}-${snippet.text}`} className="rounded-md bg-muted p-2">
              {snippet.heading ? (
                <figcaption className="mb-1 text-xs font-medium text-muted-foreground">
                  {snippet.heading}
                </figcaption>
              ) : null}
              <blockquote className="break-words text-sm leading-6">{snippet.text}</blockquote>
              <div className="mt-1 text-xs text-muted-foreground">line {snippet.line}</div>
            </figure>
          ))}
        </div>
      ) : null}
    </article>
  );
}
