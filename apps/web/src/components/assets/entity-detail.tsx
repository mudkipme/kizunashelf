import {
  BookOpenIcon,
  CalendarDaysIcon,
  CircleDotIcon,
  FileTextIcon,
  LanguagesIcon,
  LinkIcon,
} from "lucide-react";
import { Link } from "react-router-dom";

import { DetailSection, EmptyLine } from "@/components/assets/detail-section";
import { EntityDates } from "@/components/assets/entity-dates";
import { EntityCover } from "@/components/assets/entity-cover";
import { FrontmatterPanel } from "@/components/assets/frontmatter-panel";
import { MarkdownView } from "@/components/assets/markdown-view";
import { RelationLocalGraph } from "@/components/relations/relation-local-graph";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationKey } from "@/lib/relations";
import {
  defaultTitleOption,
  fieldLabelForKey,
  titleLabelForKey,
} from "@/lib/type-config";
import { titleLanguageLabel } from "@/lib/title-language";
import type { Entity, EntityDatesResponse, EntitySummary, Relation, TypeConfig } from "@/types/api";

export function EntityDetail({
  entity,
  relations,
  relatedEntities,
  relationGroups,
  dates,
  typeConfig,
}: {
  entity: Entity;
  relations: Relation[];
  relatedEntities: EntitySummary[];
  relationGroups: Array<{ field: string; items: Relation[] }>;
  dates?: EntityDatesResponse;
  typeConfig?: TypeConfig;
}) {
  const relatedById = new Map(relatedEntities.map((item) => [item.id, item]));
  const subtitleTitles = entitySubtitleTitles(entity, typeConfig);
  const outgoingRelationGroups = relationGroupsForDirection(relationGroups, "out");
  const incomingRelationGroups = relationGroupsForDirection(relationGroups, "in");

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <section className="min-w-0 rounded-md border">
          <div className="border-b p-4">
            <div className="flex gap-3">
              <EntityCover entity={entity} size="lg" />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="secondary">{entity.typeLabel}</Badge>
                </div>
                <h2 className="mt-2 text-xl font-semibold leading-snug">{entity.title}</h2>
                {subtitleTitles.length > 0 ? (
                  <dl className="mt-3 grid gap-1 text-xs sm:grid-cols-[auto_minmax(0,1fr)]">
                    {subtitleTitles.map((item) => (
                      <div key={item.key} className="contents">
                        <dt className="flex items-center gap-1 text-muted-foreground">
                          <LanguagesIcon />
                          {item.label}
                        </dt>
                        <dd className="min-w-0 truncate font-medium">{item.title}</dd>
                      </div>
                    ))}
                  </dl>
                ) : null}
              </div>
            </div>
          </div>
          <div className="p-4">
            <DetailSection title="Frontmatter" icon={<BookOpenIcon />}>
              <FrontmatterPanel entity={entity} relationGroups={relationGroups} typeConfig={typeConfig} />
            </DetailSection>

            {entity.body.trim() ? (
              <DetailSection title="Markdown" icon={<FileTextIcon />}>
                <MarkdownView markdown={entity.body} relations={relations} />
              </DetailSection>
            ) : null}
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
                    {fieldLabelForKey(typeConfig, key)}: {value}
                  </a>
                ))}
              </div>
            ) : (
              <EmptyLine>No external refs</EmptyLine>
            )}
          </DetailSection>

          <DetailSection title="Dates" icon={<CalendarDaysIcon />}>
            <EntityDates dates={dates} typeConfig={typeConfig} />
          </DetailSection>

          <DetailSection title="Relations" icon={<CircleDotIcon />}>
            {relationGroups.length > 0 ? (
              <div className="flex flex-col gap-4">
                <RelationDirectionSection
                  title="Outgoing"
                  groups={outgoingRelationGroups}
                  entityId={entity.id}
                  relatedById={relatedById}
                  typeConfig={typeConfig}
                />
                <RelationDirectionSection
                  title="Incoming"
                  groups={incomingRelationGroups}
                  entityId={entity.id}
                  relatedById={relatedById}
                  typeConfig={typeConfig}
                />
              </div>
            ) : (
              <EmptyLine>No relations</EmptyLine>
            )}
          </DetailSection>
        </aside>
      </div>

      {relatedEntities.length > 0 ? (
        <RelationLocalGraph
          target={{
            targetTitle: entity.title,
            targetTypeLabel: entity.typeLabel,
            count: relations.length,
          }}
          sources={relatedEntities}
        />
      ) : null}
    </div>
  );
}

