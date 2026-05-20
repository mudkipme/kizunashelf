import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { cors } from "hono/cors";
import { fileURLToPath } from "node:url";

import { createLibraryLoader } from "./library-cache";
import { registerRoutes } from "./routes";
import { staticResponse } from "./static-web";

const app = new Hono();
const defaultConfigPath = fileURLToPath(
  new URL("../../../config/kizunashelf.config.json", import.meta.url),
);
const configPath = process.env.KIZUNASHELF_CONFIG ?? defaultConfigPath;
const port = Number(process.env.PORT ?? 8787);
const hostname = process.env.HOST ?? "0.0.0.0";
const cacheTtlMs = Number(process.env.KIZUNASHELF_CACHE_TTL_MS ?? 10_000);
const webDistPath = fileURLToPath(new URL("../../web/dist", import.meta.url));
const serveStaticWeb = process.env.KIZUNASHELF_SERVE_WEB !== "false";
const getLibrary = createLibraryLoader(configPath, cacheTtlMs);

app.use(
  "*",
  cors({
    origin: ["http://localhost:5173", "http://127.0.0.1:5173"],
  }),
);

registerRoutes(app, getLibrary);

if (serveStaticWeb) {
  app.get("*", async (c, next) => {
    if (c.req.path.startsWith("/api/")) return next();

    const response = await staticResponse(webDistPath, c.req.path);
    if (response) return response;

    return next();
  });
}

serve(
  {
    fetch: app.fetch,
    hostname,
    port,
  },
  (info) => {
    console.log(`KizunaShelf listening on http://${info.address}:${info.port}`);
  },
);
