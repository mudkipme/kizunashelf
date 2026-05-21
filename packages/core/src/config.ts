import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { KizunaConfigSchema, type KizunaConfig } from "./types";

export async function loadConfig(configPath?: string): Promise<KizunaConfig> {
  const path = resolve(configPath ?? "config/kizunashelf.config.json");
  const raw = await readFile(path, "utf8");
  const parsed = JSON.parse(raw);

  const result = KizunaConfigSchema.safeParse(parsed);
  if (!result.success) {
    const errorDetails = result.error.issues
      .map((issue) => `  - [${issue.path.join(".") || "root"}]: ${issue.message} (${issue.code})`)
      .join("\n");
    throw new Error(`Invalid KizunaShelf config at ${path}:\n${errorDetails}`);
  }

  return result.data;
}
