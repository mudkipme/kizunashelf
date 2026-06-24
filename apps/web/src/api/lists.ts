import {
  addListItem,
  createList,
  deleteList,
  getList,
  getLists,
  updateList,
  type AddListItemRequest,
  type CreateListRequest,
  type UpdateListRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

export function fetchLists(init?: RequestInit) {
  return getLists(init, apiFetch);
}

export function fetchList(id: string, init?: RequestInit) {
  return getList(id, init, apiFetch);
}

export function addList(request: CreateListRequest) {
  return createList(request, undefined, apiFetch);
}

export function saveList(id: string, request: UpdateListRequest) {
  return updateList(id, request, undefined, apiFetch);
}

export function removeList(id: string) {
  return deleteList(id, undefined, apiFetch);
}

export function addItemToList(id: string, request: AddListItemRequest) {
  return addListItem(id, request, undefined, apiFetch);
}
