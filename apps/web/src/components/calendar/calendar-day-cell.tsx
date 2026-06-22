import { Badge } from "@/components/ui/badge";
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

  const uniqueEntries = day.entries.filter(
    (entry, index, entries) =>
      entries.findIndex((other) => other.entity.id === entry.entity.id) === index,
  );

  return (
    <button
      type="button"
      onClick={() => onSelect(day.date)}
      className={cn(
        "flex min-h-28 min-w-0 flex-col gap-2 border-b border-r p-2 text-left hover:bg-accent",
        selected && "bg-accent",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium">{dayNumber}</span>
        {uniqueEntries.length > 0 ? (
          <Badge variant="secondary">{uniqueEntries.length}</Badge>
        ) : null}
      </div>

      <div className="flex min-w-0 flex-col gap-1">
        {uniqueEntries.slice(0, 3).map((entry) => (
          <span key={entry.id} className="truncate text-xs text-muted-foreground">
            {entry.entity.title}
          </span>
        ))}
        {uniqueEntries.length > 3 ? (
          <span className="text-xs text-muted-foreground">+{uniqueEntries.length - 3}</span>
        ) : null}
      </div>
    </button>
  );
}
