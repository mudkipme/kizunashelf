import { useLingui } from "@lingui/react/macro";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { toast } from "sonner";

import { isAbortError } from "@/api/client";
import { refreshLibrary } from "@/api/settings";
import { waitForVaultChange } from "@/api/vault-changes";

const INITIAL_RETRY_MS = 1_000;
const MAX_RETRY_MS = 30_000;

/** Bridges the core's native VFS watcher into TanStack Query. It renders no UI:
 * browsing views refetch immediately, while editor pages independently protect
 * their local drafts when those fresh query results arrive. */
export function VaultChangeSync() {
  const queryClient = useQueryClient();
  const { t } = useLingui();

  useEffect(() => {
    const controller = new AbortController();
    let generation = 0;
    let refreshing = false;
    async function refresh() {
      if (controller.signal.aborted || refreshing || document.visibilityState === "hidden") return;
      refreshing = true;
      try {
        await refreshLibrary({ signal: controller.signal });
        if (controller.signal.aborted) return;
        toast.dismiss("vault-refresh");
        await queryClient.invalidateQueries();
      } catch (error) {
        if (!controller.signal.aborted && !isAbortError(error))
          toast.error(t`Couldn't refresh. Showing the last loaded version.`, {
            id: "vault-refresh",
            action: { label: t`Retry`, onClick: () => void refresh() },
          });
      } finally {
        refreshing = false;
      }
    }
    const onResume = () => {
      void refresh();
    };
    window.addEventListener("focus", onResume);
    document.addEventListener("visibilitychange", onResume);

    async function run() {
      let retryMs = INITIAL_RETRY_MS;
      while (!controller.signal.aborted) {
        try {
          const response = await waitForVaultChange(generation, { signal: controller.signal });
          if (controller.signal.aborted || !response.supported) return;
          generation = response.generation;
          retryMs = INITIAL_RETRY_MS;
          if (response.changed) await queryClient.invalidateQueries();
        } catch (error) {
          if (controller.signal.aborted || isAbortError(error)) return;
          await abortableDelay(retryMs, controller.signal);
          retryMs = Math.min(retryMs * 2, MAX_RETRY_MS);
        }
      }
    }

    void run();
    return () => {
      controller.abort();
      window.removeEventListener("focus", onResume);
      document.removeEventListener("visibilitychange", onResume);
      toast.dismiss("vault-refresh");
    };
  }, [queryClient, t]);

  return null;
}

function abortableDelay(milliseconds: number, signal: AbortSignal) {
  if (signal.aborted) return Promise.resolve();
  return new Promise<void>((resolve) => {
    const timeout = window.setTimeout(done, milliseconds);
    function done() {
      signal.removeEventListener("abort", done);
      window.clearTimeout(timeout);
      resolve();
    }
    signal.addEventListener("abort", done, { once: true });
  });
}
