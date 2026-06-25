import { useState } from "react";
import { CheckIcon, RefreshCwIcon } from "lucide-react";

import { EpisodeSyncDialog } from "@/components/assets/episode-sync-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { EntityEpisodes, EpisodeGroup } from "@/types/api";

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
}: {
  episodes: EntityEpisodes;
  disabled: boolean;
  saving: boolean;
  onSave: (groups: EpisodeGroup[]) => void;
  entityId?: string;
  revision?: string;
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
            No {episodes.itemNoun.toLowerCase()}s yet — sync from a provider or add a list under the{" "}
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
              {group.items.map((item, itemIndex) => (
                <li key={itemIndex}>
                  <button
                    type="button"
                    disabled={!interactive}
                    onClick={() => toggle(groupIndex, itemIndex)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                      interactive && "hover:bg-accent",
                      !checklist && "cursor-default",
                    )}
                    aria-pressed={item.watched}
                  >
                    {checklist ? (
                      <span
                        className={cn(
                          "flex size-4 shrink-0 items-center justify-center rounded border",
                          item.watched ? "border-primary bg-primary text-primary-foreground" : "border-input",
                        )}
                      >
                        {item.watched ? <CheckIcon className="size-3" /> : null}
                      </span>
                    ) : (
                      <span className="text-xs text-muted-foreground">•</span>
                    )}
                    {item.key ? (
                      <span className="shrink-0 tabular-nums text-muted-foreground">{item.key}</span>
                    ) : null}
                    <span className={cn("min-w-0 truncate", item.watched && checklist && "text-muted-foreground")}>
                      {item.title || (item.key ? "" : "—")}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
}
