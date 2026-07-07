import { Plural } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
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
          fallback={
            <span className="px-3 text-center text-sm font-medium text-muted-foreground">
              {entity.typeLabel}
            </span>
          }
        />
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-2 p-2.5">
        <EntityTitle
          as="div"
          entity={entity}
          language={language}
          className="line-clamp-2 text-sm font-medium leading-5"
        />
        <div className="mt-auto flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <span className="min-w-0 truncate">
            {entity.dates.length > 0 ? (
              <EntityDateList entity={entity} compact labelsByType={labelsByType} />
            ) : (
              entity.basename
            )}
          </span>
          <span className="ml-auto shrink-0 tabular-nums">
            <Plural value={entity.relationCount} one="# link" other="# links" />
          </span>
        </div>
      </div>
    </Link>
  );
}
