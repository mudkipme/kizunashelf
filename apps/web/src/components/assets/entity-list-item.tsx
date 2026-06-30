import { Link } from "react-router-dom";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityCover } from "@/components/assets/entity-cover";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

export function EntityListItem({
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
      className={`grid w-full gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none ${
        showCover ? "grid-cols-[44px_1fr]" : "grid-cols-1"
      }`}
    >
      {showCover ? <EntityCover entity={entity} /> : null}
      <span className="min-w-0">
        <span className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{title}</span>
          <Badge variant="outline">{entity.typeLabel}</Badge>
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <EntityDateList entity={entity} compact labelsByType={labelsByType} />
          {entity.episodeProgress ? (
            <span className="ml-auto shrink-0 tabular-nums">
              {entity.episodeProgress.watched}/{entity.episodeProgress.total}
            </span>
          ) : null}
          <span className={entity.episodeProgress ? "shrink-0" : "ml-auto shrink-0"}>
            {entity.relationCount} links
          </span>
        </span>
        {entity.summary ? (
          <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">{entity.summary}</span>
        ) : null}
      </span>
    </Link>
  );
}
