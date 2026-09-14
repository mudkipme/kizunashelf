import { Trans } from "@lingui/react/macro";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { HouseIcon } from "lucide-react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { invalidateSmartListData } from "@/api/invalidate-smart-list-data";
import { setHomeVisibility } from "@/api/smart-lists";
import { Button } from "@/components/ui/button";
import type { SmartListDetail } from "@/types/api";

export function HomeToggle({ list, disabled }: { list: SmartListDetail; disabled?: boolean }) {
  const queryClient = useQueryClient();
  const toggle = useMutation({
    mutationFn: () => setHomeVisibility(list.id, list.revision, !list.showOnHome),
    onSuccess: () => invalidateSmartListData(queryClient),
    onError: (error) => {
      toast.error(errorMessage(error));
      void invalidateSmartListData(queryClient);
    },
  });
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={disabled || toggle.isPending}
      onClick={() => toggle.mutate()}
    >
      <HouseIcon data-icon="inline-start" />
      {list.showOnHome ? <Trans>Remove from Home</Trans> : <Trans>Add to Home</Trans>}
    </Button>
  );
}
