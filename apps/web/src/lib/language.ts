import { create } from "zustand";
import { persist } from "zustand/middleware";

import { isIso639TitleLanguage } from "@/lib/title-language";

export const languageStorageKey = "kizunashelf.language.v1";

/** The browser's preferred language as a bare ISO 639 code (e.g. "en-US" -> "en"). */
function browserLanguage(): string {
  const raw =
    (typeof navigator !== "undefined" && (navigator.language || navigator.languages?.[0])) || "en";
  const base = raw.split("-")[0]?.toLowerCase();
  return isIso639TitleLanguage(base) ? (base as string) : "en";
}

type LanguageState = {
  /** The viewer's display language; drives title resolution (and, later, i18n). */
  language: string;
  setLanguage: (language: string) => void;
};

export const useLanguageStore = create<LanguageState>()(
  persist(
    (set) => ({
      language: browserLanguage(),
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

/** The current display language code. Use with `entityTitle(entity, language)`. */
export function useTitleLanguage(): string {
  return useLanguageStore((state) => state.language);
}
