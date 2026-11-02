import { Trans } from "@lingui/react/macro";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import type { VaultInfo } from "@/lib/desktop";
import { desktopVaultsKey } from "@/lib/desktop-vaults";

type VaultAction = (discardChanges: boolean) => Promise<VaultInfo[]>;

export function useVaultActions() {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<{ action: VaultAction } | null>(null);

  async function run(action: VaultAction, discardChanges = false) {
    setBusy(true);
    try {
      const vaults = await action(discardChanges);
      queryClient.setQueryData(desktopVaultsKey, vaults);
      return true;
    } catch (error) {
      if (error === "vault_has_unsaved_changes") setPending({ action });
      else toast.error(errorMessage(error));
      return false;
    } finally {
      setBusy(false);
    }
  }

  const confirmation = (
    <AlertDialog
      open={pending !== null}
      onOpenChange={(open) => {
        if (!open) setPending(null);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Trans>Discard unsaved changes?</Trans>
          </AlertDialogTitle>
          <AlertDialogDescription>
            <Trans>Switching vaults will discard your unsaved edits.</Trans>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            <Trans>Keep editing</Trans>
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (pending) {
                const { action } = pending;
                setPending(null);
                void run(action, true);
              }
            }}
          >
            <Trans>Discard and switch</Trans>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  return { busy, run, confirmation };
}
