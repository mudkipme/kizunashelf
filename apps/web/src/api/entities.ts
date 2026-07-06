import {
  cancelAssetJob,
  createAssetJob,
  createEntity,
  deleteEntity,
  downloadEntityAssets,
  getAssetJob,
  getCapabilities,
  listAssetJobs,
  quickAddExternalEntity,
  searchExternalSources,
  updateEntity,
  uploadEntityAsset,
  type AssetDownloadJobRequest,
  type AssetDownloadRequest,
  type AssetUploadRequest,
  type CreateEntityRequest,
  type DeleteEntityRequest,
  type QuickAddRequest,
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

export function quickAddEntity(request: QuickAddRequest) {
  return quickAddExternalEntity(request, undefined, apiFetch);
}

export function downloadAssets(id: string, request: AssetDownloadRequest) {
  return downloadEntityAssets(id, request, undefined, apiFetch);
}

export function uploadAsset(id: string, request: AssetUploadRequest) {
  return uploadEntityAsset(id, request, undefined, apiFetch);
}

export function startAssetJob(request: AssetDownloadJobRequest) {
  return createAssetJob(request, undefined, apiFetch);
}

export function fetchAssetJob(id: string, init?: RequestInit) {
  return getAssetJob(id, init, apiFetch);
}

export function fetchAssetJobs(init?: RequestInit) {
  return listAssetJobs(init, apiFetch);
}

export function stopAssetJob(id: string) {
  return cancelAssetJob(id, undefined, apiFetch);
}
