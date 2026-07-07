import { useEffect, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { CheckIcon, FolderOpenIcon, FolderPlusIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  addVault,
  createVault,
  listVaults,
  removeVault,
  selectDirectory,
  switchVault,
  type VaultInfo,
} from "@/lib/desktop";

import { Field, SettingsSection } from "./settings-controls";
import { joinPath } from "./settings-model";

/**
 * Desktop-only multi-vault manager (Obsidian-style). Lists remembered vaults for
 * quick switching, opens an existing folder, or creates a new vault (seeded with
 * the starter template). All actions go through the Tauri commands, which rebuild
 * the in-process API for the chosen vault; `onChanged` lets the caller refetch.
 */
export function VaultSwitcher({
  onboarding = false,
  onChanged,
}: {
  onboarding?: boolean;
  onChanged?: () => void;
}) {
  const { t } = useLingui();
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listVaults().then(setVaults).catch((reason) => toast.error(message(reason, t`Something went wrong`)));
    // Mount-only vault load; `t` is only read in the error path, so re-running on
    // a locale change (which would refetch) is not wanted.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function run(action: () => Promise<VaultInfo[]>, success: string) {
    setBusy(true);
    try {
      setVaults(await action());
      onChanged?.();
      toast.success(success);
    } catch (reason) {
      toast.error(message(reason, t`Something went wrong`));
    } finally {
      setBusy(false);
    }
  }

  async function openExisting() {
    const dir = await selectDirectory().catch(() => undefined);
    if (dir) await run(() => addVault(dir), t`Vault added`);
  }

  async function chooseParent() {
    const dir = await selectDirectory(parent || undefined).catch(() => undefined);
    if (dir) setParent(dir);
  }

  const newRoot = parent && name.trim() ? joinPath(parent, name.trim()) : "";

  return (
    <SettingsSection
      title={t`Vaults`}
      description={
        onboarding
          ? t`Open or create a vault to get started.`
          : t`Switch between vaults or manage your list. Removing a vault only forgets it here — its folder is left on disk.`
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          {vaults.map((vault) => (
            <div key={vault.path} className="flex items-center gap-2 rounded-md border px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-sm">
                  {vault.active ? <CheckIcon className="size-4 text-primary" /> : null}
                  <span className="truncate font-medium">{vault.name}</span>
                </div>
                <div className="truncate text-xs text-muted-foreground">{vault.path}</div>
              </div>
              {!vault.active ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => void run(() => switchVault(vault.path), t`Switched vault`)}
                >
                  <Trans>Open</Trans>
                </Button>
              ) : null}
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                aria-label={t`Remove ${vault.name}`}
                disabled={busy}
                onClick={() => void run(() => removeVault(vault.path), t`Vault removed`)}
              >
                <Trash2Icon />
              </Button>
            </div>
          ))}
          {vaults.length === 0 ? (
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
                setName("");
                void run(() => createVault(parent, vaultName), t`Vault created`);
              }}
            >
              <Trans>Create vault</Trans>
            </Button>
          </div>
        </div>
      </div>
    </SettingsSection>
  );
}

function message(reason: unknown, fallback: string) {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string") return reason;
  return fallback;
}
