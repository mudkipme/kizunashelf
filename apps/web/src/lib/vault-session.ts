import type { QueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";

let generation = 0;
const listeners = new Set<() => void>();
export function useVaultSession() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => generation,
  );
}

/** Cancel old reads before replacing all vault-scoped caches and editor state. */
export async function resetVaultSession(client: QueryClient) {
  await client.cancelQueries();
  client.clear();
  generation += 1;
  for (const listener of listeners) listener();
}
