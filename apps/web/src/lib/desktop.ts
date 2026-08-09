declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isDesktopRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
}

/** True in the desktop shell on macOS, where the window uses the overlay title
 * bar (`titleBarStyle: Overlay`) and the header must clear the native traffic
 * lights. WKWebView reports `navigator.platform` as `MacIntel` on all Macs. */
export function isMacDesktopRuntime() {
  return isDesktopRuntime() && navigator.platform.startsWith("Mac");
}

/** Mirror the current page into the native window title (taskbar, window
 * switcher, Mission Control). No-op outside the desktop shell; failures are
 * cosmetic and intentionally swallowed. */
export async function setWindowTitle(title: string) {
  if (!isDesktopRuntime()) return;
  try {
    const module = await import("@tauri-apps/api/window");
    await module.getCurrentWindow().setTitle(title);
  } catch {
    // A title update is never worth surfacing an error for.
  }
}

export async function selectDirectory(initial?: string) {
  if (!isDesktopRuntime()) return undefined;
  const module = await import("@tauri-apps/plugin-dialog");
  const selected = await module.open({
    directory: true,
    multiple: false,
    defaultPath: initial || undefined,
  });
  return typeof selected === "string" ? selected : undefined;
}

// MARK: Desktop multi-vault + credentials commands (see apps/desktop/src-tauri).

/** A remembered desktop vault. */
export type VaultInfo = { name: string; path: string; active: boolean };

/**
 * External-provider credentials, stored in the OS keychain on desktop. Keyed by
 * the secret-store key each provider declares in its catalog `credentials` (e.g.
 * `igdb_client_id`) — the key set lives in the Rust core, never hard-coded here.
 */
export type Credentials = Record<string, string>;

/** Lazily import and call a Tauri command. The desktop runtime check is the
 * caller's responsibility (see {@link isDesktopRuntime}). */
export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const module = await import("@tauri-apps/api/core");
  return (module.invoke as <R>(command: string, args?: Record<string, unknown>) => Promise<R>)<T>(
    command,
    args,
  );
}

export const listVaults = () => invoke<VaultInfo[]>("list_vaults");
export const addVault = (path: string) => invoke<VaultInfo[]>("add_vault", { path });
export const createVault = (parent: string, name: string) =>
  invoke<VaultInfo[]>("create_vault", { parent, name });
export const switchVault = (path: string) => invoke<VaultInfo[]>("switch_vault", { path });
export const removeVault = (path: string) => invoke<VaultInfo[]>("remove_vault", { path });
export const getCredentials = () => invoke<Credentials>("get_credentials");
export const setCredentials = (credentials: Credentials) =>
  invoke<void>("set_credentials", { credentials });
