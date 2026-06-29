import { isDesktopRuntime } from "@/lib/desktop";

/**
 * Custom URI scheme the desktop app registers to serve local vault assets.
 * Must match `ASSET_SCHEME` in `apps/desktop/src-tauri/src/lib.rs`.
 */
const DESKTOP_ASSET_SCHEME = "kizasset";

/**
 * Build the desktop asset URL for the custom `kizasset` scheme. The host form
 * differs by platform: Windows/Android use `http://<scheme>.localhost/...`,
 * everything else uses `<scheme>://localhost/...`.
 */
function desktopAssetUrl(encodedPath: string): string {
  const isWindowsOrAndroid =
    typeof navigator !== "undefined" && /windows|android/i.test(navigator.userAgent);
  return isWindowsOrAndroid
    ? `http://${DESKTOP_ASSET_SCHEME}.localhost/${encodedPath}`
    : `${DESKTOP_ASSET_SCHEME}://localhost/${encodedPath}`;
}

/**
 * Resolve a frontmatter image value to a URL the runtime can load.
 *
 * Remote URLs (`http(s):`, `data:`, `blob:`) pass through unchanged. A
 * vault-relative local path (written after an asset download) is served through
 * the API asset route in the web runtime, or the `kizasset` custom scheme in the
 * desktop (Tauri) runtime, which has no HTTP listener.
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
  return isDesktopRuntime() ? desktopAssetUrl(encoded) : `/api/assets/${encoded}`;
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
