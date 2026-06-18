import { apiFetch } from "@/api/client";
import type {
  LanguagesResponse,
  PathSuggestionsResponse,
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

export async function getPathSuggestions(path: string, init?: RequestInit) {
  const params = new URLSearchParams();
  params.set("path", path);
  const response = await apiFetch(`/api/settings/path-suggestions?${params.toString()}`, init);
  return (await response.json()) as PathSuggestionsResponse;
}
