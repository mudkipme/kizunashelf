import {
  fetchEpisodes,
  importEpisodes,
  type FetchEpisodesRequest,
  type ImportEpisodesRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";
import { postLogActivity } from "@/api/log";
import { todayLocal } from "@/lib/date";

/// Checks/unchecks one episode by logging it through `/log` (the single write
/// path): checking is `op: add` (stamps `✅`, writes a daily-note line for loggable
/// types), unchecking is `op: remove` (the exact inverse). Returns the log result —
/// `.entity` is the refreshed detail. Located by group + key, index as fallback.
export function logEpisodeWatched(
  id: string,
  args: { revision: string; group: string; key: string; index: number; watched: boolean },
) {
  return postLogActivity(id, {
    op: args.watched ? "add" : "remove",
    kind: "progress",
    revision: args.revision,
    episode: { group: args.group, key: args.key, index: args.index },
    // The user's local date (the server never assumes UTC "today"); on uncheck the
    // server derives the date from the episode's stored `✅` instead.
    date: todayLocal(),
  });
}

/** Lists episode sources + fetches one provider's structured episodes. */
export function fetchEpisodeSources(id: string, request: FetchEpisodesRequest, init?: RequestInit) {
  return fetchEpisodes(id, request, init, apiFetch);
}

/** Merges provider episodes into the entity; returns the refreshed entity detail. */
export function syncEpisodes(id: string, request: ImportEpisodesRequest) {
  return importEpisodes(id, request, undefined, apiFetch);
}
