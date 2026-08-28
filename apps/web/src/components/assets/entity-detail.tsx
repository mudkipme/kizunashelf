import { useLingui } from "@lingui/react/macro";
import { ExternalLinkIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { AssetImage } from "@/components/assets/asset-image";
import { CoverFallback } from "@/components/assets/cover-fallback";
import { DetailSection } from "@/components/assets/detail-section";
import { EntityDates } from "@/components/assets/entity-dates";
import { EntityEpisodesPanel, EpisodeSyncButton } from "@/components/assets/entity-episodes";
import { FrontmatterPanel, useVisibleFrontmatterEntries } from "@/components/assets/frontmatter-panel";
import { LightboxProvider } from "@/components/assets/image-lightbox";
import { MarkdownView } from "@/components/assets/markdown-view";
import { StatusBadge } from "@/components/entities/status-badge";
import { CONTENT_MEASURE } from "@/components/layout/page-container";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { relationKey } from "@/lib/relations";
import {
  entityFieldLabel,
  fieldLabelForKey,
  titleLabelForKey,
  typeHasCoverField,
} from "@/lib/type-config";
import { entityTitle, entityTitleParts, titleLanguageLabel } from "@/lib/title-language";
import { cn } from "@/lib/utils";
import type {
  Entity,
  EntityDatesResponse,
  EntityEpisodes,
  EntitySummary,
  Relation,
  TypeConfig,
} from "@/types/api";

/// The edge every cover carries instead of a border: a hairline drawn *inside*
/// the image reads as the edge of a photograph (and keeps pale artwork from
/// bleeding into the page), where a border reads as one more UI box.
const COVER_EDGE = "ring-1 ring-black/5 dark:ring-white/10";

/**
 * The entity detail page's body: a toolbar, a hero, and two panes.
 *
 * The page is drawn the way the app's own chrome is — with tone, alignment and
 * space rather than outlines. It spends exactly two hairlines: under the
 * toolbar, and between the content and the inspector. Nothing here is a card;
 * the window is already the frame.
 *
 * Above `lg` the two panes scroll independently, so the inspector (links,
 * dates) stays put while a long body scrolls past it — a native two-pane
 * arrangement. Below `lg` the inspector stacks under the content and the whole
 * thing is one scroll.
 */
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
  onSetEpisodeDate,
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
  /// Sets a checked episode's `✅` completion date (clicking the date on its row).
  onSetEpisodeDate?: (group: string, key: string, index: number, date: string) => void;
  /// The page's verbs, rendered at the right of the toolbar.
  actions?: ReactNode;
}) {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const { text: displayTitle, lang: displayTitleLang } = entityTitleParts(entity, language);
  // Stamp `lang` on the title only when it differs from the viewer's language,
  // so a foreign-language title (e.g. a Japanese original in a Chinese UI) gets
  // the right Han glyphs; a same-language title inherits the document `lang`.
  const titleLang = displayTitleLang && displayTitleLang !== language ? displayTitleLang : undefined;
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
  // Inspector sections hide entirely (title included) when empty; when both go
  // the whole pane collapses so the content column reclaims its width.
  const detailEntries = useVisibleFrontmatterEntries(entity, relationGroups, typeConfig);
  const hasDetails = detailEntries.length > 0;
  const hasLinks = Object.keys(entity.externalRefs).length > 0;
  const hasDates = Boolean(dates && dates.totals.metadata + dates.totals.dailyNotes > 0);
  const showAside = hasLinks || hasDates;
  const tags = entity.tags ?? [];

  return (
    <LightboxProvider>
      <div className="flex h-full min-h-0 flex-col overflow-hidden">
        {/* The type and status ride in the toolbar rather than the hero: the
            toolbar sits outside the scroll, so the two facts that say *what
            this is* stay on screen while a long entry scrolls past. */}
        <div className="flex min-h-(--toolbar-height) shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5">
          <Badge variant="secondary">{entity.typeLabel}</Badge>
          <StatusBadge status={entity.status} />
          {actions ? <div className="ml-auto flex items-center gap-2">{actions}</div> : null}
        </div>

        <div className="flex min-h-0 flex-1 flex-col overflow-auto overscroll-contain lg:flex-row lg:overflow-hidden">
          {/* The pane takes the whole width — it owns the scrollbar and the
              hairline beside it — and only the content inside is bounded. */}
          <div className="min-w-0 flex-1 px-4 py-4 lg:overflow-auto lg:overscroll-contain">
            <div className={CONTENT_MEASURE}>
              <header className="mb-8 flex min-w-0 items-start gap-4">
                {showCover ? <HeroCover entity={entity} /> : null}
                <div className="min-w-0 flex-1">
                  <h1 className="text-2xl font-semibold leading-tight tracking-tight" lang={titleLang}>
                    {displayTitle}
                  </h1>
                  {subtitleTitles.length > 0 ? (
                    <dl className="mt-2 flex flex-col gap-0.5 text-xs">
                      {subtitleTitles.map((item) => (
                        <div key={item.key} className="flex min-w-0 gap-2">
                          <dt className="shrink-0 text-muted-foreground">{item.label}</dt>
                          {/* Wrap the full alternate title on mobile; truncate to
                              one line only once there's room beside the cover. */}
                          <dd className="min-w-0 break-words sm:truncate">{item.title}</dd>
                        </div>
                      ))}
                    </dl>
                  ) : null}
                  {tags.length > 0 ? (
                    <div className="mt-3 flex flex-wrap gap-1">
                      {tags.map((tag) => (
                        <span
                          key={tag}
                          className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                        >
                          #{tag}
                        </span>
                      ))}
                    </div>
                  ) : null}
                </div>
              </header>

              {hasDetails ? (
                <DetailSection title={t`Details`}>
                  <FrontmatterPanel
                    entity={entity}
                    relationGroups={relationGroups}
                    typeConfig={typeConfig}
                  />
                </DetailSection>
              ) : null}

              {episodes ? (
                <DetailSection
                  title={episodes.heading}
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
                    onToggle={(group, key, index, watched) =>
                      onToggleEpisode?.(group, key, index, watched)
                    }
                    onSetDate={(group, key, index, date) =>
                      onSetEpisodeDate?.(group, key, index, date)
                    }
                    relations={relations}
                  />
                  {episodes.trailing.trim() ? (
                    <MarkdownView markdown={episodes.trailing} relations={relations} />
                  ) : null}
                </DetailSection>
              ) : null}

              {hasConnections ? (
                <DetailSection title={t`Connections`}>
                  <div className="flex flex-col gap-5">
                    <RelationDirectionSection
                      title={t`Links to`}
                      groups={outgoingGroups}
                      entityId={entity.id}
                      relatedById={relatedById}
                      coverTypes={coverTypes}
                    />
                    <RelationDirectionSection
                      title={t`Linked from`}
                      groups={incomingGroups}
                      entityId={entity.id}
                      relatedById={relatedById}
                      coverTypes={coverTypes}
                    />
                  </div>
                </DetailSection>
              ) : null}

              {(notesBody ?? entity.body).trim() ? (
                <DetailSection title={t`Notes`}>
                  <MarkdownView markdown={notesBody ?? entity.body} relations={relations} />
                </DetailSection>
              ) : null}
            </div>
          </div>

          {showAside ? (
            <aside className="shrink-0 border-t px-4 py-4 lg:w-80 lg:overflow-auto lg:overscroll-contain lg:border-l lg:border-t-0">
              {hasLinks ? (
                <DetailSection title={t`Links`}>
                  {/* Negative margin so the hover fill bleeds to the pane's
                      padding edge, the way a native source-list row does. */}
                  <div className="-mx-2 flex flex-col">
                    {Object.entries(entity.externalRefs).map(([key, value]) => (
                      <a
                        key={key}
                        href={value}
                        target="_blank"
                        rel="noreferrer"
                        title={value}
                        className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors hover:bg-accent"
                      >
                        <span className="min-w-0 shrink-0 truncate font-medium">
                          {fieldLabelForKey(typeConfig, key)}
                        </span>
                        {/* The host, not the whole URL: a truncated ref URL in a
                            320px pane is noise, and the host is what identifies it. */}
                        <span className="min-w-0 flex-1 truncate text-right text-muted-foreground">
                          {linkHost(value)}
                        </span>
                        <ExternalLinkIcon className="size-3 shrink-0 text-muted-foreground" />
                      </a>
                    ))}
                  </div>
                </DetailSection>
              ) : null}

              {hasDates ? (
                <DetailSection title={t`Dates`}>
                  <EntityDates dates={dates} typeConfig={typeConfig} />
                </DetailSection>
              ) : null}
            </aside>
          ) : null}
        </div>
      </div>
    </LightboxProvider>
  );
}

