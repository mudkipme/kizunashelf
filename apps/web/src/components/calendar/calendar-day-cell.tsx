import { EntityTitle } from "@/components/entities/entity-title";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
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

  // `day.items` is already one entry per entity (the core merges same-entity,
  // same-day facets), so no client-side dedup is needed.
  const items = day.items;

  return (
    <button
      type="button"
      onClick={() => onSelect(day.date)}
      className={cn(
        "flex aspect-square min-w-0 flex-col gap-2 border-r border-b p-2 text-left hover:bg-accent sm:aspect-auto sm:min-h-28",
        selected && "bg-accent",
      )}
    >
      {/* Top-align so the day number sits at the same height whether or not a
          (taller) count badge is present — `items-center` dropped badged numbers
          a couple px below their badge-less neighbors. */}
      <div className="flex items-start justify-between gap-2">
        <span className="text-xs leading-5 font-medium">{dayNumber}</span>
        {items.length > 0 ? <Badge variant="secondary">{items.length}</Badge> : null}
      </div>

      <div className="hidden min-w-0 flex-col gap-1 sm:flex">
        {items.slice(0, 3).map((item) => (
          <EntityTitle
            key={`${item.date}-${item.entity.id}`}
            as="span"
            entity={item.entity}
            language={language}
            className="truncate text-xs text-muted-foreground"
          />
        ))}
        {items.length > 3 ? (
          <span className="text-xs text-muted-foreground">+{items.length - 3}</span>
        ) : null}
      </div>
    </button>
  );
}
