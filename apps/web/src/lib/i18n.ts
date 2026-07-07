import { i18n, type Messages } from "@lingui/core";

import { messages as enMessages } from "@/locales/en/messages.po";

/**
 * The UI locales the app ships translations for — mirrors the core's
 * `uiSupported` user languages. Any other language preference falls back to
 * the English UI (titles still follow the preference).
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

export async function activateUiLocale(locale: UiLocale) {
  if (i18n.locale === locale) {
    return;
  }
  if (locale !== "en") {
    const { messages } = await catalogLoaders[locale]();
    i18n.load(locale, messages);
  }
  i18n.activate(locale);
}

export { i18n };
