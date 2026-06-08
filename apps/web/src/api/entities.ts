import {
  createEntity,
  deleteEntity,
  getCapabilities,
  searchExternalSources,
  updateEntity,
  type CreateEntityRequest,
  type DeleteEntityRequest,
  type SearchExternalSourcesParams,
  type UpdateEntityRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

export function getAppCapabilities(init?: RequestInit) {
  return getCapabilities(init, apiFetch);
}

export function saveEntity(id: string, request: UpdateEntityRequest) {
  return updateEntity(id, request, undefined, apiFetch);
}

export function addEntity(request: CreateEntityRequest) {
  return createEntity(request, undefined, apiFetch);
}

export function removeEntity(id: string, request: DeleteEntityRequest) {
  return deleteEntity(id, request, undefined, apiFetch);
}

export function searchSources(params: SearchExternalSourcesParams, init?: RequestInit) {
  return searchExternalSources(params, init, apiFetch);
}
