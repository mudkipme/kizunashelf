import { Trans, useLingui } from "@lingui/react/macro";
import { CheckIcon, RefreshCwIcon } from "lucide-react";
import { useState } from "react";

import { EpisodeSyncDialog } from "@/components/assets/episode-sync-dialog";
import { InlineMarkdown } from "@/components/assets/markdown-view";
import { Button } from "@/components/ui/button";
import { useIsoDateFormat } from "@/lib/locale";
import { cn } from "@/lib/utils";
import type { EntityEpisodes, Relation } from "@/types/api";

/// Displays an entity's episodes/tracks list (grouped by season/disc) with a
/// watched/total roll-up. When the section uses checklist tracking, each item is a
/// toggle; toggling persists the whole list via `onSave` (the Markdown body stays
/// the source of truth). A "Sync" action imports from external providers. Structural
/// edits (add/remove/reorder) are done in the Markdown body for now.
/// The "Sync" action for an episodes section: a button plus its import dialog,
/// self-contained so it can live in the section header. Renders nothing when
/// syncing isn't available (read-only, or no entity/revision to guard against).
export function EpisodeSyncButton({
  episodes,
  disabled,
  entityId,
  revision,
}: {
  episodes: EntityEpisodes;
  disabled: boolean;
  entityId?: string;
  revision?: string;
}) {
  const [open, setOpen] = useState(false);

  if (disabled || !entityId || revision == null) return null;

  return (
    <>
      <Button type="button" variant="outline" size="sm" onClick={() => setOpen(true)}>
        <RefreshCwIcon data-icon="inline-start" />
        <Trans>Sync</Trans>
      </Button>
      <EpisodeSyncDialog
        open={open}
        onOpenChange={setOpen}
        entityId={entityId}
        revision={revision}
        episodes={episodes}
      />
    </>
  );
}

