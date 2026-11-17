import { Badge } from "@/components/ui/badge";
import { useIsoDateFormat } from "@/lib/locale";
import { entityFieldLabel } from "@/lib/type-config";
import type { EntitySummary } from "@/types/api";

export function EntityDateList({
  entity,
  labelsByType,
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  const formatDate = useIsoDateFormat();
  if (entity.dates.length === 0) return null;

  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1">
      {entity.dates.map((date) => (
        <Badge
          key={`${date.field}-${date.value}`}
          variant="outline"
          title={`${entityFieldLabel(labelsByType, entity.type, date.field)}: ${formatDate(date.value)}`}
          className="max-w-full truncate px-1.5 py-0 text-xs font-normal"
        >
          {formatDate(date.value)}
        </Badge>
      ))}
    </span>
  );
}
