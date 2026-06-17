declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isDesktopRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
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

/** External-provider credentials, stored in the OS keychain on desktop. */
export type Credentials = {
  igdbClientId: string;
  igdbClientSecret: string;
  tvdbApiKey: string;
  tvdbPin: string;
};

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
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