/**
 * The detail page's cover, at the image's own aspect ratio.
 *
 * Everywhere else a cover is a fixed square, because a list needs a predictable
 * row height. A detail page doesn't: it has one image and room to show it, so a
 * poster is a poster and a square is a square. Both axes are capped so a wide
 * still can't push the title off the line.
 */
function HeroCover({ entity }: { entity: Entity }) {
  return (
    <AssetImage
      src={entity.image}
      lightbox
      className={cn("max-h-44 w-auto max-w-32 shrink-0 rounded-md object-contain", COVER_EDGE)}
      fallback={<CoverFallback type={entity.type} className={cn("size-28 shrink-0 rounded-md", COVER_EDGE)} />}
    />
  );
}

/// The host of an external ref, as the compact stand-in for its URL. A value
/// that isn't a URL (refs are free-form) falls back to itself.
function linkHost(value: string) {
  try {
    return new URL(value).host.replace(/^www\./, "");
  } catch {
    return value;
  }
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
    <div className="flex flex-col gap-3">
      <div className="flex items-baseline gap-2 text-xs font-medium">
        <span>{title}</span>
        <span className="tabular-nums text-muted-foreground">{total}</span>
      </div>
      <div className="flex flex-col gap-4">
        {groups.map((group) => (
          <div key={group.key} className="flex flex-col gap-2">
            <div className="text-xs text-muted-foreground">{group.header}</div>
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
              <div className="flex min-w-0 flex-wrap gap-1.5">
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
        className={cn("aspect-square w-full rounded-md object-cover", COVER_EDGE)}
        fallback={
          <CoverFallback
            type={summary?.type}
            className={cn("aspect-square w-full rounded-md", COVER_EDGE)}
          />
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
// to that entity. An unresolved link renders as a dimmed, inert chip.
//
// A filled chip rather than an outlined button: a group can hold dozens of
// these, and at that count an outline per item is a wall of rectangles where a
// fill is just a tint.
const RELATION_CHIP =
  "max-w-full break-words rounded-md bg-muted px-2 py-1 text-xs leading-5 transition-colors";

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

  if (summary && otherId) {
    return (
      <Link
        to={`/entities/${encodeURIComponent(otherId)}`}
        className={cn(
          RELATION_CHIP,
          "hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        )}
      >
        {label}
      </Link>
    );
  }
  return <span className={cn(RELATION_CHIP, "text-muted-foreground")}>{label}</span>;
}
