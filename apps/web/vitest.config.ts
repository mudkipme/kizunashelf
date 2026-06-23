import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

// Vitest is configured standalone (not via the app's vite.config) so unit tests
// don't pull in the React/Tailwind plugins. We only re-declare the `@` alias the
// source uses and the `__APP_VERSION__` define, so importing app modules resolves.
export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify("test"),
  },
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
