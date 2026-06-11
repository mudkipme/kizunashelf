/**
 * Resolve a frontmatter image value to a URL the browser can load.
 *
 * Remote URLs (`http(s):`, `data:`, `blob:`) pass through unchanged. A
 * vault-relative local path (written after an asset download) is served through
 * the API asset route. Desktop (Tauri) asset resolution from disk lands in a
 * later phase; until then a local path falls back to the same route, which only
 * resolves in the web runtime.
 */
export function resolveAssetSrc(value: string | null | undefined): string | undefined {
  if (!value) return undefined;
  const trimmed = value.trim();
  if (!trimmed) return undefined;
  if (/^(?:https?:|data:|blob:)/i.test(trimmed)) return trimmed;
  const encoded = trimmed
    .split("/")
    .filter(Boolean)
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return `/api/assets/${encoded}`;
}

/** True when `value` is a downloaded local asset rather than a remote URL. */
export function isLocalAsset(value: string | null | undefined): boolean {
  if (!value) return false;
  const trimmed = value.trim();
  return trimmed.length > 0 && !/^(?:https?:|data:|blob:)/i.test(trimmed);
}

/** True when `value` is a remote URL eligible for download. */
export function isRemoteAsset(value: string | null | undefined): boolean {
  if (!value) return false;
  return /^https?:/i.test(value.trim());
}
