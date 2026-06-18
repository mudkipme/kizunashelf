import { getPathSuggestions as requestPathSuggestions } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";
import type {
  LanguagesResponse,
  SaveSettingsRequest,
  SettingsConfigResponse,
  VaultTemplatesResponse,
} from "@/types/config";

export async function getSettingsConfig(init?: RequestInit) {
  const response = await apiFetch("/api/settings/config", init);
  return (await response.json()) as SettingsConfigResponse;
}

export async function getVaultTemplates(init?: RequestInit) {
  const response = await apiFetch("/api/vault-templates", init);
  return (await response.json()) as VaultTemplatesResponse;
}

export async function getLanguages(init?: RequestInit) {
  const response = await apiFetch("/api/languages", init);
  return (await response.json()) as LanguagesResponse;
}

export async function saveSettingsConfig(request: SaveSettingsRequest) {
  const response = await apiFetch("/api/settings/config", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
  return (await response.json()) as SettingsConfigResponse;
}

// Uses the generated, Zod-validated client (the rest of the app's pattern) so the
// query params stay in sync with the OpenAPI contract. `base` is a vault-relative
// directory the suggestions are rooted at (e.g. the taxonomy root for a type's
// folder path); omitted means the vault root.
export function getPathSuggestions(path: string, base?: string, init?: RequestInit) {
  return requestPathSuggestions({ path, base }, init, apiFetch);
}