function entitySubtitleTitles(entity: Entity, typeConfig: TypeConfig | undefined) {
  const defaultTitleLanguage = defaultTitleOption(typeConfig);
  const normalize = (value: string) => value.trim().toLowerCase();
  const seen = new Set([normalize(entity.title)]);
  const subtitles: Array<{ key: string; label: string; title: string }> = [];

  for (const [key, title] of Object.entries(entity.titles)) {
    const normalized = normalize(title);
    if (!normalized || key === defaultTitleLanguage || seen.has(normalized)) continue;
    seen.add(normalized);
    subtitles.push({
      key,
      label: titleLabelForKey(typeConfig, key) || titleLanguageLabel(key),
      title,
    });
  }

  return subtitles;
}

function relationGroupsForDirection(
  relationGroups: Array<{ field: string; items: Relation[] }>,
  direction: Relation["direction"],
) {
  return relationGroups
    .map((group) => ({
      ...group,
      items: group.items.filter((relation) => relation.direction === direction),
    }))
    .filter((group) => group.items.length > 0);
}

function RelationDirectionSection({
  title,
  groups,
  entityId,
  relatedById,
  typeConfig,
}: {
  title: string;
  groups: Array<{ field: string; items: Relation[] }>;
  entityId: string;
  relatedById: Map<string, EntitySummary>;
  typeConfig?: TypeConfig;
}) {
  if (groups.length === 0) return null;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <span>{title}</span>
        <Badge variant="outline">{groups.reduce((count, group) => count + group.items.length, 0)}</Badge>
      </div>
      <div className="flex flex-col gap-3">
        {groups.map((group) => (
          <div key={group.field} className="flex flex-col gap-1">
            <div className="text-xs font-medium text-muted-foreground">
              {fieldLabelForKey(typeConfig, group.field)}
            </div>
            <div className="flex min-w-0 flex-wrap gap-1">
              {group.items.map((relation) => (
                <RelationButton
                  key={relationKey(relation)}
                  relation={relation}
                  entityId={entityId}
                  relatedById={relatedById}
                />
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function RelationButton({
  relation,
  entityId,
  relatedById,
}: {
  relation: Relation;
  entityId: string;
  relatedById: Map<string, EntitySummary>;
}) {
  const display = relationDisplay(relation, entityId, relatedById);
  const className =
    "h-auto min-h-8 max-w-full justify-start whitespace-normal break-all text-left leading-5";

  if (!display.href) {
    return (
      <Button variant="outline" size="sm" className={className} disabled>
        {display.label}
      </Button>
    );
  }

  return (
    <Button variant="outline" size="sm" className={className} asChild>
      <Link to={display.href}>{display.label}</Link>
    </Button>
  );
}

function relationDisplay(
  relation: Relation,
  entityId: string,
  relatedById: Map<string, EntitySummary>,
) {
  if (relation.sourceId === entityId) {
    return {
      label: relation.targetTitle,
      href: relation.targetId ? `/entities/${encodeURIComponent(relation.targetId)}` : undefined,
    };
  }

  if (relation.targetId === entityId) {
    const source = relatedById.get(relation.sourceId);
    return {
      label: source?.title ?? relation.sourceId,
      href: source ? `/entities/${encodeURIComponent(source.id)}` : undefined,
    };
  }

  return { label: relation.targetTitle };
}
