import { Badge } from "@/components/ui/badge";
import { entityFieldLabel } from "@/lib/type-config";
import type { EntitySummary } from "@/types/api";

export function EntityDateList({
  entity,
  compact = false,
  labelsByType,
}: {
  entity: EntitySummary;
  compact?: boolean;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  if (entity.dates.length === 0) return null;

  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1">
      {entity.dates.map((date) => (
        <Badge
          key={`${date.field}-${date.value}`}
          variant="outline"
          className={compact ? "max-w-full truncate px-1.5 py-0 text-[11px] font-normal" : "max-w-full truncate font-normal"}
        >
          {entityFieldLabel(labelsByType, entity.type, date.field)}: {date.value}
        </Badge>
      ))}
    </span>
  );
}
