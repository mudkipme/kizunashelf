import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { isDesktopRuntime, onVaultsChanged } from "@/lib/desktop";
import { desktopVaultsKey, desktopVaultsQuery } from "@/lib/desktop-vaults";
import { resetVaultSession } from "@/lib/vault-session";

export function DesktopVaultSync() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const vaults = useQuery(desktopVaultsQuery());
  const activePath = useRef<string | null | undefined>(undefined);
  if (activePath.current === undefined && vaults.data) {
    activePath.current = vaults.data.find((vault) => vault.active)?.path ?? null;
  }

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onVaultsChanged((next) => {
      void (async () => {
        const path = next.find((vault) => vault.active)?.path ?? null;
        const changed = activePath.current !== path;
        activePath.current = path;
        if (changed) {
          await resetVaultSession(queryClient);
          navigate("/", { replace: true });
        }
        queryClient.setQueryData(desktopVaultsKey, next);
      })().catch((error) => toast.error(errorMessage(error)));
    })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((error) => toast.error(errorMessage(error)));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [navigate, queryClient]);

  return null;
}
