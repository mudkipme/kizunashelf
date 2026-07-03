import { MutationCache, QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { errorMessage, isAbortError, isConflictError } from "@/api/client";

// Toasts are the primary surface for failed writes. Every mutation error flows
// through here, so call sites no longer render their own inline error UI (the
// `AppFrame` banner is reserved for query/load errors). A mutation that handles
// its own failure can opt out with `meta: { suppressErrorToast: true }`.
declare module "@tanstack/react-query" {
  interface Register {
    mutationMeta: {
      suppressErrorToast?: boolean;
    };
  }
}

export const queryClient = new QueryClient({
  mutationCache: new MutationCache({
    onError(error, _variables, _context, mutation) {
      if (mutation.meta?.suppressErrorToast || isAbortError(error)) return;
      toast.error(
        isConflictError(error)
          ? "This changed elsewhere since you opened it — reload and try again."
          : errorMessage(error),
      );
    },
  }),
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});
