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
        {day.counts.total > 0 ? <Badge variant="secondary">{day.counts.total}</Badge> : null}
      </div>

      {day.counts.total > 0 ? (
        <div className="flex flex-wrap gap-1">
          {day.counts.taxonomy > 0 ? (
            <Badge variant="outline">T {day.counts.taxonomy}</Badge>
          ) : null}
          {day.counts.dailyNotes > 0 ? (
            <Badge variant="outline">D {day.counts.dailyNotes}</Badge>
          ) : null}
        </div>
      ) : null}

      <div className="flex min-w-0 flex-col gap-1">
        {day.entries.slice(0, 3).map((entry) => (
          <span key={entry.id} className="truncate text-xs text-muted-foreground">
            {entry.entity.title}
          </span>
        ))}
        {day.entries.length > 3 ? (
          <span className="text-xs text-muted-foreground">+{day.entries.length - 3}</span>
        ) : null}
      </div>
    </button>
  );
}
