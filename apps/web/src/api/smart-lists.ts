import {
  createSmartList,
  getSmartListSuggestions,
  createSuggestedSmartLists,
  setSmartListHome,
  deleteSmartList,
  getSmartList,
  getSmartListResults,
  previewSmartList,
  updateSmartList,
  type CreateSmartListRequest,
  type GetSmartListResultsParams,
  type SmartListPreviewRequest,
  type UpdateSmartListRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

export function fetchSmartList(id: string, init?: RequestInit) {
  return getSmartList(id, init, apiFetch);
}

export function fetchSmartListResults(
  id: string,
  params?: GetSmartListResultsParams,
  init?: RequestInit,
) {
  return getSmartListResults(id, params, init, apiFetch);
}

export function addSmartList(request: CreateSmartListRequest) {
  return createSmartList(request, undefined, apiFetch);
}

export function saveSmartList(id: string, request: UpdateSmartListRequest) {
  return updateSmartList(id, request, undefined, apiFetch);
}

export function removeSmartList(id: string) {
  return deleteSmartList(id, undefined, apiFetch);
}

export function fetchSmartListPreview(request: SmartListPreviewRequest, init?: RequestInit) {
  return previewSmartList(request, init, apiFetch);
}

export function fetchSmartListSuggestions(language: string, init?: RequestInit) {
  return getSmartListSuggestions({ language }, init, apiFetch);
}

export function addSuggestedSmartLists(language: string, suggestionIds?: string[]) {
  return createSuggestedSmartLists({ language, suggestionIds }, undefined, apiFetch);
}

export function setHomeVisibility(id: string, revision: string, showOnHome: boolean) {
  return setSmartListHome(id, { revision, showOnHome }, undefined, apiFetch);
}
