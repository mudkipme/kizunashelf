import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import type { KizunaConfig } from "./types";

export async function loadConfig(configPath?: string): Promise<KizunaConfig> {
  const path = resolve(configPath ?? "config/kizunashelf.config.json");
  const raw = await readFile(path, "utf8");
  const parsed = JSON.parse(raw) as KizunaConfig;

  if (!parsed.vaultRoot || !parsed.taxonomyRoot || !Array.isArray(parsed.types)) {
    throw new Error(`Invalid KizunaShelf config at ${path}`);
  }

  return parsed;
}
