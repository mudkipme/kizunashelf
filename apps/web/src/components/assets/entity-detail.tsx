import {
  BookOpenIcon,
  CalendarDaysIcon,
  CircleDotIcon,
  FileTextIcon,
  LanguagesIcon,
  LinkIcon,
  ListChecksIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
import { DetailSection, EmptyLine } from "@/components/assets/detail-section";
import { EntityDates } from "@/components/assets/entity-dates";
import { EntityCover } from "@/components/assets/entity-cover";
import { EntityEpisodesPanel, EpisodeSyncButton } from "@/components/assets/entity-episodes";
import { FrontmatterPanel } from "@/components/assets/frontmatter-panel";
import { LightboxProvider } from "@/components/assets/image-lightbox";
import { MarkdownView } from "@/components/assets/markdown-view";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useTitleLanguage } from "@/lib/language";
import { relationKey } from "@/lib/relations";
import {
  entityFieldLabel,
  fieldLabelForKey,
  titleLabelForKey,
  typeHasCoverField,
} from "@/lib/type-config";
import { entityTitle, titleLanguageLabel } from "@/lib/title-language";
import type {
  Entity,
  EntityDatesResponse,
  EntityEpisodes,
  EntitySummary,
  Relation,
  TypeConfig,
} from "@/types/api";

export function EntityDetail({
  entity,
  relations,
  relatedEntities,
  relationGroups,
  dates,
  typeConfig,
  episodes,
  episodesSaving = false,
  notesBody,
  contentWritable = true,
  labelsByType,
  typeLabels,
  coverTypes,
  onToggleEpisode,
  actions,
}: {
  entity: Entity;
  relations: Relation[];
  relatedEntities: EntitySummary[];
  relationGroups: Array<{ field: string; items: Relation[] }>;
  dates?: EntityDatesResponse;
  typeConfig?: TypeConfig;
  episodes?: EntityEpisodes;
  episodesSaving?: boolean;
  /// The body to render in "Notes" — `entity.body` with the episodes section
  /// stripped (it has its own panel). Falls back to `entity.body` while loading.
  notesBody?: string;
  contentWritable?: boolean;
  /// Field labels by type id, used to resolve "Linked from" field names against
  /// the *source* entity's type (the field lives on the linking type, not this one).
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  /// Type id → configured type label, so connection headers show the schema label
  /// ("Games") rather than the raw type id ("games") when the related entity that
  /// would otherwise supply the label isn't resolved.
  typeLabels?: ReadonlyMap<string, string>;
  /// Type ids that declare an image/imageList field. Connections to these types
  /// render as a cover grid; others fall back to text chips (no cover to show).
  coverTypes?: ReadonlySet<string>;
  onToggleEpisode?: (group: string, key: string, index: number, watched: boolean) => void;
  actions?: ReactNode;
}) {
  const language = useTitleLanguage();
  const displayTitle = entityTitle(entity, language);
  const relatedById = new Map(relatedEntities.map((item) => [item.id, item]));
  const subtitleTitles = entitySubtitleTitles(entity, displayTitle, typeConfig);
  // "Links to" groups by field; the untyped `body` field is further split by
  // target type ("body · Music"). "Linked from" groups by (source type, field)
  // so e.g. anime-via-franchise and games-via-franchise stay separate.
  const outgoingGroups = buildOutgoingGroups(relationGroups, relatedById, typeConfig, typeLabels);
  const incomingGroups = buildIncomingGroups(relations, relatedById, labelsByType, typeLabels);
  const hasConnections = outgoingGroups.length > 0 || incomingGroups.length > 0;
  // The cover slot shows only for types that declare an image/imageList field
  // (with a placeholder when this entity has no value); other types show none.
  const showCover = typeHasCoverField(typeConfig);

  return (
    <LightboxProvider>
      <div className="flex flex-col gap-4">
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
          <section className="min-w-0 rounded-md border">
            <div className="border-b p-4">
              <div className="flex gap-3">
                {showCover ? <EntityCover entity={entity} size="lg" lightbox /> : null}
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="secondary">{entity.typeLabel}</Badge>
                    {(entity.tags ?? []).map((tag) => (
                      <span
                        key={tag}
                        className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                      >
                        #{tag}
                      </span>
                    ))}
                  </div>
                  <h2 className="mt-2 text-xl font-semibold leading-snug">{displayTitle}</h2>
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
                {actions ? <div className="shrink-0">{actions}</div> : null}
              </div>
            </div>
            <div className="p-4">
              <DetailSection title="Details" icon={<BookOpenIcon />}>
                <FrontmatterPanel entity={entity} relationGroups={relationGroups} typeConfig={typeConfig} />
              </DetailSection>

              {episodes ? (
                <DetailSection
                  title={episodes.heading}
                  icon={<ListChecksIcon />}
                  action={
                    <EpisodeSyncButton
                      episodes={episodes}
                      disabled={!contentWritable || !onToggleEpisode}
                      entityId={entity.id}
                      revision={entity.revision}
                    />
                  }
                >
                  {/* Prose the user wrote around the list in the Markdown source —
                      rendered read-only here since the body view drops the section. */}
                  {episodes.description.trim() ? (
                    <MarkdownView markdown={episodes.description} relations={relations} />
                  ) : null}
                  <EntityEpisodesPanel
                    episodes={episodes}
                    disabled={!contentWritable || !onToggleEpisode}
                    saving={episodesSaving}
                    onToggle={(group, key, index, watched) => onToggleEpisode?.(group, key, index, watched)}
                    relations={relations}
                  />
                  {episodes.trailing.trim() ? (
                    <MarkdownView markdown={episodes.trailing} relations={relations} />
                  ) : null}
                </DetailSection>
              ) : null}

              {hasConnections ? (
                <DetailSection title="Connections" icon={<CircleDotIcon />}>
                  <div className="flex flex-col gap-4">
                    <RelationDirectionSection
                      title="Links to"
                      groups={outgoingGroups}
                      entityId={entity.id}
                      relatedById={relatedById}
                      coverTypes={coverTypes}
                    />
                    <RelationDirectionSection
                      title="Linked from"
                      groups={incomingGroups}
                      entityId={entity.id}
                      relatedById={relatedById}
                      coverTypes={coverTypes}
                    />
                  </div>
                </DetailSection>
              ) : null}

              {(notesBody ?? entity.body).trim() ? (
                <DetailSection title="Notes" icon={<FileTextIcon />}>
                  <MarkdownView markdown={notesBody ?? entity.body} relations={relations} />
                </DetailSection>
              ) : null}
            </div>
          </section>

          <aside className="min-w-0 rounded-md border p-4">
            <DetailSection title="Links" icon={<LinkIcon />}>
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
                <EmptyLine>No links yet</EmptyLine>
              )}
            </DetailSection>

            <DetailSection title="Dates" icon={<CalendarDaysIcon />}>
              <EntityDates dates={dates} typeConfig={typeConfig} />
            </DetailSection>
          </aside>
        </div>
      </div>
    </LightboxProvider>
  );
}

