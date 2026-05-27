type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

type DesktopApiResponse = {
  status: number;
  body: string;
  contentType?: string | null;
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const apiFetch: typeof globalThis.fetch = async (input, init) => {
  const response = isTauriRuntime()
    ? await fetchTauriResponse(input, init)
    : await fetch(input, init);
  if (!response.ok) throw await ApiHttpError.fromResponse(response);
  return response;
};

export function errorMessage(error: unknown) {
  if (error instanceof ApiHttpError) return error.message;
  if (typeof error === "string") return error;
  return error instanceof Error ? error.message : "Unknown error";
}

export function isAbortError(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}

function isTauriRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
}

async function fetchTauriResponse(input: RequestInfo | URL, init?: RequestInit) {
  const method = init?.method ?? "GET";
  if (method.toUpperCase() !== "GET") {
    throw new Error(`Unsupported desktop API method: ${method}`);
  }
  if (init?.signal?.aborted) {
    throw new DOMException("The operation was aborted", "AbortError");
  }
  const invoke = await getTauriInvoke();
  const response = await invoke<DesktopApiResponse>("api_request", {
    method,
    url: requestUrl(input),
  });
  return new Response(response.body, {
    status: response.status,
    headers: response.contentType ? { "content-type": response.contentType } : undefined,
  });
}

async function getTauriInvoke(): Promise<TauriInvoke> {
  const module = await import("@tauri-apps/api/core");
  return module.invoke as TauriInvoke;
}

function requestUrl(input: RequestInfo | URL) {
  if (typeof input === "string") return input;
  if (input instanceof URL) return input.toString();
  return input.url;
}

class ApiHttpError extends Error {
  private constructor(
    readonly status: number,
    readonly info: unknown,
    message: string,
  ) {
    super(message);
    this.name = "ApiHttpError";
  }

  static async fromResponse(response: Response) {
    const info = await readErrorInfo(response);
    const serverMessage =
      info && typeof info === "object" && "error" in info && typeof info.error === "string"
        ? info.error
        : response.statusText || "Request failed";
    return new ApiHttpError(response.status, info, `${response.status} ${serverMessage}`);
  }
}

async function readErrorInfo(response: Response) {
  const text = await response.text();
  if (!text) return undefined;
  if (!(response.headers.get("content-type") ?? "").toLowerCase().includes("json")) return text;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}
