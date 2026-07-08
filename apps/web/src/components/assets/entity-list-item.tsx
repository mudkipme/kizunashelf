import { Plural } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityCover } from "@/components/assets/entity-cover";
import { EntityTitle } from "@/components/entities/entity-title";
import { StatusBadge } from "@/components/entities/status-badge";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import type { EntitySummary } from "@/types/api";

export function EntityListItem({
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
  /// single type, where labeling every row with it is redundant.
  showType?: boolean;
}) {
  const language = useTitleLanguage();

  // Drop the `0/0` progress chip (no total known) and the `0 links` chip — both
  // are noise. Whichever chip renders first gets pushed to the right edge.
  const progressLabel =
    entity.episodeProgress && entity.episodeProgress.total > 0
      ? `${entity.episodeProgress.watched}/${entity.episodeProgress.total}`
      : null;
  const showLinks = entity.relationCount > 0;

  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className={`grid w-full gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none ${
        showCover ? "grid-cols-[44px_1fr]" : "grid-cols-1"
      }`}
    >
      {showCover ? <EntityCover entity={entity} /> : null}
      <span className="min-w-0">
        <span className="flex flex-wrap items-center gap-2">
          <EntityTitle entity={entity} language={language} className="truncate text-sm font-medium" />
          {showType ? <Badge variant="outline">{entity.typeLabel}</Badge> : null}
          <StatusBadge status={entity.status} />
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <EntityDateList entity={entity} compact labelsByType={labelsByType} />
          {progressLabel ? (
            <span className="ml-auto shrink-0 tabular-nums">{progressLabel}</span>
          ) : null}
          {showLinks ? (
            <span className={progressLabel ? "shrink-0" : "ml-auto shrink-0"}>
              <Plural value={entity.relationCount} one="# link" other="# links" />
            </span>
          ) : null}
        </span>
        {entity.summary ? (
          <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">{entity.summary}</span>
        ) : null}
      </span>
    </Link>
  );
}
