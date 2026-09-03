import { fileURLToPath, URL } from "node:url";

import { lingui, linguiTransformerBabelPreset } from "@lingui/vite-plugin";
import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

const alias = { "@": fileURLToPath(new URL("./src", import.meta.url)) };

// Two suites, split by file extension so each gets only the machinery it needs.
//
// `unit` (*.test.ts) covers the pure helpers — rule models, title-language
// derivation, pagination — in plain Node with no plugins at all, which keeps it
// near-instant.
//
// `ui` (*.test.tsx) renders components in a real Chromium via Vitest's browser
// mode. This app leans on base-ui/radix comboboxes, popovers and dnd-kit, whose
// behavior is positioning, focus management and pointer events — exactly what a
// DOM emulator approximates rather than reproduces. Running them for real means
// the tests exercise the same code path a user does, and needs no shims for
// ResizeObserver, scrollIntoView or pointer capture.
export default defineConfig({
  test: {
    projects: [
      {
        resolve: { alias },
        test: {
          name: "unit",
          environment: "node",
          include: ["src/**/*.test.ts"],
        },
      },
      {
        // The same transform chain as the app build: the Lingui macros used by
        // every component are compiled by their own Babel pass, since the
        // oxc-based React plugin has no Babel hook.
        plugins: [
          react(),
          lingui(),
          babel({ presets: [linguiTransformerBabelPreset()] }),
          tailwindcss(),
        ],
        resolve: { alias },
        // Pre-bundled up front. Vite otherwise discovers these mid-run the
        // first time a test imports them and reloads the page to swap them in,
        // which aborts whatever test file was mid-import — a cold-cache flake
        // that only ever shows up in CI. These are the ones no app entry pulls
        // in early enough for the initial scan to find.
        optimizeDeps: {
          include: ["react-dom/client", "@dnd-kit/core", "@dnd-kit/sortable", "@dnd-kit/utilities"],
        },
        test: {
          name: "ui",
          include: ["src/**/*.test.tsx"],
          setupFiles: ["./src/test/setup.ts"],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
