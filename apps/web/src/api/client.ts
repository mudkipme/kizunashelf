export async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return (await response.json()) as T;
}

export function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unknown error";
}
