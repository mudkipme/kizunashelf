import { invoke, isDesktopRuntime } from "@/lib/desktop";

type DesktopApiResponse = {
  status: number;
  body: string;
  contentType?: string | null;
};

export const apiFetch: typeof globalThis.fetch = async (input, init) => {
  const response = isDesktopRuntime()
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

/** HTTP status of a failed API request, or `undefined` for non-HTTP errors. */
export function errorStatus(error: unknown): number | undefined {
  return error instanceof ApiHttpError ? error.status : undefined;
}

/** Whether an error is a 409 optimistic-concurrency conflict (stale revision). */
export function isConflictError(error: unknown) {
  return errorStatus(error) === 409;
}

async function fetchTauriResponse(input: RequestInfo | URL, init?: RequestInit) {
  const method = init?.method ?? "GET";
  const signal = init?.signal ?? undefined;
  throwIfAborted(signal);
  // `invoke` can't be cancelled mid-flight, but React Query (and StrictMode in
  // dev) abort the signal to cancel superseded fetches. Native `fetch` rejects
  // immediately on abort; we mirror that by racing the invoke against the
  // signal, otherwise the cancelled request's result is silently discarded and
  // the query never settles — leaving the app stuck on "Loading".
  const request = invoke<DesktopApiResponse>("api_request", {
    method,
    url: requestUrl(input),
    body: await requestBody(init?.body),
  });
  const response = await raceAbort(request, signal);
  return new Response(response.body, {
    status: response.status,
    headers: response.contentType ? { "content-type": response.contentType } : undefined,
  });
}

function throwIfAborted(signal?: AbortSignal) {
  if (signal?.aborted) throw abortError(signal);
}

function raceAbort<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(abortError(signal));
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(abortError(signal));
    signal.addEventListener("abort", onAbort, { once: true });
    const cleanup = () => signal.removeEventListener("abort", onAbort);
    promise.then(
      (value) => {
        cleanup();
        resolve(value);
      },
      (error) => {
        cleanup();
        reject(error);
      },
    );
  });
}

function abortError(signal: AbortSignal) {
  const reason = (signal as AbortSignal & { reason?: unknown }).reason;
  return reason instanceof Error
    ? reason
    : new DOMException("The operation was aborted", "AbortError");
}

async function requestBody(body: BodyInit | null | undefined) {
  if (body == null) return undefined;
  if (typeof body === "string") return body;
  if (body instanceof URLSearchParams) return body.toString();
  if (body instanceof Blob) return body.text();
  if (body instanceof ArrayBuffer) return new TextDecoder().decode(body);
  throw new Error("Unsupported desktop API request body");
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
