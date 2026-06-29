import { useQuery } from "@tanstack/react-query";

import { capabilitiesQuery } from "@/api/queries";

/**
 * Server-declared write capabilities (the read-only-mode gates), with safe
 * defaults while the query loads: writes are assumed **allowed** (optimistic, so
 * controls don't flash disabled on every navigation) and asset downloads assumed
 * **off** (they require explicit opt-in). Collapsing the `!== false` / `=== true`
 * conventions here keeps the polarity consistent across pages — a misread of
 * which default applied was an easy footgun.
 */
export function useCapabilities() {
  const query = useQuery(capabilitiesQuery());
  const data = query.data;
  return {
    contentWritable: data?.contentWritable !== false,
    assetDownloadEnabled: data?.assetDownloadEnabled === true,
    settingsWritable: data?.settingsWritable !== false,
    isPending: query.isPending,
    error: query.error,
  };
}

/** Shared copy for the read-only (content writes disabled) state. */
export const CONTENT_WRITES_DISABLED = "Content writes are disabled.";
