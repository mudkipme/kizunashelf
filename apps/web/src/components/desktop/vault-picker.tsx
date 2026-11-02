import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, ChevronsUpDownIcon, FolderOpenIcon } from "lucide-react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useVaultActions } from "@/hooks/use-vault-actions";
import { openVaultManager, switchVault } from "@/lib/desktop";
import { desktopVaultsQuery } from "@/lib/desktop-vaults";

function manageVaults() {
  void openVaultManager().catch((error) => toast.error(errorMessage(error)));
}

export function VaultManagerButton() {
  return (
    <Button type="button" variant="outline" onClick={manageVaults}>
      <FolderOpenIcon data-icon="inline-start" />
      <Trans>Manage vaults…</Trans>
    </Button>
  );
}

export function VaultPicker() {
  const { t } = useLingui();
  const vaults = useQuery(desktopVaultsQuery());
  const { busy, run, confirmation } = useVaultActions();
  const active = vaults.data?.find((vault) => vault.active);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            className="min-w-0 flex-1 justify-between"
            disabled={busy}
            aria-label={active ? t`Switch vault: ${active.name}` : t`Vaults`}
            title={active?.path}
          >
            <span className="truncate">{active?.name ?? t`Vaults`}</span>
            <ChevronsUpDownIcon data-icon="inline-end" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-64">
          <DropdownMenuGroup>
            {vaults.data?.map((vault) => (
              <DropdownMenuItem
                key={vault.path}
                disabled={busy || vault.active}
                title={vault.path}
                onSelect={() => void run((discard) => switchVault(vault.path, discard))}
              >
                <FolderOpenIcon />
                <span className="truncate">{vault.name}</span>
                {vault.active ? <CheckIcon className="ml-auto" /> : null}
              </DropdownMenuItem>
            ))}
          </DropdownMenuGroup>
          <DropdownMenuSeparator />
          <DropdownMenuGroup>
            <DropdownMenuItem onSelect={manageVaults}>
              <FolderOpenIcon />
              <Trans>Manage vaults…</Trans>
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      {confirmation}
    </>
  );
}
