import { create } from "zustand";
import { persist } from "zustand/middleware";

import { UI_LOCALES, type UiLocale } from "@/lib/i18n";
import { isIso639TitleLanguage, primaryLanguage } from "@/lib/title-language";

/**
 * The viewer's single language preference. Unlike a title language it may
 * carry a script subtag (`zh-Hans`/`zh-Hant`) — the one place that
 * distinction lives on the client. Everything language-sensitive derives from
 * it: the UI locale (`useUiLocale`, English fallback), the title/content
 * language (`useTitleLanguage`, the bare primary subtag — `titles` maps and
 * schema config never see a script subtag), and the language sent to
 * provider-bound requests (`useLanguagePreference`, the raw value, so
 * providers that distinguish Simplified/Traditional can). See
 * `docs/i18n-plan.md`.
 */
export const languageStorageKey = "kizunashelf.language.v2";
const legacyStorageKey = "kizunashelf.language.v1";

/** Whether a browser/system tag means Traditional Chinese (script or region). */
function isTraditionalChineseTag(tag: string): boolean {
  const lower = tag.toLowerCase();
  return lower.includes("hant") || /-(tw|hk|mo)(-|$)/.test(lower);
}

/**
 * The Simplified/Traditional preference a bare `zh` maps to, sniffed from the
 * browser languages. Used for the first-run default and the v1 migration —
 * a stored `zh` is ambiguous between scripts.
 */
function preferredChineseScript(): UiLocale {
  const tags = (typeof navigator !== "undefined" && navigator.languages) || [];
  for (const tag of tags) {
    if (primaryLanguage(tag) === "zh") {
      return isTraditionalChineseTag(tag) ? "zh-Hant" : "zh-Hans";
    }
  }
  return "zh-Hans";
}

/** The browser's preferred language as a preference code (`zh-TW` → `zh-Hant`, `en-US` → `en`). */
function browserPreference(): string {
  const tags = (typeof navigator !== "undefined" &&
    (navigator.languages?.length ? navigator.languages : [navigator.language])) || ["en"];
  for (const tag of tags) {
    if (!tag) {
      continue;
    }
    const primary = primaryLanguage(tag);
    if (!isIso639TitleLanguage(primary)) {
      continue;
    }
    return primary === "zh" ? (isTraditionalChineseTag(tag) ? "zh-Hant" : "zh-Hans") : primary;
  }
  return "en";
}

/**
 * The initial preference: the persisted v2 value wins (via rehydration); on
 * first run under v2, migrate the v1 value — a bare `zh` becomes the sniffed
 * script — else derive from the browser language.
 */
function initialLanguage(): string {
  try {
    if (localStorage.getItem(languageStorageKey)) {
      // A v2 value exists; persist rehydration replaces this placeholder.
      return "en";
    }
    const legacy = localStorage.getItem(legacyStorageKey);
    if (legacy) {
      localStorage.removeItem(legacyStorageKey);
      const parsed: unknown = JSON.parse(legacy);
      const value = (parsed as { state?: { language?: unknown } })?.state?.language;
      if (typeof value === "string" && value.trim()) {
        return value === "zh" ? preferredChineseScript() : value;
      }
    }
  } catch {
    // Storage unavailable/corrupt: fall through to the browser default.
  }
  return browserPreference();
}

type LanguageState = {
  /** The viewer's language preference (may carry a script subtag, e.g. `zh-Hans`). */
  language: string;
  setLanguage: (language: string) => void;
};

export const useLanguageStore = create<LanguageState>()(
  persist(
    (set) => ({
      language: initialLanguage(),
      setLanguage(language) {
        set({ language });
      },
    }),
    {
      name: languageStorageKey,
      partialize: (state) => ({ language: state.language }),
    },
  ),
);

/**
 * The raw language preference — what provider-bound requests send (external
 * search, quick-add, episode fetch), so providers that distinguish
 * `zh-Hans`/`zh-Hant` can honor it. For title lookup use `useTitleLanguage`.
 */
export function useLanguagePreference(): string {
  return useLanguageStore((state) => state.language);
}

/** The UI locale a preference maps to: one of the shipped translations, else English. */
export function uiLocaleFor(preference: string): UiLocale {
  const wanted = preference.trim().toLowerCase();
  const exact = UI_LOCALES.find((locale) => locale.toLowerCase() === wanted);
  if (exact) {
    return exact;
  }
  // A bare/legacy `zh` still deserves a Chinese UI.
  if (primaryLanguage(preference) === "zh") {
    return preferredChineseScript();
  }
  return "en";
}

/** The current UI locale. Drives the message catalog, `<html lang>`, and formatting. */
export function useUiLocale(): UiLocale {
  return useLanguageStore((state) => uiLocaleFor(state.language));
}

/**
 * The current title/content language: the preference's bare primary subtag.
 * Use with `entityTitle(entity, language)` and anywhere a schema title
 * language is matched — `titles` maps never carry script subtags.
 */
export function useTitleLanguage(): string {
  return useLanguageStore((state) => primaryLanguage(state.language) || "en");
}
