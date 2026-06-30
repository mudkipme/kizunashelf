import { logActivity, type LogActivityRequest } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/** Logs an activity for an entity (writes a daily-note line). Pass `dryRun` to
 * preview the result — the resolved line, target note, and what it would stamp —
 * without touching disk. */
export function postLogActivity(id: string, request: LogActivityRequest, dryRun = false) {
  return logActivity(id, request, dryRun ? { dryRun: true } : undefined, undefined, apiFetch);
}
