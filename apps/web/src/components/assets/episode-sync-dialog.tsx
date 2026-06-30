import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckIcon,
  ChevronRightIcon,
  DownloadIcon,
  MinusIcon,
  RefreshCwIcon,
  XIcon,
} from "lucide-react";

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
import type {
  EntityEpisodes,
  EpisodeGroup,
  ProviderEpisodeGroup,
  ProviderEpisodeItem,
} from "@/types/api";

// Selection is per episode: an id keys a (season label, episode key) pair. The
// separator is a NUL so it never collides with real labels/keys.
const SEP = "\u0000";
const itemId = (label: string, key: string) => `${label}${SEP}${key}`;

/// Pulls a list from an external provider and merges it into the entity. The list is
/// schema-defined and need not be episodes — it may be tracks, chapters, a walkthrough,
/// etc. — so the wording stays generic ("items"). The preview shows every fetched item
/// (groups are collapsible for long TheTVDB-style lists); a tick means "write this from
/// the provider". Items already in the user's list start unticked so their edits are kept
/// — ticking one updates its title; unticked new items (e.g. Bangumi's semi-specials) are
/// skipped. One grouping choice mirrors the provider's structure or flattens it; progress
/// is always preserved server-side.
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
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
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

  // Reset scope/grouping defaults whenever a fresh fetch lands (per the agreed table:
  // empty/grouped entity → all seasons grouped; flat entity → one season flat). Only
  // episodes *not* already in the user's list start ticked, so a sync defaults to
  // adding new episodes without touching existing titles; long lists collapse.
  useEffect(() => {
    if (!data) return;
    const labels = groups.map((group) => group.label);
    const willGroup = seasoned && existingIsFlat ? false : seasoned;
    const localKeys = new Set<string>();
    for (const group of episodes.groups) {
      for (const item of group.items) {
        localKeys.add(itemId(group.label.trim().toLowerCase(), item.key.trim()));
      }
    }
    const isNew = (seasonLabel: string, key: string) => {
      const target = willGroup ? seasonLabel : "";
      return !localKeys.has(itemId(target.trim().toLowerCase(), key.trim()));
    };
    const freshIds = (gs: ProviderEpisodeGroup[]) =>
      new Set(
        gs.flatMap((group) =>
          group.items
            .filter((item) => isNew(group.label, item.key))
            .map((item) => itemId(group.label, item.key)),
        ),
      );
    if (seasoned && existingIsFlat) {
      setGroupBySeason(false);
      setSelected(freshIds(groups.slice(0, 1)));
      setExpanded(new Set(labels.slice(0, 1)));
    } else {
      setGroupBySeason(seasoned);
      setSelected(freshIds(groups));
      setExpanded(new Set(labels.length <= 3 ? labels : []));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);

  const allIds = useMemo(
    () => groups.flatMap((group) => group.items.map((item) => itemId(group.label, item.key))),
    [groups],
  );
  const allSelected = allIds.length > 0 && allIds.every((id) => selected.has(id));

  function toggleItem(label: string, key: string) {
    setSelected((current) => {
      const next = new Set(current);
      const id = itemId(label, key);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleGroup(group: ProviderEpisodeGroup) {
    const ids = group.items.map((item) => itemId(group.label, item.key));
    const everySelected = ids.every((id) => selected.has(id));
    setSelected((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (everySelected) next.delete(id);
        else next.add(id);
      }
      return next;
    });
  }

  function toggleExpand(label: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(label)) next.delete(label);
      else next.add(label);
      return next;
    });
  }

  // Build the payload groups (selected items only, grouped or flattened) to merge.
  const payloadGroups: EpisodeGroup[] = useMemo(() => {
    const picked = groups
      .map((group) => ({
        label: group.label,
        items: group.items.filter((item) => selected.has(itemId(group.label, item.key))),
      }))
      .filter((group) => group.items.length > 0);
    const toEpisode = (item: ProviderEpisodeItem) => ({
      key: item.key,
      title: item.title,
      watched: false,
      // Carry the provider's air/release date so the core writes the 📅 suffix.
      date: item.date ?? undefined,
    });
    if (groupBySeason && seasoned) {
      return picked.map((group) => ({ label: group.label, items: group.items.map(toEpisode) }));
    }
    return [{ label: "", items: picked.flatMap((group) => group.items).map(toEpisode) }];
  }, [groups, selected, groupBySeason, seasoned]);

  // The local episode (target group, key) pairs, to mark already-tracked rows. The
  // target group depends on whether we keep seasons (import label) or flatten ("").
  const existingKeys = useMemo(() => {
    const set = new Set<string>();
    for (const group of episodes.groups) {
      for (const item of group.items) {
        set.add(itemId(group.label.trim().toLowerCase(), item.key.trim()));
      }
    }
    return set;
  }, [episodes]);
  function isTracked(groupLabel: string, key: string) {
    const target = groupBySeason && seasoned ? groupLabel : "";
    return existingKeys.has(itemId(target.trim().toLowerCase(), key.trim()));
  }

  const { incoming, already } = useMemo(
    () => previewCounts(episodes, payloadGroups),
    [episodes, payloadGroups],
  );

  async function apply() {
    setImporting(true);
    setError(undefined);
    try {
      // Ticked items are deliberate writes: overwrite matched titles, add new ones.
      const detail = await syncEpisodes(entityId, { revision, groups: payloadGroups, overwrite: true });
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
  const ready = Boolean(data) && !sources.isPending && !sources.error && !noSources;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Sync from a provider</DialogTitle>
          <DialogDescription>
            Tick the items to write from the provider — new ones are added, ticked existing ones have
            their title updated. Items already in your list start unticked, and your progress is always
            kept.
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

        {ready ? (
          <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
            <span className="tabular-nums">
              {selected.size} of {allIds.length} selected
            </span>
            <button
              type="button"
              onClick={() => setSelected(allSelected ? new Set() : new Set(allIds))}
              className="font-medium text-foreground hover:underline"
            >
              {allSelected ? "Clear all" : "Select all"}
            </button>
          </div>
        ) : null}

        <div className="flex max-h-[55vh] flex-col gap-0.5 overflow-auto rounded-md border p-1">
          {sources.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">Loading</p>
          ) : sources.error ? (
            <p className="p-3 text-center text-sm text-destructive">{errorMessage(sources.error)}</p>
          ) : noSources ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              No provider with a list is linked on this entity.
            </p>
          ) : seasoned ? (
            groups.map((group) => (
              <ProviderGroup
                key={group.label}
                group={group}
                selected={selected}
                expanded={expanded.has(group.label)}
                isTracked={isTracked}
                onToggleGroup={() => toggleGroup(group)}
                onToggleExpand={() => toggleExpand(group.label)}
                onToggleItem={toggleItem}
              />
            ))
          ) : (
            <ul className="flex flex-col">
              {(groups[0]?.items ?? []).map((item) => (
                <ItemRow
                  key={item.key}
                  item={item}
                  checked={selected.has(itemId("", item.key))}
                  tracked={isTracked("", item.key)}
                  onToggle={() => toggleItem("", item.key)}
                />
              ))}
            </ul>
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
            Keep groups
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
            {importing ? "Importing" : `Import (${already > 0 ? `${incoming - already} new · ${already} updated` : `${incoming}`})`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ProviderGroup({
  group,
  selected,
  expanded,
  isTracked,
  onToggleGroup,
  onToggleExpand,
  onToggleItem,
}: {
  group: ProviderEpisodeGroup;
  selected: Set<string>;
  expanded: boolean;
  isTracked: (label: string, key: string) => boolean;
  onToggleGroup: () => void;
  onToggleExpand: () => void;
  onToggleItem: (label: string, key: string) => void;
}) {
  const chosen = group.items.filter((item) => selected.has(itemId(group.label, item.key))).length;
  const state = chosen === 0 ? "none" : chosen === group.items.length ? "all" : "some";

  return (
    <div className="flex flex-col">
      <div className="flex items-center gap-2 rounded-md pr-2 transition-colors hover:bg-accent">
        <button
          type="button"
          onClick={onToggleGroup}
          aria-label={state === "all" ? "Deselect group" : "Select group"}
          className="flex items-center py-2 pl-2"
        >
          <span
            className={cn(
              "flex size-4 shrink-0 items-center justify-center rounded border",
              state === "none" ? "border-input" : "border-primary bg-primary text-primary-foreground",
            )}
          >
            {state === "all" ? <CheckIcon className="size-3" /> : null}
            {state === "some" ? <MinusIcon className="size-3" /> : null}
          </span>
        </button>
        <button
          type="button"
          onClick={onToggleExpand}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-1.5 py-2 text-left"
        >
          <ChevronRightIcon
            className={cn("size-4 shrink-0 text-muted-foreground transition-transform", expanded && "rotate-90")}
          />
          <span className="min-w-0 flex-1 truncate text-sm font-medium">{group.label || "Items"}</span>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {chosen}/{group.items.length}
          </span>
        </button>
      </div>
      {expanded ? (
        <ul className="flex flex-col pl-6">
          {group.items.map((item) => (
            <ItemRow
              key={item.key}
              item={item}
              checked={selected.has(itemId(group.label, item.key))}
              tracked={isTracked(group.label, item.key)}
              onToggle={() => onToggleItem(group.label, item.key)}
            />
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function ItemRow({
  item,
  checked,
  tracked,
  onToggle,
}: {
  item: ProviderEpisodeItem;
  checked: boolean;
  tracked: boolean;
  onToggle: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        onClick={onToggle}
        aria-pressed={checked}
        className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm transition-colors hover:bg-accent"
      >
        <span
          className={cn(
            "flex size-4 shrink-0 items-center justify-center rounded border",
            checked ? "border-primary bg-primary text-primary-foreground" : "border-input",
          )}
        >
          {checked ? <CheckIcon className="size-3" /> : null}
        </span>
        {item.key ? (
          <span className="shrink-0 tabular-nums text-xs text-muted-foreground">{item.key}</span>
        ) : null}
        <span className="min-w-0 flex-1 truncate">{item.title || "—"}</span>
        {item.date ? (
          <span className="shrink-0 tabular-nums text-xs text-muted-foreground">📅 {item.date}</span>
        ) : null}
        {tracked ? (
          <span className="shrink-0 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            in list
          </span>
        ) : null}
      </button>
    </li>
  );
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
