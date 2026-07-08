import { Plural } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityTitle } from "@/components/entities/entity-title";
import { StatusBadge } from "@/components/entities/status-badge";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import type { EntitySummary } from "@/types/api";

export function EntityGridItem({
  entity,
  labelsByType,
  showCover = true,
  showType = true,
}: {
  entity: EntitySummary;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  /// Whether to render the cover slot. Off for types with no image/imageList
  /// field; a placeholder still shows when the type has a cover but this entity
  /// has no value.
  showCover?: boolean;
  /// Whether to render the type badge. Off when the list is already scoped to a
  /// single type, where labeling every card with it is redundant.
  showType?: boolean;
}) {
  const language = useTitleLanguage();

  // Episode progress is meaningful only when a total is known — a `0/0` chip is
  // noise. Same for `0 links`; drop both zero cases rather than show them.
  const progressLabel =
    entity.episodeProgress && entity.episodeProgress.total > 0
      ? `${entity.episodeProgress.watched}/${entity.episodeProgress.total}`
      : null;
  const showLinks = entity.relationCount > 0;

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
        {showType || entity.status ? (
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            {showType ? <Badge variant="outline">{entity.typeLabel}</Badge> : null}
            <StatusBadge status={entity.status} />
          </div>
        ) : null}
        <EntityTitle
          as="div"
          entity={entity}
          language={language}
          className="line-clamp-2 text-sm font-medium leading-5"
        />
        {entity.summary ? (
          <div className="line-clamp-3 text-xs leading-5 text-muted-foreground">{entity.summary}</div>
        ) : null}
        <div className="mt-auto flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span className="min-w-0">
            {entity.dates.length > 0 ? (
              <EntityDateList entity={entity} compact labelsByType={labelsByType} />
            ) : null}
          </span>
          <span className="shrink-0">
            {progressLabel ??
              (showLinks ? (
                <Plural value={entity.relationCount} one="# link" other="# links" />
              ) : null)}
          </span>
        </div>
      </div>
    </Link>
  );
}
