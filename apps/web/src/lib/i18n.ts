import { i18n, type Messages } from "@lingui/core";

import { messages as enMessages } from "@/locales/en/messages.po";

/**
 * The UI locales this build ships translations for. This is a client build fact,
 * not a contract flag: the core lists every *preference* option, and any
 * preference without a catalog here falls back to the English UI (titles still
 * follow the preference).
 */
export const UI_LOCALES = ["en", "ja", "zh-Hans", "zh-Hant"] as const;
export type UiLocale = (typeof UI_LOCALES)[number];

// The source locale is bundled so the very first render is already translated;
// the other catalogs load on demand when the viewer switches.
i18n.load("en", enMessages);
i18n.activate("en");

const catalogLoaders: Record<Exclude<UiLocale, "en">, () => Promise<{ messages: Messages }>> = {
  ja: () => import("../locales/ja/messages.po"),
  "zh-Hans": () => import("../locales/zh-Hans/messages.po"),
  "zh-Hant": () => import("../locales/zh-Hant/messages.po"),
};

// Monotonic request token: two fast language switches can resolve their
// dynamic imports out of order, and the last *selected* locale must win.
let activation = 0;

export async function activateUiLocale(locale: UiLocale) {
  if (i18n.locale === locale) {
    return;
  }
  const request = ++activation;
  try {
    if (locale !== "en") {
      const { messages } = await catalogLoaders[locale]();
      if (request !== activation) return;
      i18n.load(locale, messages);
    }
    if (request === activation) {
      i18n.activate(locale);
    }
  } catch (error) {
    // Keep the previous catalog rather than leaving the UI half-switched; the
    // next switch (or reload) retries the chunk.
    if (request === activation) {
      console.error(`Failed to load the ${locale} UI catalog`, error);
    }
  }
}

export { i18n };
