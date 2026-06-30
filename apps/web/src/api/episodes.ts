import {
  fetchEpisodes,
  importEpisodes,
  toggleEpisode,
  type FetchEpisodesRequest,
  type ImportEpisodesRequest,
  type ToggleEpisodeRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/** Checks/unchecks a single episode (located by group + key, else index); returns the refreshed detail. */
export function setEpisodeWatched(id: string, request: ToggleEpisodeRequest) {
  return toggleEpisode(id, request, undefined, apiFetch);
}

/** Lists episode sources + fetches one provider's structured episodes. */
export function fetchEpisodeSources(id: string, request: FetchEpisodesRequest, init?: RequestInit) {
  return fetchEpisodes(id, request, init, apiFetch);
}

/** Merges provider episodes into the entity; returns the refreshed entity detail. */
export function syncEpisodes(id: string, request: ImportEpisodesRequest) {
  return importEpisodes(id, request, undefined, apiFetch);
}
