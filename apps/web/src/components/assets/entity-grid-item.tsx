import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

export function EntityGridItem({
  entity,
  labelsByType,
  showCover = true,
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  /// Whether to render the cover slot. Off for types with no image/imageList
  /// field; a placeholder still shows when the type has a cover but this entity
  /// has no value.
  showCover?: boolean;
}) {
  const title = entityTitle(entity, useTitleLanguage());

  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="flex min-h-64 flex-col overflow-hidden rounded-md border bg-background transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      {showCover ? (
        <div className="flex aspect-[4/3] items-center justify-center bg-muted">
          <AssetImage
            src={entity.image}
            className="size-full object-cover"
            fallback={
              <span className="text-sm font-medium text-muted-foreground">{entity.typeLabel}</span>
            }
          />
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
        <div className="flex min-w-0 items-center gap-2">
          <Badge variant="outline">{entity.typeLabel}</Badge>
        </div>
        <div className="line-clamp-2 text-sm font-medium leading-5">{title}</div>
        {entity.summary ? (
          <div className="line-clamp-3 text-xs leading-5 text-muted-foreground">{entity.summary}</div>
        ) : null}
        <div className="mt-auto flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span className="min-w-0">
            {entity.dates.length > 0 ? (
              <EntityDateList entity={entity} compact labelsByType={labelsByType} />
            ) : (
              entity.basename
            )}
          </span>
          <span className="shrink-0">
            {entity.episodeProgress
              ? `${entity.episodeProgress.watched}/${entity.episodeProgress.total}`
              : `${entity.relationCount} links`}
          </span>
        </div>
      </div>
    </Link>
  );
}