// Alternate titles to show under the heading: every other title, deduped by
// value and excluding whatever is currently displayed (the viewer-language title
// or its fallback).
function entitySubtitleTitles(
  entity: Entity,
  displayTitle: string,
  typeConfig: TypeConfig | undefined,
) {
  const normalize = (value: string) => value.trim().toLowerCase();
  const seen = new Set([normalize(displayTitle)]);
  const subtitles: Array<{ key: string; label: string; title: string }> = [];

  for (const [key, title] of Object.entries(entity.titles)) {
    const normalized = normalize(title);
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    subtitles.push({
      key,
      label: titleLabelForKey(typeConfig, key) || titleLanguageLabel(key),
      title,
    });
  }

  return subtitles;
}

type RelationGroup = { key: string; header: string; type: string; items: Relation[] };

// Groups outgoing ("Links to") relations by field. The untyped `body` field
// (body wikilinks can target any type) is split further by target type so it
// reads e.g. "body · Music"; typed relation fields stay a single group.
function buildOutgoingGroups(
  relationGroups: Array<{ field: string; items: Relation[] }>,
  relatedById: Map<string, EntitySummary>,
  typeConfig: TypeConfig | undefined,
  typeLabels: ReadonlyMap<string, string> | undefined,
): RelationGroup[] {
  const result: RelationGroup[] = [];
  for (const group of relationGroups) {
    const items = group.items.filter((relation) => relation.direction === "out");
    if (items.length === 0) continue;
    const fieldLabel = fieldLabelForKey(typeConfig, group.field);
    if (group.field !== "body") {
      // Typed relation field — all targets share one type.
      const type = items.find((relation) => relation.targetType)?.targetType ?? "";
      result.push({ key: group.field, header: fieldLabel, type, items });
      continue;
    }
    const byType = new Map<string, Relation[]>();
    for (const relation of items) {
      const type = relation.targetType ?? "";
      const existing = byType.get(type);
      if (existing) existing.push(relation);
      else byType.set(type, [relation]);
    }
    for (const [type, typeItems] of byType) {
      const typeLabel = typeLabels?.get(type) ?? relationTypeLabel(typeItems, relatedById) ?? type;
      result.push({
        key: `body::${type}`,
        header: typeLabel ? `${fieldLabel} · ${typeLabel}` : fieldLabel,
        type,
        items: typeItems,
      });
    }
  }
  return result;
}

