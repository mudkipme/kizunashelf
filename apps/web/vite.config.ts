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
  build: {
    rolldownOptions: {
      output: {
        // Split the stable framework/vendor code out of the entry chunk so it
        // caches across app deploys. Route pages are already code-split via
        // React.lazy in App.tsx, and page-only heavy libs (react-markdown,
        // lightbox, day-picker, dnd-kit) ride along in those lazy chunks — so
        // we deliberately group ONLY the broadly-shared shell deps here. A
        // catch-all node_modules group would pull the page-only libs back into
        // an eager chunk and undo the route splitting.
        advancedChunks: {
          groups: [
            {
              name: "react-vendor",
              test: /[\\/]node_modules[\\/](react|react-dom|react-router|react-router-dom|scheduler)[\\/]/,
            },
            {
              name: "vendor",
              test: /[\\/]node_modules[\\/](@tanstack|zustand|@lingui|@base-ui|radix-ui|@radix-ui|lucide-react|sonner|clsx|tailwind-merge|class-variance-authority|zod)[\\/]/,
            },
          ],
        },
      },
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
