import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckIcon, DownloadIcon, RefreshCwIcon, XIcon } from "lucide-react";

import { errorMessage } from "@/api/client";
import { fetchEpisodeSources, syncEpisodes } from "@/api/episodes";
import { queryKeys } from "@/api/queries";
import { useTitleLanguage } from "@/lib/language";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Select } from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { EntityEpisodes, EpisodeGroup, ProviderEpisodeGroup } from "@/types/api";

/// Pulls episodes from an external provider and merges them into the entity. The
/// import mirrors the provider's structure with one explicit scope/grouping choice
/// (see the agreed UX); merging always keeps local episodes + watched ticks and
/// only fills empty titles (handled by the server).
export function EpisodeSyncDialog({
  open,
  onOpenChange,
  entityId,
  revision,
  episodes,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  revision: string;
  episodes: EntityEpisodes;
}) {
  const queryClient = useQueryClient();
  const language = useTitleLanguage();
  const [provider, setProvider] = useState<string>();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [groupBySeason, setGroupBySeason] = useState(true);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string>();

  const sources = useQuery({
    queryKey: ["episodeSources", entityId, provider ?? "", language],
    queryFn: ({ signal }) =>
      fetchEpisodeSources(entityId, { ...(provider ? { provider } : {}), language }, { signal }),
    enabled: open,
  });

  const data = sources.data;
  const groups = useMemo(() => data?.groups ?? [], [data]);
  // A provider with a single unlabeled group is "flat" (one season/disc).
  const seasoned = groups.length > 1 || (groups.length === 1 && groups[0].label.trim() !== "");
  const existingIsFlat =
    episodes.groups.length === 0 ||
    (episodes.groups.length === 1 && episodes.groups[0].label.trim() === "");

  // Reset the scope/grouping defaults whenever a fresh fetch lands (per the agreed
  // table: empty/grouped entity → all seasons grouped; flat entity → one season flat).
  useEffect(() => {
    if (!data) return;
    const labels = groups.map((group) => group.label);
    if (seasoned && existingIsFlat) {
      setGroupBySeason(false);
      setSelected(new Set(labels.slice(0, 1)));
    } else {
      setGroupBySeason(seasoned);
      setSelected(new Set(labels));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);

  const chosen = groups.filter((group) => selected.has(group.label));

  // Build the payload groups (subset, grouped or flattened) the server will merge.
  const payloadGroups: EpisodeGroup[] = useMemo(() => {
    if (groupBySeason && seasoned) {
      return chosen.map((group) => ({
        label: group.label,
        items: group.items.map((item) => ({ key: item.key, title: item.title, watched: false })),
      }));
    }
    return [
      {
        label: "",
        items: chosen.flatMap((group) => group.items).map((item) => ({
          key: item.key,
          title: item.title,
          watched: false,
        })),
      },
    ];
  }, [chosen, groupBySeason, seasoned]);

  const { incoming, already } = useMemo(
    () => previewCounts(episodes, payloadGroups),
    [episodes, payloadGroups],
  );

  async function apply() {
    setImporting(true);
    setError(undefined);
    try {
      const detail = await syncEpisodes(entityId, { revision, groups: payloadGroups });
      queryClient.setQueryData(queryKeys.entity(entityId), detail);
      void queryClient.invalidateQueries({ queryKey: ["entities"] });
      onOpenChange(false);
    } catch (importError) {
      setError(errorMessage(importError));
    } finally {
      setImporting(false);
    }
  }

  const noSources = Boolean(data && data.sources.length === 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Sync from a provider</DialogTitle>
          <DialogDescription>
            Import from a provider, merging into your list — watched state and your own entries are kept.
          </DialogDescription>
        </DialogHeader>

        {data && data.sources.length > 1 ? (
          <label className="text-sm font-medium">
            Provider
            <Select
              value={data.provider}
              onChange={(event) => setProvider(event.target.value)}
              className="mt-1"
            >
              {data.sources.map((source) => (
                <option key={source.provider} value={source.provider}>
                  {source.label}
                </option>
              ))}
            </Select>
          </label>
        ) : null}

        <div className="flex max-h-72 flex-col gap-2 overflow-auto">
          {sources.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">Loading</p>
          ) : sources.error ? (
            <p className="p-3 text-center text-sm text-destructive">{errorMessage(sources.error)}</p>
          ) : noSources ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              No provider with episodes is linked on this entity.
            </p>
          ) : seasoned ? (
            <>
              {groups.map((group) => (
                <SeasonRow
                  key={group.label}
                  group={group}
                  checked={selected.has(group.label)}
                  onToggle={() => toggle(setSelected, group.label)}
                />
              ))}
            </>
          ) : (
            <p className="p-3 text-sm text-muted-foreground">
              {groups[0]?.items.length ?? 0} items available.
            </p>
          )}
        </div>

        {seasoned ? (
          <button
            type="button"
            onClick={() => setGroupBySeason((value) => !value)}
            className="flex items-center gap-2 self-start text-sm"
          >
            <span
              className={cn(
                "flex size-4 items-center justify-center rounded border",
                groupBySeason ? "border-primary bg-primary text-primary-foreground" : "border-input",
              )}
            >
              {groupBySeason ? <CheckIcon className="size-3" /> : null}
            </span>
            Keep seasons as groups
          </button>
        ) : null}

        {error ? <p className="text-xs text-destructive">{error}</p> : null}

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={importing}>
            <XIcon data-icon="inline-start" />
            Cancel
          </Button>
          <Button
            type="button"
            onClick={() => void apply()}
            disabled={importing || noSources || incoming === 0}
          >
            {importing ? <RefreshCwIcon data-icon="inline-start" className="animate-spin" /> : <DownloadIcon data-icon="inline-start" />}
            {importing ? "Importing" : `Import (${already > 0 ? `${incoming - already} new · ${already} tracked` : `${incoming}`})`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SeasonRow({
  group,
  checked,
  onToggle,
}: {
  group: ProviderEpisodeGroup;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex items-center gap-2 rounded-md p-2 text-left transition-colors hover:bg-accent"
    >
      <span
        className={cn(
          "flex size-4 shrink-0 items-center justify-center rounded border",
          checked ? "border-primary bg-primary text-primary-foreground" : "border-input",
        )}
      >
        {checked ? <CheckIcon className="size-3" /> : null}
      </span>
      <span className="min-w-0 flex-1 truncate text-sm font-medium">{group.label || "Episodes"}</span>
      <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{group.items.length}</span>
    </button>
  );
}

function toggle(setSelected: (updater: (current: Set<string>) => Set<string>) => void, label: string) {
  setSelected((current) => {
    const next = new Set(current);
    if (next.has(label)) next.delete(label);
    else next.add(label);
    return next;
  });
}

/// Counts how many of the payload items already exist locally (matched by target
/// group + key), to show "N new · M already tracked".
function previewCounts(episodes: EntityEpisodes, payloadGroups: EpisodeGroup[]) {
  const existing = new Set<string>();
  for (const group of episodes.groups) {
    for (const item of group.items) existing.add(`${group.label.trim().toLowerCase()}|${item.key.trim()}`);
  }
  let incoming = 0;
  let already = 0;
  for (const group of payloadGroups) {
    for (const item of group.items) {
      incoming += 1;
      if (existing.has(`${group.label.trim().toLowerCase()}|${item.key.trim()}`)) already += 1;
    }
  }
  return { incoming, already };
}
