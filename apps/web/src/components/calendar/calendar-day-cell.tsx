import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { EntityTitle } from "@/components/entities/entity-title";
import { cn } from "@/lib/utils";
import type { CalendarDay } from "@/types/api";

export function CalendarDayCell({
  day,
  selected,
  onSelect,
}: {
  day: CalendarDay;
  selected: boolean;
  onSelect: (date: string) => void;
}) {
  const dayNumber = Number(day.date.slice(8, 10));
  const language = useTitleLanguage();

  const uniqueEntries = day.entries.filter(
    (entry, index, entries) =>
      entries.findIndex((other) => other.entity.id === entry.entity.id) === index,
  );

  return (
    <button
      type="button"
      onClick={() => onSelect(day.date)}
      className={cn(
        "flex aspect-square min-w-0 flex-col gap-2 border-b border-r p-2 text-left hover:bg-accent sm:aspect-auto sm:min-h-28",
        selected && "bg-accent",
      )}
    >
      {/* Top-align so the day number sits at the same height whether or not a
          (taller) count badge is present — `items-center` dropped badged numbers
          a couple px below their badge-less neighbors. */}
      <div className="flex items-start justify-between gap-2">
        <span className="text-xs font-medium leading-5">{dayNumber}</span>
        {uniqueEntries.length > 0 ? (
          <Badge variant="secondary">{uniqueEntries.length}</Badge>
        ) : null}
      </div>

      <div className="hidden min-w-0 flex-col gap-1 sm:flex">
        {uniqueEntries.slice(0, 3).map((entry) => (
          <EntityTitle
            key={entry.id}
            as="span"
            entity={entry.entity}
            language={language}
            className="truncate text-xs text-muted-foreground"
          />
        ))}
        {uniqueEntries.length > 3 ? (
          <span className="text-xs text-muted-foreground">+{uniqueEntries.length - 3}</span>
        ) : null}
      </div>
    </button>
  );
}
