import { Link } from "react-router-dom";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { Badge } from "@/components/ui/badge";
import { defaultTitleLanguage } from "@/lib/constants";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

export function RelationSourceRow({
  entity,
  titleLanguage = defaultTitleLanguage,
}: {
  entity: EntitySummary;
  titleLanguage?: string;
}) {
  const title = entityTitle(entity, titleLanguage);

  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none last:border-b-0"
    >
      <span className="min-w-0">
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="min-w-0 truncate text-sm font-medium">{title}</span>
          <Badge variant="outline">{entity.typeLabel}</Badge>
          {entity.status ? <Badge variant="secondary">{entity.status}</Badge> : null}
        </span>
        <span className="mt-1 flex min-w-0 flex-wrap items-center gap-1 text-xs text-muted-foreground">
          <EntityDateList entity={entity} compact />
        </span>
        {entity.summary ? (
          <span className="mt-1 block line-clamp-1 text-xs text-muted-foreground">{entity.summary}</span>
        ) : null}
      </span>
      <span className="self-start text-xs tabular-nums text-muted-foreground">{entity.relationCount} links</span>
    </Link>
  );
}
