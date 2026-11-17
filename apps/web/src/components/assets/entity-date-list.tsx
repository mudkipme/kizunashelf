import { Badge } from "@/components/ui/badge";
import { useIsoDateFormat } from "@/lib/locale";
import { entityFieldLabel } from "@/lib/type-config";
import type { EntitySummary } from "@/types/api";

/// An entity's dates as chips, each named by its schema field label ("Release
/// date", "Completed date", …) — a row of bare dates gives no hint which is a
/// release and which is when the user finished it. The label is the schema's
/// display name, so it reads however the user named the field. In a narrow card
/// the label truncates first and the date itself always stays whole.
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
      {entity.dates.map((date) => {
        const label = entityFieldLabel(labelsByType, entity.type, date.field);
        const value = formatDate(date.value);
        return (
          <Badge
            key={`${date.field}-${date.value}`}
            variant="outline"
            title={`${label}: ${value}`}
            className="max-w-full justify-start gap-1 px-1.5 py-0 text-xs font-normal"
          >
            <span className="min-w-0 truncate text-muted-foreground">{label}</span>
            <span className="shrink-0 tabular-nums">{value}</span>
          </Badge>
        );
      })}
    </span>
  );
}
