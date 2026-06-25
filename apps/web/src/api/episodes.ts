import { updateEpisodes, type UpdateEpisodesRequest } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/** Rewrites an entity's episodes section; returns the refreshed entity detail. */
export function saveEpisodes(id: string, request: UpdateEpisodesRequest) {
  return updateEpisodes(id, request, undefined, apiFetch);
}
