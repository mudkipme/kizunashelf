import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

// Vitest is configured standalone (not via the app's vite.config) so unit tests
// don't pull in the React/Tailwind plugins. We only re-declare the `@` alias the
// source uses, so importing app modules resolves.
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