// Groups incoming ("Linked from") relations by the SOURCE entity's (type, field).
// The detail entity is the `source_id` of its own relation rows, so the linking
// entity is `target_id` and its type/field are carried on `targetType`/`field`.
function buildIncomingGroups(
  relations: Relation[],
  relatedById: Map<string, EntitySummary>,
  labelsByType: ReadonlyMap<string, ReadonlyMap<string, string>> | undefined,
  typeLabels: ReadonlyMap<string, string> | undefined,
): RelationGroup[] {
  const groups = new Map<string, RelationGroup>();
  for (const relation of relations) {
    if (relation.direction !== "in") continue;
    const type = relation.targetType ?? "";
    const key = `${type}::${relation.field}`;
    const existing = groups.get(key);
    if (existing) {
      existing.items.push(relation);
      continue;
    }
    const typeLabel =
      typeLabels?.get(type) ??
      (relation.targetId ? relatedById.get(relation.targetId)?.typeLabel : undefined) ??
      type;
    const fieldLabel = entityFieldLabel(labelsByType, type, relation.field);
    groups.set(key, {
      key,
      header: typeLabel ? `${typeLabel} · ${fieldLabel}` : fieldLabel,
      type,
      items: [relation],
    });
  }
  return [...groups.values()];
}

// The target type label for a group of resolved relations (the first resolved
// item's related entity), used to label a field that spans types (body links).
function relationTypeLabel(
  items: Relation[],
  relatedById: Map<string, EntitySummary>,
): string | undefined {
  for (const relation of items) {
    const summary = relation.targetId ? relatedById.get(relation.targetId) : undefined;
    if (summary) return summary.typeLabel;
  }
  return undefined;
}

function RelationDirectionSection({
  title,
  groups,
  entityId,
  relatedById,
  coverTypes,
}: {
  title: string;
  groups: RelationGroup[];
  entityId: string;
  relatedById: Map<string, EntitySummary>;
  coverTypes?: ReadonlySet<string>;
}) {
  if (groups.length === 0) return null;
  const total = groups.reduce((count, group) => count + group.items.length, 0);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <span>{title}</span>
        <Badge variant="outline">{total}</Badge>
      </div>
      <div className="flex flex-col gap-3">
        {groups.map((group) => (
          <div key={group.key} className="flex flex-col gap-1.5">
            <div className="text-xs font-medium text-muted-foreground">{group.header}</div>
            {coverTypes?.has(group.type) ? (
              // Types with a cover field render as a poster grid.
              <div className="grid grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-3">
                {group.items.map((relation) => (
                  <RelationGridItem
                    key={relationKey(relation)}
                    relation={relation}
                    entityId={entityId}
                    relatedById={relatedById}
                  />
                ))}
              </div>
            ) : (
              // Cover-less types fall back to text chips.
              <div className="flex min-w-0 flex-wrap gap-1">
                {group.items.map((relation) => (
                  <RelationChip
                    key={relationKey(relation)}
                    relation={relation}
                    entityId={entityId}
                    relatedById={relatedById}
                  />
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

// A single connection rendered as its cover (with a type-initial placeholder when
// there's no image), linking to that entity. The other end is always `target_id`
// for this entity's rows; an unresolved link renders a non-clickable placeholder.
function RelationGridItem({
  relation,
  entityId,
  relatedById,
}: {
  relation: Relation;
  entityId: string;
  relatedById: Map<string, EntitySummary>;
}) {
  const language = useTitleLanguage();
  const otherId = relation.sourceId === entityId ? relation.targetId : relation.sourceId;
  const summary = otherId ? relatedById.get(otherId) : undefined;
  const label = summary ? entityTitle(summary, language) : relation.targetTitle;

  const card = (
    <>
      <AssetImage
        src={summary?.image}
        alt={label}
        className="aspect-square w-full rounded-md border object-cover"
        fallback={
          <span className="flex aspect-square w-full items-center justify-center rounded-md border bg-muted text-sm font-medium text-muted-foreground">
            {(summary?.typeLabel ?? label).slice(0, 2)}
          </span>
        }
      />
      <span className="line-clamp-2 text-xs leading-4">{label}</span>
    </>
  );

  if (summary && otherId) {
    return (
      <Link
        to={`/entities/${encodeURIComponent(otherId)}`}
        title={label}
        className="flex flex-col gap-1 rounded-md text-left transition-opacity hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {card}
      </Link>
    );
  }
  return (
    <span title={label} className="flex flex-col gap-1 text-left text-muted-foreground">
      {card}
    </span>
  );
}

// A connection to a cover-less type, rendered as a text chip (the title), linking
// to that entity. An unresolved link renders as a disabled chip.
function RelationChip({
  relation,
  entityId,
  relatedById,
}: {
  relation: Relation;
  entityId: string;
  relatedById: Map<string, EntitySummary>;
}) {
  const language = useTitleLanguage();
  const otherId = relation.sourceId === entityId ? relation.targetId : relation.sourceId;
  const summary = otherId ? relatedById.get(otherId) : undefined;
  const label = summary ? entityTitle(summary, language) : relation.targetTitle;
  const className =
    "h-auto min-h-8 max-w-full justify-start whitespace-normal break-all text-left leading-5";

  if (summary && otherId) {
    return (
      <Button variant="outline" size="sm" className={className} asChild>
        <Link to={`/entities/${encodeURIComponent(otherId)}`}>{label}</Link>
      </Button>
    );
  }
  return (
    <Button variant="outline" size="sm" className={className} disabled>
      {label}
    </Button>
  );
}
