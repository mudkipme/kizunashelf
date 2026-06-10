import { getExternalProviderCatalog } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

export function getProviderCatalog(init?: RequestInit) {
  return getExternalProviderCatalog(init, apiFetch);
}
