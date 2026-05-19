import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ThemeMode = "light" | "dark" | "system";

type ThemeState = {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
};

export const themeStorageKey = "kizunashelf.theme.v1";

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      mode: "system",
      setMode(mode) {
        set({ mode });
        applyThemeMode(mode);
      },
    }),
    {
      name: themeStorageKey,
      partialize: (state) => ({ mode: state.mode }),
      onRehydrateStorage() {
        return (state) => applyThemeMode(normalizeThemeMode(state?.mode));
      },
    },
  ),
);

export function initializeTheme() {
  applyThemeMode(readStoredThemeMode());
}

export function applyThemeMode(mode: ThemeMode) {
  const resolved = resolveThemeMode(mode);
  document.documentElement.classList.toggle("dark", resolved === "dark");
  document.documentElement.dataset.theme = mode;
}

function readStoredThemeMode(): ThemeMode {
  try {
    const raw = window.localStorage.getItem(themeStorageKey);
    if (!raw) return "system";
    return normalizeThemeMode(JSON.parse(raw)?.state?.mode);
  } catch {
    return "system";
  }
}

function normalizeThemeMode(value: unknown): ThemeMode {
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

function resolveThemeMode(mode: ThemeMode) {
  if (mode !== "system") return mode;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}
