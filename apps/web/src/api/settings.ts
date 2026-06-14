import { apiFetch } from "@/api/client";
import type {
  PathSuggestionsResponse,
  SaveSettingsRequest,
  SettingsConfigResponse,
} from "@/types/config";

export async function getSettingsConfig(init?: RequestInit) {
  const response = await apiFetch("/api/settings/config", init);
  return (await response.json()) as SettingsConfigResponse;
}

export async function saveSettingsConfig(request: SaveSettingsRequest) {
  const response = await apiFetch("/api/settings/config", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
  return (await response.json()) as SettingsConfigResponse;
}

export async function getPathSuggestions(path: string, base?: string, init?: RequestInit) {
  const params = new URLSearchParams();
  params.set("path", path);
  if (base) params.set("base", base);
  const response = await apiFetch(`/api/settings/path-suggestions?${params.toString()}`, init);
  return (await response.json()) as PathSuggestionsResponse;
}
