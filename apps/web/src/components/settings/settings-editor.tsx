import { useEffect, useState } from "react";
import { PlusIcon, SaveIcon } from "lucide-react";

import { saveSettingsConfig } from "@/api/settings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { ExternalProviderCatalog } from "@/types/api";
import type { KizunaConfig } from "@/types/config";

import {
  EmptyConfigLine,
  Field,
  OptionalToggle,
  PathField,
  SettingsSection,
} from "./settings-controls";
import {
  cleanConfig,
  defaultDailyNotes,
  defaultEntityType,
  defaultHome,
  joinPath,
  normalizeConfig,
  replaceAt,
  vaultTemplates,
} from "./settings-model";
import { DailyNotesEditor, EntityTypeEditor, HomeEditor } from "./settings-sections";

type SettingsEditorProps = {
  configPath: string;
  initialConfig?: KizunaConfig;
  providerCatalog?: ExternalProviderCatalog;
  onboarding?: boolean;
  onSaved?: () => void;
};

export function SettingsEditor({
  configPath,
  initialConfig,
  providerCatalog,
  onboarding = false,
  onSaved,
}: SettingsEditorProps) {
  const [config, setConfig] = useState<KizunaConfig>(() => normalizeConfig(initialConfig));
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();
  const taxonomyBase = joinPath(config.vaultRoot, config.taxonomyRoot);
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
    ...(onboarding ? [{ id: "templates", title: "Templates", detail: `${vaultTemplates(providerCatalog).length} presets` }] : []),
    {
      id: "core",
      title: "Core",
      detail: config.contentWritable === false ? "Read only" : "Writable",
    },
    {
      id: "daily-notes",
      title: "Daily Notes",
      detail: config.dailyNotes ? `${config.dailyNotes.paths.length} paths` : "Off",
    },
    {
      id: "home",
      title: "Home",
      detail: config.home ? `${config.home.sections.length} sections` : "Off",
    },
    {
      id: "types",
      title: "Types",
      detail: `${config.types.length} types`,
    },
  ];

  useEffect(() => {
    setConfig(normalizeConfig(initialConfig));
  }, [initialConfig]);

  async function save() {
    setSaving(true);
    setError(undefined);
    setMessage(undefined);
    try {
      await saveSettingsConfig(cleanConfig(config, providerCatalog));
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
            {onboarding ? "Set Up KizunaShelf" : "Settings"}
          </h1>
          <p className="mt-1 truncate text-xs text-muted-foreground">{configPath}</p>
        </div>
        <div className="flex items-center gap-2">
          {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
          <Button type="button" onClick={save} disabled={saving}>
            <SaveIcon data-icon="inline-start" />
            {saving ? "Saving" : onboarding ? "Create Config" : "Save"}
          </Button>
        </div>
      </header>

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
        <SettingsOverview items={overviewItems} />

        <div className="flex min-w-0 flex-col gap-4">
          {onboarding ? (
            <SettingsSection
              id="templates"
              title="Create Vault Templates"
              summary={<SummaryBadges items={[`${vaultTemplates(providerCatalog).length} presets`]} />}
            >
              <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
                {vaultTemplates(providerCatalog).map((template) => (
                  <Button
                    key={template.id}
                    type="button"
                    variant="outline"
                    className="h-auto justify-start whitespace-normal py-3 text-left"
                    onClick={() =>
                      setConfig((current) => ({
                        ...template.config,
                        vaultRoot: current.vaultRoot,
                        contentWritable: current.contentWritable ?? true,
                      }))
                    }
                  >
                    {template.label}
                  </Button>
                ))}
              </div>
            </SettingsSection>
          ) : null}

          <SettingsSection
            id="core"
            title="Core"
            description="Vault location, taxonomy path, and write mode."
            summary={
              <SummaryBadges
                items={[
                  config.contentWritable === false ? "read only" : "writable",
                  `assets: ${config.assetRoot || "Assets"}`,
                ]}
              />
            }
          >
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              <PathField
                label="Vault root"
                value={config.vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, vaultRoot: value }))}
                absolute
              />
              <PathField
                label="Taxonomy root"
                value={config.taxonomyRoot}
                base={config.vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, taxonomyRoot: value }))}
              />
              <PathField
                label="Asset root"
                value={config.assetRoot ?? ""}
                base={config.vaultRoot}
                onChange={(value) => setConfig((current) => ({ ...current, assetRoot: value }))}
              />
              <Field label="Content writes">
                <Select
                  value={config.contentWritable === false ? "false" : "true"}
                  onChange={(event) =>
                    setConfig((current) => ({
                      ...current,
                      contentWritable: event.target.value === "false" ? false : true,
                    }))
                  }
                  className="h-9 w-full text-sm"
                >
                  <option value="true">Enabled</option>
                  <option value="false">Read only</option>
                </Select>
              </Field>
            </div>
          </SettingsSection>

          <SettingsSection
            id="daily-notes"
            title="Daily Notes"
            summary={<SummaryBadges items={config.dailyNotes ? [`${config.dailyNotes.paths.length} paths`] : ["off"]} />}
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
                vaultRoot={config.vaultRoot}
                onChange={(dailyNotes) => setConfig((current) => ({ ...current, dailyNotes }))}
              />
            ) : (
              <EmptyConfigLine>Daily note indexing is disabled.</EmptyConfigLine>
            )}
          </SettingsSection>

          <SettingsSection
            id="home"
            title="Home"
            summary={<SummaryBadges items={config.home ? [`${config.home.sections.length} sections`] : ["off"]} />}
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
              <HomeEditor
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
            action={
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setConfig((current) => ({ ...current, types: [...current.types, defaultEntityType()] }))
                }
              >
                <PlusIcon data-icon="inline-start" />
                Type
              </Button>
            }
          >
            <div className="flex flex-col gap-3">
              {config.types.map((typeConfig, index) => (
                <EntityTypeEditor
                  key={`${typeConfig.id}-${index}`}
                  config={typeConfig}
                  providerCatalog={providerCatalog}
                  taxonomyBase={taxonomyBase}
                  onChange={(next) => setConfig((current) => replaceAt(current, "types", index, next))}
                  onRemove={() =>
                    setConfig((current) => ({
                      ...current,
                      types: current.types.filter((_, itemIndex) => itemIndex !== index),
                    }))
                  }
                />
              ))}
              {config.types.length === 0 ? <EmptyConfigLine>No entity types configured.</EmptyConfigLine> : null}
            </div>
          </SettingsSection>
        </div>
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
