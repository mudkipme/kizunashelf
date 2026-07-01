import {
  fetchEpisodes,
  importEpisodes,
  toggleEpisode,
  type FetchEpisodesRequest,
  type ImportEpisodesRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/// Checks/unchecks one episode through `/episodes/watch` (its dedicated write
/// path). Checking stamps `date` as the episode's `✅` completion date; unchecking
/// clears it. `date` is the client's local date, so re-checking a watched episode
/// with a different date is how the completion date is edited. Returns the refreshed
/// entity detail. Located by group + key, index as fallback. Independent of
/// daily-note logging (`/log`) — the two never share state.
export function setEpisodeWatched(
  id: string,
  args: {
    revision: string;
    group: string;
    key: string;
    index: number;
    watched: boolean;
    date: string;
  },
) {
  return toggleEpisode(
    id,
    {
      revision: args.revision,
      group: args.group,
      key: args.key,
      index: args.index,
      watched: args.watched,
      date: args.date,
    },
    undefined,
    apiFetch,
  );
}

/** Lists episode sources + fetches one provider's structured episodes. */
export function fetchEpisodeSources(id: string, request: FetchEpisodesRequest, init?: RequestInit) {
  return fetchEpisodes(id, request, init, apiFetch);
}

/** Merges provider episodes into the entity; returns the refreshed entity detail. */
export function syncEpisodes(id: string, request: ImportEpisodesRequest) {
  return importEpisodes(id, request, undefined, apiFetch);
}
