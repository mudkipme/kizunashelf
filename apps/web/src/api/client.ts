import type { z } from "zod";

type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

const desktopRequestTimeoutMs = 120_000;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export async function fetchJson<TSchema extends z.ZodType>(
  url: string,
  schema: TSchema,
  init?: RequestInit,
): Promise<z.infer<TSchema>> {
  if (isTauriRuntime()) {
    const data = await fetchTauriJson(url, init);
    return schema.parse(data);
  }

  const response = await fetch(url, init);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return schema.parse(await response.json());
}

export function errorMessage(error: unknown) {
  if (typeof error === "string") return error;
  return error instanceof Error ? error.message : "Unknown error";
}

export function isAbortError(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}

function isTauriRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
}

async function fetchTauriJson(url: string, init?: RequestInit) {
  const method = init?.method ?? "GET";
  if (method.toUpperCase() !== "GET") {
    throw new Error(`Unsupported desktop API method: ${method}`);
  }
  if (init?.signal?.aborted) {
    throw new DOMException("The operation was aborted", "AbortError");
  }
  const invoke = await getTauriInvoke();
  return withTimeout(
    invoke<unknown>("api_request", { method, url }),
    desktopRequestTimeoutMs,
    `Desktop API request timed out while loading ${url}`,
  );
}

async function getTauriInvoke(): Promise<TauriInvoke> {
  const module = await import("@tauri-apps/api/core");
  return module.invoke as TauriInvoke;
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(message)), timeoutMs);
  });

  return Promise.race([promise, timeoutPromise]).finally(() => {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  });
}
