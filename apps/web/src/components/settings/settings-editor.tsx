import { useEffect, useRef, useState } from "react";
import { plural } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { SaveIcon } from "lucide-react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { saveSettingsConfig } from "@/api/settings";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type {
  AppConfig,
  ExternalProviderCatalog,
  Language,
  VaultConfig,
} from "@/types/api";

import {
  EmptyConfigLine,
  OptionalToggle,
  PathField,
  SettingsSection,
} from "./settings-controls";
import {
  cleanVaultConfig,
  defaultDailyNotes,
  defaultHome,
  joinPath,
  normalizeVaultConfig,
} from "./settings-model";
import { HomeBlock, TypesSection } from "./settings-dialogs";
import { DailyNotesEditor } from "./settings-sections";

type SettingsEditorProps = {
  vaultConfigPath?: string;
  initialApp?: AppConfig | null;
  initialVault?: VaultConfig | null;
  providerCatalog?: ExternalProviderCatalog;
  /** Title-language options from the core (`GET /api/languages`). */
  languages?: Language[];
  onboarding?: boolean;
  /** When false, the schema is read-only (server enforces it too). */
  settingsWritable?: boolean;
  /** Reports whether the form holds unsaved edits (differs from the saved schema). */
  onDirtyChange?: (dirty: boolean) => void;
  onBack?: () => void;
  onSaved?: () => void;
};

