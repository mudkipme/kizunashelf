import { Link } from "react-router-dom";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { EntityCover } from "@/components/assets/entity-cover";
import { Badge } from "@/components/ui/badge";
import { defaultTitleOptionId } from "@/lib/constants";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

export function EntityListItem({
  entity,
  titleLanguage = defaultTitleOptionId,
}: {
  entity: EntitySummary;
  titleLanguage?: string;
}) {
  const title = entityTitle(entity, titleLanguage);

  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="grid w-full grid-cols-[44px_1fr] gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <EntityCover entity={entity} />
      <span className="min-w-0">
        <span className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{title}</span>
          <Badge variant="outline">{entity.typeLabel}</Badge>
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <EntityDateList entity={entity} compact />
          <span className="ml-auto shrink-0">{entity.relationCount} links</span>
        </span>
        {entity.summary ? (
          <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">{entity.summary}</span>
        ) : null}
      </span>
    </Link>
  );
}
