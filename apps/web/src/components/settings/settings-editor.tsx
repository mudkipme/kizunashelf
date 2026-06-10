import { useEffect, useState } from "react";
import { PlusIcon, SaveIcon } from "lucide-react";

import { saveSettingsConfig } from "@/api/settings";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { ExternalProviderCatalog } from "@/types/api";
import type { KizunaConfig } from "@/types/config";

import {
  EmptyConfigLine,
  Field,
  NumberField,
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

      {onboarding ? (
        <SettingsSection title="Create Vault Templates">
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

      <SettingsSection title="Core">
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
          <NumberField
            label="Read concurrency"
            value={config.readConcurrency}
            onChange={(value) => setConfig((current) => ({ ...current, readConcurrency: value }))}
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
        title="Daily Notes"
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
        title="Home"
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
        title="Types"
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
  );
}
