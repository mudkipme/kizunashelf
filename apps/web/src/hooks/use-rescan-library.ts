import { useLingui } from "@lingui/react/macro";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { refreshLibrary } from "@/api/settings";

/**
 * Manual "I edited the vault outside the app" reindex, shared by the toolbar
 * button and the command palette so both report the same way.
 *
 * The library reloads on a TTL poll on its own, so this exists to skip the wait
 * after editing frontmatter in Obsidian. On success the whole cache is
 * invalidated so every view repulls the fresh index; failures surface through
 * the global mutation-error toast.
 */
export function useRescanLibrary() {
  const { t } = useLingui();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => refreshLibrary(),
    onSuccess: async () => {
      await queryClient.invalidateQueries();
      toast.success(t`Vault rescanned`);
    },
  });
}
