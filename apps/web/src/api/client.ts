import type { z } from "zod";

export async function fetchJson<TSchema extends z.ZodType>(
  url: string,
  schema: TSchema,
  init?: RequestInit,
): Promise<z.infer<TSchema>> {
  const response = await fetch(url, init);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return schema.parse(await response.json());
}

export function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unknown error";
}

export function isAbortError(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}
