import { useEffect, useRef, useState } from "react";
import { SaveIcon } from "lucide-react";

import { saveSettingsConfig } from "@/api/settings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type {
  AppConfig,
  ExternalProviderCatalog,
  Language,
  VaultConfig,
  VaultTemplatesResponse,
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

/** A starter schema preset, served by `GET /api/vault-templates`. */
export type VaultTemplateOption = VaultTemplatesResponse["templates"][number];

type SettingsEditorProps = {
  vaultConfigPath?: string;
  initialApp?: AppConfig | null;
  initialVault?: VaultConfig | null;
  providerCatalog?: ExternalProviderCatalog;
  /** Onboarding presets from the core; empty outside onboarding. */
  templates?: VaultTemplateOption[];
  /** Title-language options from the core (`GET /api/languages`). */
  languages?: Language[];
  onboarding?: boolean;
  /** When false, the schema is read-only (server enforces it too). */
  settingsWritable?: boolean;
  onBack?: () => void;
  onSaved?: () => void;
};

export function SettingsEditor({
  vaultConfigPath,
  initialApp,
  initialVault,
  providerCatalog,
  templates = [],
  languages = [],
  onboarding = false,
  settingsWritable = true,
  onBack,
  onSaved,
}: SettingsEditorProps) {
  // The editor edits the vault config (the schema) only. The vault root is owned
  // by the runtime (env / native switcher / @AppStorage) and is read-only here —
  // used for path display and the desktop "Browse" base, never edited or saved.
  const vaultRoot = initialApp?.vaultRoot ?? "";
  const [config, setConfig] = useState<VaultConfig>(() => normalizeVaultConfig(initialVault ?? undefined));
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();
  const taxonomyBase = joinPath(vaultRoot, config.taxonomyRoot);
  // The server only reports vaultConfigPath once a vault root is saved; during
  // onboarding derive it from the vault root for display.
  const vaultPath =
    vaultConfigPath ?? (vaultRoot ? joinPath(vaultRoot, ".kizunashelf/config.yaml") : undefined);
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
    ...(onboarding ? [{ id: "templates", title: "Templates", detail: `${templates.length} presets` }] : []),
    // The machine-level "App" config (vault root + write mode) is no longer
    // edited here: the self-hosted web app sources it from env vars, and the
    // desktop app from its native vault switcher.
    {
      id: "vault",
      title: "Vault",
      detail: `taxonomy: ${config.taxonomyRoot || "Taxonomy"}`,
    },
    {
      id: "daily-notes",
      title: "Daily Notes",
      detail: config.dailyNotes ? `${(config.dailyNotes.paths ?? []).length} paths` : "Off",
    },
    {
      id: "home",
      title: "Home",
      detail: config.home ? `${(config.home.sections ?? []).length} sections` : "Off",
    },
    {
      id: "types",
      title: "Types",
      detail: `${config.types.length} types`,
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
    setConfig(normalizeVaultConfig(initialVault ?? undefined));
  }, [initialVault]);

  async function save() {
    setSaving(true);
    setError(undefined);
    setMessage(undefined);
    try {
      await saveSettingsConfig(cleanVaultConfig(config, providerCatalog));
      setMessage("Saved");
      window.dispatchEvent(new Event("kizunashelf-config-saved"));
      onSaved?.();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Failed to save config");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">
            {onboarding ? "Configure Vault" : "Settings"}
          </h1>
          {vaultPath ? (
            <p className="mt-1 truncate text-xs text-muted-foreground">Vault: {vaultPath}</p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
          {onboarding && onBack ? (
            <Button type="button" variant="outline" onClick={onBack} disabled={saving}>
              Back
            </Button>
          ) : null}
          <Button type="button" onClick={save} disabled={saving || !settingsWritable}>
            <SaveIcon data-icon="inline-start" />
            {saving ? "Saving" : onboarding ? "Create Vault" : "Save"}
          </Button>
        </div>
      </header>

      {!settingsWritable ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          Schema editing is disabled on this instance (read-only). Set
          <code className="mx-1">KIZUNASHELF_SETTINGS_WRITABLE=true</code>
          to enable it.
        </div>
      ) : null}

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
        <SettingsOverview items={overviewItems} />

        {/* A disabled fieldset makes the whole schema form read-only natively. */}
        <fieldset disabled={!settingsWritable} className="m-0 flex min-w-0 flex-col gap-4 border-0 p-0">
          {onboarding ? (
            <SettingsSection
              id="templates"
              title="Create Vault Templates"
              summary={<SummaryBadges items={[`${templates.length} presets`]} />}
            >
              <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
                {templates.map((template) => (
                  <Button
                    key={template.id}
                    type="button"
                    variant="outline"
                    className="h-auto justify-start whitespace-normal py-3 text-left"
                    onClick={() => setConfig(template.config)}
                  >
                    {template.label}
                  </Button>
                ))}
              </div>
            </SettingsSection>
          ) : null}

          <SettingsSection
            id="vault"
            title="Vault"
            description={`Stored in the vault${vaultPath ? ` at ${vaultPath}` : ""}. Taxonomy, assets, daily notes, home, and types — synced with the vault.`}
            summary={<SummaryBadges items={[`assets: ${config.assetRoot || "Assets"}`]} />}
          >
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              <PathField
                label="Taxonomy root"
                value={config.taxonomyRoot}
                base={vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, taxonomyRoot: value }))}
              />
              <PathField
                label="Asset root"
                value={config.assetRoot ?? ""}
                base={vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, assetRoot: value }))}
              />
            </div>
          </SettingsSection>

          <SettingsSection
            id="daily-notes"
            title="Daily Notes"
            summary={<SummaryBadges items={config.dailyNotes ? [`${(config.dailyNotes.paths ?? []).length} paths`] : ["off"]} />}
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
              <EmptyConfigLine>Daily note indexing is disabled.</EmptyConfigLine>
            )}
          </SettingsSection>

          <SettingsSection
            id="home"
            title="Home"
            summary={<SummaryBadges items={config.home ? [`${(config.home.sections ?? []).length} sections`] : ["off"]} />}
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
              <EmptyConfigLine>Home sections are disabled.</EmptyConfigLine>
            )}
          </SettingsSection>

          <SettingsSection
            id="types"
            title="Types"
            summary={
              <SummaryBadges
                items={[
                  `${config.types.length} types`,
                  `${totalFields} fields`,
                  `${configuredProviderCount} providers`,
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
      <div className="px-2 py-1 text-xs font-medium text-muted-foreground">Overview</div>
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