export function EntityEpisodesPanel({
  episodes,
  disabled,
  saving,
  onToggle,
  onSetDate,
  relations,
}: {
  episodes: EntityEpisodes;
  disabled: boolean;
  saving: boolean;
  // Toggling sends only the changed episode (its group label + key, with the
  // in-group index as the fallback locator), not the whole list; the core
  // stamps/clears the ✅ completion date.
  onToggle: (group: string, key: string, index: number, watched: boolean) => void;
  // Sets a checked episode's ✅ completion date (clicking the date on its row).
  onSetDate?: (group: string, key: string, index: number, date: string) => void;
  relations: Relation[];
}) {
  const { t } = useLingui();
  const formatDate = useIsoDateFormat();
  const checklist = episodes.tracking === "checklist";
  const percent = episodes.total > 0 ? Math.round((episodes.watched / episodes.total) * 100) : 0;
  const interactive = checklist && !disabled && !saving;

  function toggle(groupIndex: number, itemIndex: number) {
    if (!interactive) return;
    const group = episodes.groups[groupIndex];
    const item = group.items[itemIndex];
    onToggle(group.label, item.key, itemIndex, !item.watched);
  }

  if (episodes.total === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        <Trans>
          No items yet — sync from a provider or add a list under the{" "}
          <code>{episodes.heading}</code> heading.
        </Trans>
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {checklist ? (
        <div className="flex items-center gap-3">
          <div className="h-2 flex-1 overflow-hidden rounded-full bg-muted">
            <div
              className="h-full rounded-full bg-primary transition-all"
              style={{ width: `${percent}%` }}
            />
          </div>
          <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
            {episodes.watched}/{episodes.total}
          </span>
        </div>
      ) : null}

      <div className="flex flex-col gap-3">
        {episodes.groups.map((group, groupIndex) => (
          <div key={groupIndex} className="flex flex-col gap-1">
            {group.label ? (
              <h4 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
                {group.label}
              </h4>
            ) : null}
            <ul className="flex flex-col">
              {group.items.map((item, itemIndex) => {
                const hasTitle = item.title.trim().length > 0;
                const title = hasTitle ? (
                  <InlineMarkdown markdown={item.title} relations={relations} />
                ) : item.key ? null : (
                  "—"
                );
                const keyLabel = item.key ? (
                  <span className="shrink-0 text-muted-foreground tabular-nums">{item.key}</span>
                ) : null;
                // A checked episode's ✅ date is editable: the overlaid transparent
                // date input opens the native picker on click; `stopPropagation`
                // keeps that click from toggling the row. Chrome only opens the
                // picker from the (invisible) calendar icon, so the click handler
                // calls `showPicker()` explicitly.
                const editableDone = checklist && interactive && item.watched && Boolean(item.done);
                const doneLabel = item.done ? (
                  editableDone ? (
                    <span
                      className="relative inline-flex cursor-pointer rounded px-0.5 hover:bg-accent hover:text-accent-foreground"
                      title={t`Change completion date`}
                      onClick={(event) => event.stopPropagation()}
                    >
                      ✅ {formatDate(item.done)}
                      <input
                        type="date"
                        value={item.done}
                        aria-label={t`Change completion date`}
                        onClick={(event) => {
                          event.stopPropagation();
                          try {
                            event.currentTarget.showPicker();
                          } catch {
                            // Unsupported or already open — segment focus still works.
                          }
                        }}
                        onChange={(event) => {
                          if (event.target.value) {
                            onSetDate?.(group.label, item.key, itemIndex, event.target.value);
                          }
                        }}
                        className="absolute inset-0 cursor-pointer opacity-0"
                      />
                    </span>
                  ) : (
                    <span>✅ {formatDate(item.done)}</span>
                  )
                ) : null;
                // With both a 📅 and a ✅ date the row runs out of width on
                // mobile and the title gets crushed, so drop the dates onto
                // their own line below the title there (desktop stays inline).
                const bothDates = Boolean(item.date && item.done);
                const dateLabel =
                  item.date || item.done ? (
                    <span
                      className={cn(
                        "shrink-0 space-x-2 text-xs text-muted-foreground tabular-nums",
                        bothDates && "max-sm:mt-0.5 max-sm:basis-full max-sm:pl-6",
                      )}
                    >
                      {item.date ? <span>📅 {formatDate(item.date)}</span> : null}
                      {doneLabel}
                    </span>
                  ) : null;

                // Non-checklist rows are plain, selectable text (no interactive
                // wrapper) so the list can be copied; checklist rows are a
                // role="checkbox" div so links inside the title stay valid and
                // clickable while the row toggles.
                if (!checklist) {
                  return (
                    <li
                      key={itemIndex}
                      className="flex items-start gap-2 px-2 py-1.5 text-sm max-sm:flex-wrap"
                    >
                      <span className="mt-0.5 text-xs text-muted-foreground">•</span>
                      {keyLabel}
                      <span className="min-w-0 flex-1">{title}</span>
                      {dateLabel}
                    </li>
                  );
                }

                return (
                  <li key={itemIndex}>
                    <div
                      role="checkbox"
                      aria-checked={item.watched}
                      aria-disabled={interactive ? undefined : true}
                      tabIndex={interactive ? 0 : -1}
                      onClick={interactive ? () => toggle(groupIndex, itemIndex) : undefined}
                      onKeyDown={
                        interactive
                          ? (event) => {
                              if (event.key === "Enter" || event.key === " ") {
                                event.preventDefault();
                                toggle(groupIndex, itemIndex);
                              }
                            }
                          : undefined
                      }
                      className={cn(
                        "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors max-sm:flex-wrap",
                        interactive ? "cursor-pointer hover:bg-accent" : "cursor-default",
                      )}
                    >
                      <span
                        className={cn(
                          "flex size-4 shrink-0 items-center justify-center rounded border",
                          item.watched
                            ? "border-primary bg-primary text-primary-foreground"
                            : "border-input",
                        )}
                      >
                        {item.watched ? <CheckIcon className="size-3" /> : null}
                      </span>
                      {keyLabel}
                      <span
                        className={cn(
                          "min-w-0 flex-1 truncate",
                          item.watched && "text-muted-foreground",
                        )}
                      >
                        {title}
                      </span>
                      {dateLabel}
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
}
