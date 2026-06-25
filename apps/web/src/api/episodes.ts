import {
  fetchEpisodes,
  importEpisodes,
  updateEpisodes,
  type FetchEpisodesRequest,
  type ImportEpisodesRequest,
  type UpdateEpisodesRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/** Rewrites an entity's episodes section; returns the refreshed entity detail. */
export function saveEpisodes(id: string, request: UpdateEpisodesRequest) {
  return updateEpisodes(id, request, undefined, apiFetch);
}

/** Lists episode sources + fetches one provider's structured episodes. */
export function fetchEpisodeSources(id: string, request: FetchEpisodesRequest, init?: RequestInit) {
  return fetchEpisodes(id, request, init, apiFetch);
}

/** Merges provider episodes into the entity; returns the refreshed entity detail. */
export function syncEpisodes(id: string, request: ImportEpisodesRequest) {
  return importEpisodes(id, request, undefined, apiFetch);
}
