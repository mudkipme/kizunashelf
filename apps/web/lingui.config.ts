import { defineConfig } from "@lingui/cli";

/**
 * UI-string catalogs (`UI_LOCALES` in `src/lib/i18n.ts`). The UI ships exactly
 * these four locales; every other language preference falls back to English.
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
      // Vitest stores PNGs below directories ending in `.test.tsx`; without
      // this exclusion Lingui mistakes those directories for source files.
      exclude: ["**/__screenshots__/**"],
    },
  ],
});
