import { create } from "zustand";
import { persist } from "zustand/middleware";

import {
  allOptions,
  defaultDirection,
  defaultSort,
  defaultTitleOptionId,
  defaultView,
} from "@/lib/constants";

export type AssetListPreferences = {
  refs: string;
  cover: string;
  sort: string;
  direction: string;
  view: string;
  titleLanguage: string;
};

type AssetListPreferencesState = {
  byType: Record<string, AssetListPreferences>;
  getPreferences: (type: string) => AssetListPreferences;
  setPreferences: (type: string, preferences: AssetListPreferences) => void;
};

const defaults: AssetListPreferences = {
  refs: allOptions,
  cover: allOptions,
  sort: defaultSort,
  direction: defaultDirection,
  view: defaultView,
  titleLanguage: defaultTitleOptionId,
};

export const preferenceKeys = [
  "refs",
  "cover",
  "sort",
  "direction",
  "view",
  "titleLanguage",
] as const;

export const useAssetListPreferencesStore = create<AssetListPreferencesState>()(
  persist(
    (set, get) => ({
      byType: {},
      getPreferences(type) {
        return normalizePreferences(get().byType[type]);
      },
      setPreferences(type, preferences) {
        set((state) => ({
          byType: {
            ...state.byType,
            [type]: normalizePreferences(preferences),
          },
        }));
      },
    }),
    {
      name: "kizunashelf.assetListPreferences.v2",
      partialize: (state) => ({ byType: state.byType }),
    },
  ),
);

export function readAssetListPreferences(type: string): AssetListPreferences {
  return useAssetListPreferencesStore.getState().getPreferences(type);
}

export function writeAssetListPreferences(type: string, preferences: AssetListPreferences) {
  useAssetListPreferencesStore.getState().setPreferences(type, preferences);
}

export function applyPreferencesToSearchParams(
  params: URLSearchParams,
  preferences: AssetListPreferences,
) {
  for (const key of preferenceKeys) {
    const value = preferences[key];
    if (value === defaults[key]) params.delete(key);
    else params.set(key, value);
  }
}

export function preferencesFromSearchParams(params: URLSearchParams): AssetListPreferences {
  return normalizePreferences({
    refs: params.get("refs") ?? undefined,
    cover: params.get("cover") ?? undefined,
    sort: params.get("sort") ?? undefined,
    direction: params.get("direction") ?? undefined,
    view: params.get("view") ?? undefined,
    titleLanguage: params.get("titleLanguage") ?? undefined,
  });
}

function normalizePreferences(preferences: Partial<AssetListPreferences> | undefined): AssetListPreferences {
  const sort = preferences?.sort ?? defaults.sort;

  return {
    refs: preferences?.refs === "with" || preferences?.refs === "without" ? preferences.refs : defaults.refs,
    cover:
      preferences?.cover === "with" || preferences?.cover === "without"
        ? preferences.cover
        : defaults.cover,
    sort: sort.startsWith("date:") || sort === "relations" || sort === "path" ? sort : defaults.sort,
    direction: preferences?.direction === "desc" ? "desc" : defaults.direction,
    view: preferences?.view === "grid" ? "grid" : defaults.view,
    titleLanguage: preferences?.titleLanguage || defaults.titleLanguage,
  };
}
