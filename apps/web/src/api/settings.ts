import {
  getLanguages as requestLanguages,
  getPathSuggestions as requestPathSuggestions,
  getRawSettingsConfig as requestRawSettingsConfig,
  getSettingsConfig as requestSettingsConfig,
  getTypePresets as requestTypePresets,
  resolveTypePresets as requestResolveTypePresets,
  saveRawSettingsConfig as requestSaveRawSettingsConfig,
  saveSettingsConfig as requestSaveSettingsConfig,
  type ResolveTypePresetsRequest,
  type SaveRawConfigRequest,
  type SaveSettingsRequest,
} from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

export function getSettingsConfig(init?: RequestInit) {
  return requestSettingsConfig(init, apiFetch);
}

export function getTypePresets(init?: RequestInit) {
  return requestTypePresets(init, apiFetch);
}

// Materialize picked presets into concrete types, merged against the editor's
// current types. Stateless server-side — the same call backs onboarding (empty
// currentTypes) and the settings "add built-in type" flow.
export function resolveTypePresets(request: ResolveTypePresetsRequest) {
  return requestResolveTypePresets(request, undefined, apiFetch);
}

export function getLanguages(init?: RequestInit) {
  return requestLanguages(init, apiFetch);
}

export function saveSettingsConfig(request: SaveSettingsRequest) {
  return requestSaveSettingsConfig(request, undefined, apiFetch);
}

// The raw-text "advanced" editor: read/write `KizunaShelf/config.yaml` verbatim.
// Saving strictly validates the YAML server-side (unknown fields are rejected).
export function getRawSettingsConfig(init?: RequestInit) {
  return requestRawSettingsConfig(init, apiFetch);
}

export function saveRawSettingsConfig(request: SaveRawConfigRequest) {
  return requestSaveRawSettingsConfig(request, undefined, apiFetch);
}

// `base` is a vault-relative directory the suggestions are rooted at (e.g. the
// taxonomy root for a type's folder path); omitted means the vault root.
export function getPathSuggestions(path: string, base?: string, init?: RequestInit) {
  return requestPathSuggestions({ path, base }, init, apiFetch);
}
