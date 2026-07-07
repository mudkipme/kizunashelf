import { defineConfig } from "@lingui/cli";

/**
 * UI-string catalogs. The locale set mirrors the core's `uiSupported` user
 * languages (`crates/kizunashelf/src/languages.rs`) — the UI ships exactly
 * these four; every other language preference falls back to the English UI.
 * `pnpm i18n:extract` regenerates the catalogs; CI diffs them like the
 * generated API contract.
 */
export default defineConfig({
  sourceLocale: "en",
  locales: ["en", "ja", "zh-Hans", "zh-Hant"],
  catalogs: [
    {
      path: "<rootDir>/src/locales/{locale}/messages",
      include: ["src"],
    },
  ],
});