export function SettingsEditor({
  vaultConfigPath,
  initialApp,
  initialVault,
  providerCatalog,
  languages = [],
  onboarding = false,
  settingsWritable = true,
  onDirtyChange,
  onBack,
  onSaved,
}: SettingsEditorProps) {
  const { t } = useLingui();
  // The editor edits the vault config (the schema) only. The vault root is owned
  // by the runtime (env / native switcher / @AppStorage) and is read-only here —
  // used for path display and the desktop "Browse" base, never edited or saved.
  const vaultRoot = initialApp?.vaultRoot ?? "";
  const [config, setConfig] = useState<VaultConfig>(() => normalizeVaultConfig(initialVault ?? undefined));
  // The last saved/loaded schema object. Every edit replaces `config` with a new
  // object (immutable updates), so dirtiness is a cheap reference check — no
  // per-keystroke serialization, and untouched-dialog open/close keeps the same
  // reference (not dirty).
  const [baselineConfig, setBaselineConfig] = useState(config);
  const [saving, setSaving] = useState(false);
  const taxonomyBase = joinPath(vaultRoot, config.taxonomyRoot);
  // The server only reports vaultConfigPath once a vault root is saved; during
  // onboarding derive it from the vault root for display.
  const vaultPath =
    vaultConfigPath ?? (vaultRoot ? joinPath(vaultRoot, "KizunaShelf/config.yaml") : undefined);
  const totalFields = config.types.reduce((sum, typeConfig) => sum + typeConfig.fields.length, 0);
  const configuredProviderCount = new Set(
    config.types.flatMap((typeConfig) => [
      ...(typeConfig.externalPriority ?? []),
      ...typeConfig.fields
        .filter((field) => field.fieldType === "externalRef")
        .map((field) => field.externalRef ?? "")
        .filter(Boolean),
    ]),
  ).size;
  const overviewItems = [
    // The machine-level "App" config (vault root + write mode) is no longer
    // edited here: the self-hosted web app sources it from env vars, and the
    // desktop app from its native vault switcher.
    {
      id: "vault",
      title: t`Vault`,
      detail: t`taxonomy: ${config.taxonomyRoot || "Taxonomy"}`,
    },
    {
      id: "daily-notes",
      title: t`Daily Notes`,
      detail: config.dailyNotes
        ? plural((config.dailyNotes.paths ?? []).length, { one: "# path", other: "# paths" })
        : t`Off`,
    },
    {
      id: "home",
      title: t`Home`,
      detail: config.home
        ? plural((config.home.sections ?? []).length, { one: "# section", other: "# sections" })
        : t`Off`,
    },
    {
      id: "types",
      title: t`Types`,
      detail: plural(config.types.length, { one: "# type", other: "# types" }),
    },
  ];

  // Seed the editable config from the loaded config exactly once, when it first
  // arrives. Re-seeding on every prop identity change (e.g. a background refetch
  // on window focus) would silently discard the user's in-progress schema edits.
  const seededRef = useRef(false);
  useEffect(() => {
    if (seededRef.current) return;
    if (initialVault === undefined) return;
    seededRef.current = true;
    const seeded = normalizeVaultConfig(initialVault ?? undefined);
    setConfig(seeded);
    setBaselineConfig(seeded);
  }, [initialVault]);

  // Dirty once any edit replaces the config object; resets when the baseline is
  // re-pointed at the current draft on seed/save.
  const dirty = config !== baselineConfig;
  useEffect(() => {
    onDirtyChange?.(dirty);
    return () => onDirtyChange?.(false);
  }, [dirty, onDirtyChange]);

  async function save() {
    setSaving(true);
    try {
      await saveSettingsConfig(cleanVaultConfig(config, providerCatalog));
      setBaselineConfig(config);
      toast.success(t`Settings saved`);
      window.dispatchEvent(new Event("kizunashelf-config-saved"));
      onSaved?.();
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">
            {onboarding ? <Trans>Configure Vault</Trans> : <Trans>Settings</Trans>}
          </h1>
          {vaultPath ? (
            <p className="mt-1 truncate text-xs text-muted-foreground">
              <Trans>Vault: {vaultPath}</Trans>
            </p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {onboarding && onBack ? (
            <Button type="button" variant="outline" onClick={onBack} disabled={saving}>
              <Trans>Back</Trans>
            </Button>
          ) : null}
          <Button type="button" onClick={save} disabled={saving || !settingsWritable}>
            <SaveIcon data-icon="inline-start" />
            {saving ? t`Saving` : onboarding ? t`Create Vault` : t`Save`}
          </Button>
        </div>
      </header>

      {!settingsWritable ? (
        <Alert>
          <Trans>
            Schema editing is disabled on this instance (read-only). Set
            <code className="mx-1">KIZUNASHELF_SETTINGS_WRITABLE=true</code>
            to enable it.
          </Trans>
        </Alert>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
        <SettingsOverview items={overviewItems} />

        {/* A disabled fieldset makes the whole schema form read-only natively. */}
        <fieldset disabled={!settingsWritable} className="m-0 flex min-w-0 flex-col gap-4 border-0 p-0">
          <SettingsSection
            id="vault"
            title={t`Vault`}
            description={
              vaultPath
                ? t`Stored in the vault at ${vaultPath}. Taxonomy, assets, daily notes, home, and types — synced with the vault.`
                : t`Stored in the vault. Taxonomy, assets, daily notes, home, and types — synced with the vault.`
            }
            summary={<SummaryBadges items={[t`assets: ${config.assetRoot || "Assets"}`]} />}
          >
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              <PathField
                label={t`Taxonomy root`}
                value={config.taxonomyRoot}
                base={vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, taxonomyRoot: value }))}
              />
              <PathField
                label={t`Asset root`}
                value={config.assetRoot ?? ""}
                base={vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, assetRoot: value }))}
              />
            </div>
          </SettingsSection>

          <SettingsSection
            id="daily-notes"
            title={t`Daily Notes`}
            summary={
              <SummaryBadges
                items={
                  config.dailyNotes
                    ? [plural((config.dailyNotes.paths ?? []).length, { one: "# path", other: "# paths" })]
                    : [t`off`]
                }
              />
            }
            action={
              <OptionalToggle
                enabled={Boolean(config.dailyNotes)}
                onEnable={() =>
                  setConfig((current) => ({
                    ...current,
                    dailyNotes: current.dailyNotes ?? defaultDailyNotes(),
                  }))
                }
                onDisable={() => setConfig((current) => ({ ...current, dailyNotes: null }))}
              />
            }
          >
            {config.dailyNotes ? (
              <DailyNotesEditor
                config={config.dailyNotes}
                vaultRoot={vaultRoot}
                onChange={(dailyNotes) => setConfig((current) => ({ ...current, dailyNotes }))}
              />
            ) : (
              <EmptyConfigLine>
                <Trans>Daily note indexing is disabled.</Trans>
              </EmptyConfigLine>
            )}
          </SettingsSection>

          <SettingsSection
            id="home"
            title={t`Home`}
            summary={
              <SummaryBadges
                items={
                  config.home
                    ? [plural((config.home.sections ?? []).length, { one: "# section", other: "# sections" })]
                    : [t`off`]
                }
              />
            }
            action={
              <OptionalToggle
                enabled={Boolean(config.home)}
                onEnable={() =>
                  setConfig((current) => ({ ...current, home: current.home ?? defaultHome() }))
                }
                onDisable={() => setConfig((current) => ({ ...current, home: null }))}
              />
            }
          >
            {config.home ? (
              <HomeBlock
                config={config.home}
                types={config.types}
                onChange={(home) => setConfig((current) => ({ ...current, home }))}
              />
            ) : (
              <EmptyConfigLine>
                <Trans>Home sections are disabled.</Trans>
              </EmptyConfigLine>
            )}
          </SettingsSection>

          <SettingsSection
            id="types"
            title={t`Types`}
            summary={
              <SummaryBadges
                items={[
                  plural(config.types.length, { one: "# type", other: "# types" }),
                  plural(totalFields, { one: "# field", other: "# fields" }),
                  plural(configuredProviderCount, { one: "# provider", other: "# providers" }),
                ]}
              />
            }
          >
            <TypesSection
              types={config.types}
              providerCatalog={providerCatalog}
              languages={languages}
              taxonomyBase={taxonomyBase}
              taxonomyRoot={config.taxonomyRoot}
              onChange={(types) => setConfig((current) => ({ ...current, types }))}
            />
          </SettingsSection>
        </fieldset>
      </div>
    </div>
  );
}

function SettingsOverview({
  items,
}: {
  items: Array<{ id: string; title: string; detail: string }>;
}) {
  return (
    <aside className="h-fit rounded-md border p-2 lg:sticky lg:top-3">
      <div className="px-2 py-1 text-xs font-medium text-muted-foreground">
        <Trans>Overview</Trans>
      </div>
      <div className="mt-1 flex flex-col gap-1">
        {items.map((item) => (
          <Button key={item.id} asChild variant="ghost" className="h-auto justify-between px-2 py-2">
            <a href={`#${item.id}`} className="min-w-0">
              <span className="truncate text-sm">{item.title}</span>
              <span className="ml-2 shrink-0 text-xs text-muted-foreground">{item.detail}</span>
            </a>
          </Button>
        ))}
      </div>
    </aside>
  );
}

function SummaryBadges({ items }: { items: string[] }) {
  return (
    <div className="flex flex-wrap gap-1">
      {items.map((item) => (
        <Badge key={item} variant="secondary">
          {item}
        </Badge>
      ))}
    </div>
  );
}
