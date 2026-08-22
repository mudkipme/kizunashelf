import { getVaultChanges } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/** Waits for the native VFS change generation to advance. The server bounds
 * each request to 25 seconds, so this works through both HTTP and Tauri invoke. */
export function waitForVaultChange(after: number, init?: RequestInit) {
  return getVaultChanges({ after }, init, apiFetch);
}
