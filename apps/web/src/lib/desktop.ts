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
