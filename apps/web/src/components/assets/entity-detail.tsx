import { BookOpenIcon, CalendarDaysIcon, CircleDotIcon, FileTextIcon, LinkIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { DetailSection, EmptyLine } from "@/components/assets/detail-section";
import { EntityDates } from "@/components/assets/entity-dates";
import { EntityCover } from "@/components/assets/entity-cover";
import { MarkdownView } from "@/components/assets/markdown-view";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationKey, relationTargetHref } from "@/lib/relations";
import type { Entity, EntityDatesResponse, Relation } from "@/types/api";

export function EntityDetail({
  entity,
  relations,
  relationGroups,
  dates,
}: {
  entity: Entity;
  relations: Relation[];
  relationGroups: Array<{ field: string; items: Relation[] }>;
  dates?: EntityDatesResponse;
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
      <section className="min-w-0 rounded-md border">
        <div className="border-b p-4">
          <div className="flex gap-3">
            <EntityCover entity={entity} size="lg" />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">{entity.typeLabel}</Badge>
                {entity.status ? <Badge variant="outline">{entity.status}</Badge> : null}
              </div>
              <h2 className="mt-2 text-xl font-semibold leading-snug">{entity.title}</h2>
              <p className="mt-1 truncate text-xs text-muted-foreground">{entity.path}</p>
            </div>
          </div>
          {entity.summary ? (
            <p className="mt-3 text-sm leading-6 text-muted-foreground">{entity.summary}</p>
          ) : null}
        </div>
        <div className="p-4">
          {entity.body.trim() ? (
            <DetailSection title="Markdown" icon={<FileTextIcon />}>
              <MarkdownView markdown={entity.body} relations={relations} />
            </DetailSection>
          ) : null}

          <DetailSection title="Frontmatter" icon={<BookOpenIcon />}>
            <pre className="max-h-[520px] overflow-auto rounded-md bg-muted p-3 text-xs leading-5">
              {JSON.stringify(entity.frontmatter, null, 2)}
            </pre>
          </DetailSection>
        </div>
      </section>

      <aside className="min-w-0 rounded-md border p-4">
        <DetailSection title="External refs" icon={<LinkIcon />}>
          {Object.entries(entity.externalRefs).length > 0 ? (
            <div className="flex flex-col gap-2">
              {Object.entries(entity.externalRefs).map(([key, value]) => (
                <a
                  key={key}
                  href={value}
                  target="_blank"
                  rel="noreferrer"
                  className="truncate rounded-md border px-2 py-1 text-xs hover:bg-accent"
                >
                  {key}: {value}
                </a>
              ))}
            </div>
          ) : (
            <EmptyLine>No external refs</EmptyLine>
          )}
        </DetailSection>

        <DetailSection title="Dates" icon={<CalendarDaysIcon />}>
          <EntityDates dates={dates} />
        </DetailSection>

        <DetailSection title="Relations" icon={<CircleDotIcon />}>
          {relationGroups.length > 0 ? (
            <div className="flex flex-col gap-3">
              {relationGroups.map((group) => (
                <div key={group.field} className="flex flex-col gap-1">
                  <div className="text-xs font-medium text-muted-foreground">{group.field}</div>
                  <div className="flex min-w-0 flex-wrap gap-1">
                    {group.items.map((relation) =>
                      relation.direction === "out" ? (
                        <Button
                          key={relationKey(relation)}
                          variant="outline"
                          size="sm"
                          className="h-auto min-h-8 max-w-full justify-start whitespace-normal break-all text-left leading-5"
                          asChild
                        >
                          <Link to={relationTargetHref(relation.field, relation)}>
                            {relation.targetTitle}
                          </Link>
                        </Button>
                      ) : relation.targetId ? (
                        <Button
                          key={relationKey(relation)}
                          variant="outline"
                          size="sm"
                          className="h-auto min-h-8 max-w-full justify-start whitespace-normal break-all text-left leading-5"
                          asChild
                        >
                          <Link to={`/entities/${encodeURIComponent(relation.targetId)}`}>
                            {relation.targetTitle}
                          </Link>
                        </Button>
                      ) : (
                        <Button
                          key={relationKey(relation)}
                          variant="outline"
                          size="sm"
                          className="h-auto min-h-8 max-w-full justify-start whitespace-normal break-all text-left leading-5"
                          disabled
                        >
                          {relation.targetTitle}
                        </Button>
                      ),
                    )}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <EmptyLine>No relations</EmptyLine>
          )}
        </DetailSection>
      </aside>
    </div>
  );
}
