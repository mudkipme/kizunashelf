import { lingui, linguiTransformerBabelPreset } from "@lingui/vite-plugin";
import babel from "@rolldown/plugin-babel";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? "0.3.0"),
  },
  plugins: [
    react(),
    lingui(),
    // Compiles the Lingui macros (`@lingui/react/macro`) — the oxc-based react
    // plugin has no babel hook, so the transform runs as its own pass.
    babel({ presets: [linguiTransformerBabelPreset()] }),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:8787",
    },
    allowedHosts: ["porygon-z.lan"]
  },
});
