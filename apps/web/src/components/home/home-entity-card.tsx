import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
import { CoverFallback } from "@/components/assets/cover-fallback";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityTitle } from "@/components/entities/entity-title";
import { useTitleLanguage } from "@/lib/language";
import type { EntitySummary } from "@/types/api";

export function HomeEntityCard({
  entity,
  labelsByType,
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  const language = useTitleLanguage();

  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="group flex min-h-56 min-w-0 flex-col overflow-hidden rounded-md border bg-background transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <div className="flex aspect-[4/5] items-center justify-center bg-muted">
        <AssetImage
          src={entity.image}
          className="size-full object-cover transition-transform group-hover:scale-[1.02]"
          fallback={<CoverFallback type={entity.type} />}
        />
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-2 p-2.5">
        <EntityTitle
          as="div"
          entity={entity}
          language={language}
          className="line-clamp-2 text-sm leading-5 font-medium"
        />
        {/* Dates only; a link count here (usually "0 links") was noise, and the
            library's list and grid views already drop it. */}
        <div className="mt-auto flex min-w-0 text-xs text-muted-foreground">
          {entity.dates.length > 0 ? (
            <EntityDateList entity={entity} labelsByType={labelsByType} />
          ) : (
            <span className="min-w-0 truncate">{entity.basename}</span>
          )}
        </div>
      </div>
    </Link>
  );
}
