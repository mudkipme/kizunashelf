import { useState } from "react";
import { CheckIcon, RefreshCwIcon } from "lucide-react";

import { EpisodeSyncDialog } from "@/components/assets/episode-sync-dialog";
import { InlineMarkdown } from "@/components/assets/markdown-view";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { EntityEpisodes, EpisodeGroup, Relation } from "@/types/api";

/// Displays an entity's episodes/tracks list (grouped by season/disc) with a
/// watched/total roll-up. When the section uses checklist tracking, each item is a
/// toggle; toggling persists the whole list via `onSave` (the Markdown body stays
/// the source of truth). A "Sync" action imports from external providers. Structural
/// edits (add/remove/reorder) are done in the Markdown body for now.
export function EntityEpisodesPanel({
  episodes,
  disabled,
  saving,
  onSave,
  entityId,
  revision,
  relations,
}: {
  episodes: EntityEpisodes;
  disabled: boolean;
  saving: boolean;
  onSave: (groups: EpisodeGroup[]) => void;
  entityId?: string;
  revision?: string;
  relations: Relation[];
}) {
  const checklist = episodes.tracking === "checklist";
  const percent = episodes.total > 0 ? Math.round((episodes.watched / episodes.total) * 100) : 0;
  const interactive = checklist && !disabled && !saving;
  const [syncOpen, setSyncOpen] = useState(false);
  const canSync = Boolean(entityId && revision != null && !disabled);

  function toggle(groupIndex: number, itemIndex: number) {
    if (!interactive) return;
    const groups = episodes.groups.map((group, gi) =>
      gi !== groupIndex
        ? group
        : {
            ...group,
            items: group.items.map((item, ii) =>
              ii !== itemIndex ? item : { ...item, watched: !item.watched },
            ),
          },
    );
    onSave(groups);
  }

  const syncButton = canSync ? (
    <Button type="button" variant="outline" size="sm" onClick={() => setSyncOpen(true)}>
      <RefreshCwIcon data-icon="inline-start" />
      Sync
    </Button>
  ) : null;
  const syncDialog =
    canSync && entityId && revision != null ? (
      <EpisodeSyncDialog
        open={syncOpen}
        onOpenChange={setSyncOpen}
        entityId={entityId}
        revision={revision}
        episodes={episodes}
      />
    ) : null;

  if (episodes.total === 0) {
    return (
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <p className="text-sm text-muted-foreground">
            No items yet — sync from a provider or add a list under the{" "}
            <code>{episodes.heading}</code> heading.
          </p>
          {syncButton}
        </div>
        {syncDialog}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {syncButton ? <div className="flex justify-end">{syncButton}</div> : null}
      {syncDialog}
      {checklist ? (
        <div className="flex items-center gap-3">
          <div className="h-2 flex-1 overflow-hidden rounded-full bg-muted">
            <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${percent}%` }} />
          </div>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {episodes.watched}/{episodes.total}
          </span>
        </div>
      ) : null}

      <div className="flex flex-col gap-3">
        {episodes.groups.map((group, groupIndex) => (
          <div key={groupIndex} className="flex flex-col gap-1">
            {group.label ? (
              <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
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
                  <span className="shrink-0 tabular-nums text-muted-foreground">{item.key}</span>
                ) : null;

                // Non-checklist rows are plain, selectable text (no interactive
                // wrapper) so the list can be copied; checklist rows are a
                // role="checkbox" div so links inside the title stay valid and
                // clickable while the row toggles.
                if (!checklist) {
                  return (
                    <li key={itemIndex} className="flex items-start gap-2 px-2 py-1.5 text-sm">
                      <span className="mt-0.5 text-xs text-muted-foreground">•</span>
                      {keyLabel}
                      <span className="min-w-0">{title}</span>
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
                        "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
                        interactive ? "cursor-pointer hover:bg-accent" : "cursor-default",
                      )}
                    >
                      <span
                        className={cn(
                          "flex size-4 shrink-0 items-center justify-center rounded border",
                          item.watched ? "border-primary bg-primary text-primary-foreground" : "border-input",
                        )}
                      >
                        {item.watched ? <CheckIcon className="size-3" /> : null}
                      </span>
                      {keyLabel}
                      <span className={cn("min-w-0 truncate", item.watched && "text-muted-foreground")}>
                        {title}
                      </span>
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
