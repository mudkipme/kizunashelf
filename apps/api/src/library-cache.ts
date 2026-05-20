import { loadConfig, readLibrary, type Library } from "@kizunashelf/core";

export type GetLibrary = () => Promise<Library>;

export function createLibraryLoader(configPath: string, cacheTtlMs: number): GetLibrary {
  let cachedLibrary: Library | undefined;
  let cachedAt = 0;

  return async function getLibrary() {
    if (cachedLibrary && Date.now() - cachedAt < cacheTtlMs) {
      return cachedLibrary;
    }

    const config = await loadConfig(configPath);
    cachedLibrary = await readLibrary(config);
    cachedAt = Date.now();
    return cachedLibrary;
  };
}
