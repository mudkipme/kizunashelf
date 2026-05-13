import { DatabaseIcon, TablePropertiesIcon } from "lucide-react";

import { Metric } from "@/components/assets/metric";
import { TypeButton } from "@/components/assets/type-button";
import { Separator } from "@/components/ui/separator";
import type { StatsResponse } from "@/types/api";

export function LibrarySidebar({
  stats,
  selectedType,
  onSelectType,
}: {
  stats?: StatsResponse;
  selectedType: string;
  onSelectType: (type: string) => void;
}) {
  return (
    <aside className="border-b md:border-b-0 md:border-r">
      <div className="flex flex-col gap-4 p-3">
        <section className="flex flex-col gap-2">
          <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
            <DatabaseIcon />
            Library
          </div>
          <div className="grid grid-cols-2 gap-2 md:grid-cols-1">
            <Metric
              label="Entities"
              value={stats?.byType.reduce((sum, item) => sum + item.count, 0) ?? 0}
            />
            <Metric label="Relations" value={stats?.relations ?? 0} />
          </div>
        </section>

        <Separator />

        <section className="flex flex-col gap-1">
          <div className="flex items-center gap-2 px-1 text-xs font-medium uppercase text-muted-foreground">
            <TablePropertiesIcon />
            Types
          </div>
          {stats?.byType.map((type) => (
            <TypeButton
              key={type.id}
              active={selectedType === type.id}
              label={type.label}
              count={type.count}
              onClick={() => onSelectType(type.id)}
            />
          ))}
        </section>
      </div>
    </aside>
  );
}
