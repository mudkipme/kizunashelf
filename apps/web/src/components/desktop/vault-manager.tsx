import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, FolderOpenIcon, FolderPlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { Field, SettingsSection } from "@/components/settings/settings-controls";
import { joinPath } from "@/components/settings/settings-model";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useVaultActions } from "@/hooks/use-vault-actions";
import { addVault, createVault, removeVault, selectDirectory, switchVault } from "@/lib/desktop";
import { desktopVaultsQuery } from "@/lib/desktop-vaults";

export function VaultManager() {
  const { t } = useLingui();
  const vaults = useQuery(desktopVaultsQuery());
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const { busy, run, confirmation } = useVaultActions();

  async function openExisting() {
    try {
      const dir = await selectDirectory();
      if (dir) await run((discard) => addVault(dir, discard));
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }

  async function chooseParent() {
    try {
      const dir = await selectDirectory(parent || undefined);
      if (dir) setParent(dir);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }

  const newRoot = parent && name.trim() ? joinPath(parent, name.trim()) : "";

  return (
    <>
      <SettingsSection title={t`Vaults`} description={t`Open or create a vault to get started.`}>
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            {vaults.data?.map((vault) => (
              <div key={vault.path} className="flex items-center gap-2 rounded-md border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 text-sm">
                    {vault.active ? <CheckIcon className="size-4 text-primary" /> : null}
                    <span className="truncate font-medium">{vault.name}</span>
                  </div>
                  <div className="truncate text-xs text-muted-foreground">{vault.path}</div>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => void run((discard) => switchVault(vault.path, discard))}
                >
                  <Trans>Open</Trans>
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={t`Remove ${vault.name}`}
                  disabled={busy}
                  onClick={() => void run((discard) => removeVault(vault.path, discard))}
                >
                  <Trash2Icon />
                </Button>
              </div>
            ))}
            {vaults.error ? (
              <p role="alert" className="text-sm text-destructive">
                {errorMessage(vaults.error)}
              </p>
            ) : null}
            {vaults.data?.length === 0 ? (
              <div className="rounded-md border border-dashed px-3 py-4 text-center text-xs text-muted-foreground">
                <Trans>No vaults yet.</Trans>
              </div>
            ) : null}
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <div className="flex flex-col gap-2 rounded-md border p-3">
              <div className="text-sm font-medium">
                <Trans>Open existing vault</Trans>
              </div>
              <Button type="button" variant="outline" disabled={busy} onClick={openExisting}>
                <FolderOpenIcon data-icon="inline-start" />
                <Trans>Open folder</Trans>
              </Button>
            </div>
            <div className="flex flex-col gap-2 rounded-md border p-3">
              <div className="text-sm font-medium">
                <Trans>Create new vault</Trans>
              </div>
              <Field label={t`Parent folder`}>
                <div className="flex items-center gap-2">
                  <Input value={parent} readOnly placeholder={t`No folder selected`} />
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    aria-label={t`Select parent folder`}
                    onClick={chooseParent}
                  >
                    <FolderPlusIcon />
                  </Button>
                </div>
              </Field>
              <Field label={t`Vault name`}>
                <Input
                  value={name}
                  placeholder={t`My Vault`}
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
              <Button
                type="button"
                disabled={busy || !newRoot}
                onClick={() => {
                  const vaultName = name.trim();
                  void run((discard) => createVault(parent, vaultName, discard)).then((created) => {
                    if (created) setName("");
                  });
                }}
              >
                <Trans>Create vault</Trans>
              </Button>
            </div>
          </div>
        </div>
      </SettingsSection>
      {confirmation}
    </>
  );
}
