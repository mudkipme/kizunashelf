import type { ReactNode } from "react";
import { useEffect, useId, useState } from "react";
import { FolderOpenIcon, PlusIcon, SaveIcon, Trash2Icon } from "lucide-react";

import { getPathSuggestions, saveSettingsConfig } from "@/api/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { isDesktopRuntime, selectDirectory } from "@/lib/desktop";
import {
  fieldTypeLabel,
  supportsDateRole,
  supportsEnumOptions,
  supportsTitleOptions,
} from "@/lib/type-config";
import type {
  DailyNotesConfig,
  FieldConfig,
  FieldType,
  EntityTypeConfig,
  HomeConfig,
  HomeSectionConfig,
  KizunaConfig,
  SeasonLanguage,
} from "@/types/config";

type SettingsEditorProps = {
  configPath: string;
  initialConfig?: KizunaConfig;
  onboarding?: boolean;
  onSaved?: () => void;
};

export function SettingsEditor({
  configPath,
  initialConfig,
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
      await saveSettingsConfig(cleanConfig(config));
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

      {error ? <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}

      {onboarding ? (
        <SettingsSection title="Create Vault Templates">
          <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
            {vaultTemplates().map((template) => (
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
              taxonomyBase={taxonomyBase}
              onChange={(next) =>
                setConfig((current) => replaceAt(current, "types", index, next))
              }
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

function DailyNotesEditor({
  config,
  vaultRoot,
  onChange,
}: {
  config: DailyNotesConfig;
  vaultRoot: string;
  onChange: (config: DailyNotesConfig) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <StringListEditor
        label="Paths"
        values={config.paths}
        placeholder="Daily Notes"
        base={vaultRoot}
        pathItems
        onChange={(paths) => onChange({ ...config, paths })}
      />
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <TextField
          label="Date pattern"
          value={config.datePattern ?? ""}
          onChange={(datePattern) => onChange({ ...config, datePattern })}
        />
        <NumberField
          label="Snippet max length"
          value={config.snippetMaxLength}
          onChange={(snippetMaxLength) => onChange({ ...config, snippetMaxLength })}
        />
      </div>
    </div>
  );
}

function HomeEditor({
  config,
  types,
  onChange,
}: {
  config: HomeConfig;
  types: EntityTypeConfig[];
  onChange: (config: HomeConfig) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <TextField
        label="Title"
        value={config.title ?? ""}
        onChange={(title) => onChange({ ...config, title })}
      />
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-medium">Sections</h3>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => onChange({ ...config, sections: [...config.sections, defaultHomeSection(types[0]?.id)] })}
        >
          <PlusIcon data-icon="inline-start" />
          Section
        </Button>
      </div>
      <div className="flex flex-col gap-3">
        {config.sections.map((section, index) => (
          <HomeSectionEditor
            key={`${section.id}-${index}`}
            section={section}
            types={types}
            onChange={(next) => onChange(replaceAt(config, "sections", index, next))}
            onRemove={() =>
              onChange({ ...config, sections: config.sections.filter((_, itemIndex) => itemIndex !== index) })
            }
          />
        ))}
        {config.sections.length === 0 ? <EmptyConfigLine>No home sections configured.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

function HomeSectionEditor({
  section,
  types,
  onChange,
  onRemove,
}: {
  section: HomeSectionConfig;
  types: EntityTypeConfig[];
  onChange: (section: HomeSectionConfig) => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-md border p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="truncate text-sm font-medium">{section.title || section.id || "Home section"}</h4>
        <IconButton label="Remove section" onClick={onRemove} />
      </div>
      <div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-3">
        <TextField label="ID" value={section.id} onChange={(id) => onChange({ ...section, id })} />
        <TextField label="Title" value={section.title} onChange={(title) => onChange({ ...section, title })} />
        <Field label="Type">
          <Select value={section.type} onChange={(event) => onChange({ ...section, type: event.target.value })} className="h-9 w-full text-sm">
            {types.map((type) => (
              <option key={type.id} value={type.id}>
                {type.label || type.id}
              </option>
            ))}
            {!types.some((type) => type.id === section.type) ? <option value={section.type}>{section.type}</option> : null}
          </Select>
        </Field>
        <NumberField label="Limit" value={section.limit} onChange={(limit) => onChange({ ...section, limit })} />
        <TextField label="Sort" value={section.sort ?? ""} onChange={(sort) => onChange({ ...section, sort })} />
        <Field label="Direction">
          <Select
            value={section.direction ?? ""}
            onChange={(event) =>
              onChange({
                ...section,
                direction: event.target.value ? (event.target.value as "asc" | "desc") : null,
              })
            }
            className="h-9 w-full text-sm"
          >
            <option value="">Default</option>
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </Select>
        </Field>
      </div>
    </div>
  );
}

function EntityTypeEditor({
  config,
  taxonomyBase,
  onChange,
  onRemove,
}: {
  config: EntityTypeConfig;
  taxonomyBase: string;
  onChange: (config: EntityTypeConfig) => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-md border p-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="truncate text-sm font-semibold">{config.label || config.id || "Entity type"}</h3>
        <IconButton label="Remove type" onClick={onRemove} />
      </div>
      <div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-3">
        <TextField label="ID" value={config.id} onChange={(id) => onChange({ ...config, id })} />
        <TextField label="Label" value={config.label} onChange={(label) => onChange({ ...config, label })} />
        <TextField label="Icon" value={config.icon ?? ""} onChange={(icon) => onChange({ ...config, icon })} />
        <PathField label="Path" value={config.path} base={taxonomyBase} onChange={(path) => onChange({ ...config, path })} />
        <Field label="Filename title language">
          <Select
            value={config.filename?.titleLanguage ?? ""}
            onChange={(event) =>
              onChange({
                ...config,
                filename: event.target.value
                  ? {
                      titleLanguage: event.target.value,
                      defaultTitle: config.filename?.defaultTitle ?? false,
                    }
                  : null,
              })
            }
            className="h-9 w-full text-sm"
          >
            <option value="">None</option>
            <option value="zh">Chinese</option>
            <option value="ja">Japanese</option>
            <option value="en">English</option>
            <option value="original">Original</option>
          </Select>
        </Field>
        <Field label="Filename default title">
          <Select
            value={config.filename?.defaultTitle ? "true" : "false"}
            disabled={!config.filename}
            onChange={(event) =>
              onChange({
                ...config,
                filename: config.filename
                  ? { ...config.filename, defaultTitle: event.target.value === "true" }
                  : null,
              })
            }
            className="h-9 w-full text-sm"
          >
            <option value="false">No</option>
            <option value="true">Yes</option>
          </Select>
        </Field>
      </div>
      <Separator className="my-3" />
      <FieldsEditor fields={config.fields} onChange={(fields) => onChange({ ...config, fields })} />
    </div>
  );
}

function FieldsEditor({
  fields,
  onChange,
}: {
  fields: FieldConfig[];
  onChange: (fields: FieldConfig[]) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-medium">Fields</h4>
        <Button type="button" variant="outline" size="sm" onClick={() => onChange([...fields, defaultField()])}>
          <PlusIcon data-icon="inline-start" />
          Field
        </Button>
      </div>
      <div className="flex flex-col gap-3">
        {fields.map((field, index) => (
          <FieldConfigEditor
            key={`${field.field}-${field.fieldType}-${index}`}
            field={field}
            onChange={(next) => onChange(replaceArray(fields, index, next))}
            onRemove={() => onChange(fields.filter((_, itemIndex) => itemIndex !== index))}
          />
        ))}
        {fields.length === 0 ? <EmptyConfigLine>No fields configured.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

function FieldConfigEditor({
  field,
  onChange,
  onRemove,
}: {
  field: FieldConfig;
  onChange: (field: FieldConfig) => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-md border p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="truncate text-sm font-medium">{field.displayName || field.field || "Field"}</h4>
        <IconButton label="Remove field" onClick={onRemove} />
      </div>
      <div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-3">
        <TextField label="Field" value={field.field} onChange={(value) => onChange({ ...field, field: value })} />
        <Field label="Type">
          <Select
            value={field.fieldType}
            onChange={(event) => onChange({ ...field, fieldType: event.target.value as FieldType })}
            className="h-9 w-full text-sm"
          >
            {fieldTypeOptions.map((option) => (
              <option key={option} value={option}>
                {fieldTypeLabel(option)}
              </option>
            ))}
          </Select>
        </Field>
        <TextField
          label="Display name"
          value={field.displayName ?? ""}
          onChange={(displayName) => onChange({ ...field, displayName })}
        />
        {supportsTitleOptions(field.fieldType) ? (
          <>
            <TextField
              label="Title language"
              value={field.titleLanguage ?? ""}
              onChange={(titleLanguage) => onChange({ ...field, titleLanguage })}
            />
            <Field label="Default title">
              <Select
                value={field.defaultTitle ? "true" : "false"}
                onChange={(event) => onChange({ ...field, defaultTitle: event.target.value === "true" })}
                className="h-9 w-full text-sm"
              >
                <option value="false">No</option>
                <option value="true">Yes</option>
              </Select>
            </Field>
          </>
        ) : null}
        {supportsEnumOptions(field.fieldType) ? (
          <>
            <div className="lg:col-span-3">
              <StringListEditor
                label="Enum options"
                values={field.enumOptions ?? []}
                placeholder="Completed"
                onChange={(enumOptions) => onChange({ ...field, enumOptions })}
              />
            </div>
          </>
        ) : null}
        {field.fieldType === "progress" ? (
          <TextField
            label="Total progress field"
            value={field.totalProgressField ?? ""}
            onChange={(totalProgressField) => onChange({ ...field, totalProgressField })}
          />
        ) : null}
        {supportsDateRole(field.fieldType) ? (
          <Field label="Date role">
            <Select
              value={field.dateRole ?? ""}
              onChange={(event) =>
                onChange({ ...field, dateRole: (event.target.value || null) as FieldConfig["dateRole"] })
              }
              className="h-9 w-full text-sm"
            >
              <option value="">None</option>
              <option value="planning">Planning</option>
              <option value="completed">Completed</option>
            </Select>
          </Field>
        ) : null}
        {field.fieldType === "season" ? (
          <Field label="Season language">
            <Select
              value={field.seasonLanguage ?? "zh"}
              onChange={(event) => onChange({ ...field, seasonLanguage: event.target.value as SeasonLanguage })}
              className="h-9 w-full text-sm"
            >
              <option value="zh">Chinese</option>
              <option value="ja">Japanese</option>
              <option value="en">English</option>
            </Select>
          </Field>
        ) : null}
        {field.fieldType === "externalRef" ? (
          <TextField
            label="External source"
            value={field.externalRef ?? ""}
            onChange={(externalRef) => onChange({ ...field, externalRef })}
          />
        ) : null}
        {field.fieldType === "relation" ? (
          <TextField
            label="Relation type"
            value={field.relationType ?? ""}
            onChange={(relationType) => onChange({ ...field, relationType })}
          />
        ) : null}
      </div>
    </div>
  );
}

function StringListEditor({
  label,
  values,
  placeholder = "field",
  suggestions = [],
  base,
  pathItems = false,
  onChange,
}: {
  label: string;
  values: string[];
  placeholder?: string;
  suggestions?: string[];
  base?: string;
  pathItems?: boolean;
  onChange: (values: string[]) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <Button type="button" variant="outline" size="sm" onClick={() => onChange([...values, ""])}>
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => (
          <div key={index} className="flex items-center gap-2">
            {pathItems ? (
              <PathField
                value={value}
                base={base}
                placeholder={placeholder}
                onChange={(next) => onChange(replaceArray(values, index, next))}
                hideLabel
              />
            ) : (
              <InputWithSuggestions
                value={value}
                placeholder={placeholder}
                suggestions={suggestions}
                onChange={(next) => onChange(replaceArray(values, index, next))}
              />
            )}
            <IconButton label={`Remove ${label}`} onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))} />
          </div>
        ))}
        {values.length === 0 ? <EmptyConfigLine>No values.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

function KeyedStringListEditor({
  label,
  values,
  keyPlaceholder = "language",
  addLabel = "Language",
  onChange,
}: {
  label: string;
  values: Record<string, string[]>;
  keyPlaceholder?: string;
  addLabel?: string;
  onChange: (values: Record<string, string[]>) => void;
}) {
  const entries = Object.entries(values);
  function changeKey(oldKey: string, key: string) {
    const next = { ...values };
    const value = next[oldKey] ?? [];
    delete next[oldKey];
    next[key] = value;
    onChange(next);
  }
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <Button type="button" variant="outline" size="sm" onClick={() => onChange({ ...values, [keyPlaceholder]: [] })}>
          <PlusIcon data-icon="inline-start" />
          {addLabel}
        </Button>
      </div>
      {entries.map(([language, fields]) => (
        <div key={language} className="rounded-md border p-3">
          <div className="flex items-center gap-2">
            <Input
              value={language}
              placeholder={keyPlaceholder}
              onChange={(event) => changeKey(language, event.target.value)}
              aria-label={addLabel}
            />
            <IconButton
              label={`Remove ${addLabel}`}
              onClick={() => {
                const next = { ...values };
                delete next[language];
                onChange(next);
              }}
            />
          </div>
          <div className="mt-3">
            <StringListEditor
              label="Fields"
              values={fields}
              onChange={(nextFields) => onChange({ ...values, [language]: nextFields })}
            />
          </div>
        </div>
      ))}
      {entries.length === 0 ? <EmptyConfigLine>No values configured.</EmptyConfigLine> : null}
    </div>
  );
}

function PathField({
  label,
  value,
  base,
  absolute = false,
  placeholder,
  hideLabel = false,
  onChange,
}: {
  label?: string;
  value: string;
  base?: string;
  absolute?: boolean;
  placeholder?: string;
  hideLabel?: boolean;
  onChange: (value: string) => void;
}) {
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const datalistId = useId();
  const desktop = isDesktopRuntime();

  useEffect(() => {
    if (!value && !base) return;
    const controller = new AbortController();
    const timeout = window.setTimeout(() => {
      void getPathSuggestions(value, absolute ? undefined : base, { signal: controller.signal }).then(
        (result) => setSuggestions(result.suggestions),
        () => setSuggestions([]),
      );
    }, 120);
    return () => {
      controller.abort();
      window.clearTimeout(timeout);
    };
  }, [absolute, base, value]);

  async function browse() {
    const selected = await selectDirectory(base || value).catch(() => undefined);
    if (!selected) return;
    onChange(absolute || !base ? selected : relativeToBase(selected, base));
  }

  const input = (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <Input
        value={value}
        placeholder={placeholder}
        list={datalistId}
        onChange={(event) => onChange(event.target.value)}
      />
      <datalist id={datalistId}>
        {suggestions.map((suggestion) => (
          <option key={suggestion} value={suggestion} />
        ))}
      </datalist>
      {desktop ? (
        <Button type="button" variant="outline" size="icon" onClick={browse} aria-label="Select folder">
          <FolderOpenIcon />
        </Button>
      ) : null}
    </div>
  );

  if (hideLabel) return input;
  return <Field label={label ?? "Path"}>{input}</Field>;
}

function InputWithSuggestions({
  value,
  suggestions,
  placeholder,
  onChange,
}: {
  value: string;
  suggestions: string[];
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  const datalistId = useId();
  return (
    <>
      <Input value={value} placeholder={placeholder} list={datalistId} onChange={(event) => onChange(event.target.value)} />
      <datalist id={datalistId}>
        {suggestions.map((suggestion) => (
          <option key={suggestion} value={suggestion} />
        ))}
      </datalist>
    </>
  );
}

function TextField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <Field label={label}>
      <Input value={value} onChange={(event) => onChange(event.target.value)} />
    </Field>
  );
}

function NumberField({
  label,
  value,
  onChange,
}: {
  label: string;
  value?: number | null;
  onChange: (value: number | null) => void;
}) {
  return (
    <Field label={label}>
      <Input
        type="number"
        value={value ?? ""}
        onChange={(event) => onChange(event.target.value === "" ? null : Number(event.target.value))}
      />
    </Field>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

function SettingsSection({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-md border">
      <header className="flex items-center justify-between gap-2 border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        {action}
      </header>
      <div className="flex flex-col gap-4 p-3">{children}</div>
    </section>
  );
}

function OptionalToggle({
  enabled,
  onEnable,
  onDisable,
}: {
  enabled: boolean;
  onEnable: () => void;
  onDisable: () => void;
}) {
  return (
    <Button type="button" variant="outline" size="sm" onClick={enabled ? onDisable : onEnable}>
      {enabled ? "Disable" : "Enable"}
    </Button>
  );
}

function IconButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button type="button" variant="ghost" size="icon" onClick={onClick} aria-label={label}>
      <Trash2Icon />
    </Button>
  );
}

function EmptyConfigLine({ children }: { children: ReactNode }) {
  return <div className="rounded-md border border-dashed p-3 text-sm text-muted-foreground">{children}</div>;
}

function normalizeConfig(config?: KizunaConfig): KizunaConfig {
  if (!config) return defaultConfig();
  return {
    vaultRoot: config.vaultRoot ?? "",
    taxonomyRoot: config.taxonomyRoot ?? "Taxonomy",
    contentWritable: config.contentWritable ?? true,
    readConcurrency: config.readConcurrency ?? null,
    dailyNotes: config.dailyNotes ? normalizeDailyNotes(config.dailyNotes) : null,
    home: config.home ? normalizeHome(config.home) : null,
    types: (config.types ?? []).map(normalizeEntityType),
  };
}

function normalizeDailyNotes(config: DailyNotesConfig): DailyNotesConfig {
  return {
    paths: config.paths ?? [],
    datePattern: config.datePattern ?? "",
    snippetMaxLength: config.snippetMaxLength ?? null,
  };
}

function normalizeHome(config: HomeConfig): HomeConfig {
  return {
    title: config.title ?? "",
    sections: config.sections ?? [],
  };
}

function normalizeEntityType(config: EntityTypeConfig): EntityTypeConfig {
  return {
    id: config.id ?? "",
    label: config.label ?? "",
    icon: config.icon ?? "",
    path: config.path ?? "",
    filename: config.filename
      ? {
          titleLanguage: config.filename.titleLanguage ?? "",
          defaultTitle: config.filename.defaultTitle ?? false,
        }
      : null,
    fields: (config.fields ?? []).map(normalizeField),
  };
}

function normalizeField(field: FieldConfig): FieldConfig {
  return {
    field: field.field ?? "",
    fieldType: field.fieldType ?? "text",
    displayName: field.displayName ?? "",
    titleLanguage: field.titleLanguage ?? "",
    defaultTitle: field.defaultTitle ?? false,
    enumOptions: field.enumOptions ?? [],
    totalProgressField: field.totalProgressField ?? "",
    dateRole: field.dateRole ?? null,
    seasonLanguage: field.seasonLanguage ?? "zh",
    externalRef: field.externalRef ?? "",
    relationType: field.relationType ?? "",
  };
}

function cleanConfig(config: KizunaConfig): KizunaConfig {
  return {
    vaultRoot: config.vaultRoot,
    taxonomyRoot: config.taxonomyRoot,
    contentWritable: config.contentWritable ?? undefined,
    readConcurrency: config.readConcurrency ?? undefined,
    dailyNotes: config.dailyNotes
      ? {
          paths: cleanStrings(config.dailyNotes.paths),
          datePattern: emptyToUndefined(config.dailyNotes.datePattern),
          snippetMaxLength: config.dailyNotes.snippetMaxLength ?? undefined,
        }
      : undefined,
    home: config.home
      ? {
          title: emptyToUndefined(config.home.title),
          sections: config.home.sections.map((section) => ({
            id: section.id,
            title: section.title,
            type: section.type,
            limit: section.limit ?? undefined,
            sort: emptyToUndefined(section.sort),
            direction: section.direction ?? undefined,
          })),
        }
      : undefined,
    types: config.types.map((typeConfig) => ({
      id: typeConfig.id,
      label: typeConfig.label,
      icon: emptyToUndefined(typeConfig.icon),
      path: typeConfig.path,
      filename: typeConfig.filename?.titleLanguage
        ? {
            titleLanguage: typeConfig.filename.titleLanguage,
            defaultTitle: typeConfig.filename.defaultTitle || undefined,
          }
        : undefined,
      fields: typeConfig.fields
        .map(cleanField)
        .filter((field): field is FieldConfig => Boolean(field)),
    })),
  };
}

function cleanField(field: FieldConfig): FieldConfig | undefined {
  const key = field.field.trim();
  if (!key) return undefined;
  return {
    field: key,
    fieldType: field.fieldType,
    displayName: emptyToUndefined(field.displayName),
    titleLanguage: field.fieldType === "title" ? emptyToUndefined(field.titleLanguage) : undefined,
    defaultTitle: field.fieldType === "title" && field.defaultTitle ? true : undefined,
    enumOptions:
      field.fieldType === "enum" || field.fieldType === "enumList"
        ? cleanStrings(field.enumOptions ?? [])
        : undefined,
    totalProgressField:
      field.fieldType === "progress" ? emptyToUndefined(field.totalProgressField) : undefined,
    dateRole:
      field.fieldType === "date" || field.fieldType === "season" ? field.dateRole || undefined : undefined,
    seasonLanguage: field.fieldType === "season" ? field.seasonLanguage || "zh" : undefined,
    externalRef: field.fieldType === "externalRef" ? emptyToUndefined(field.externalRef) : undefined,
    relationType: field.fieldType === "relation" ? emptyToUndefined(field.relationType) : undefined,
  };
}

function defaultConfig(): KizunaConfig {
  return {
    vaultRoot: "",
    taxonomyRoot: "Taxonomy",
    contentWritable: true,
    readConcurrency: 8,
    dailyNotes: defaultDailyNotes(),
    home: defaultHome(),
    types: [defaultEntityType()],
  };
}

function defaultDailyNotes(): DailyNotesConfig {
  return {
    paths: ["Daily Notes"],
    datePattern: "^(\\d{4}-\\d{2}-\\d{2})\\.md$",
    snippetMaxLength: 260,
  };
}

function defaultHome(): HomeConfig {
  return { title: "Home", sections: [] };
}

function defaultHomeSection(type = ""): HomeSectionConfig {
  return {
    id: "section",
    title: "Section",
    type,
    limit: 12,
    sort: "title",
    direction: "asc",
  };
}

function defaultEntityType(): EntityTypeConfig {
  return {
    id: "type",
    label: "Type",
    icon: "",
    path: "Type",
    filename: { titleLanguage: "original", defaultTitle: true },
    fields: [
      { field: "id", fieldType: "id", displayName: "ID" },
      {
        field: "state",
        fieldType: "enum",
        displayName: "State",
        enumOptions: ["Backlog", "Active", "Completed", "Paused", "Dropped"],
      },
      { field: "progress", fieldType: "progress", displayName: "Progress" },
    ],
  };
}

function vaultTemplates(): Array<{ id: string; label: string; config: KizunaConfig }> {
  return [
    {
      id: "media",
      label: "Media Library",
      config: {
        ...defaultConfig(),
        types: [
          mediaType("anime", "Anime", "📺", "Anime", ["bgm_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("drama", "Drama", "🎭", "Drama", ["thetvdb_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("movie", "Movie", "🎬", "Movie", ["bgm_url", "thetvdb_url"], ["release_date"], ["complete_date"]),
          mediaType("games", "Games", "🎮", "Games", ["igdb_url"], ["release_date"], ["complete_date"]),
        ],
        home: {
          title: "Home",
          sections: [
            { id: "recent-anime", title: "Recent Anime", type: "anime", limit: 12, sort: "date:season", direction: "desc" },
            { id: "games", title: "Games", type: "games", limit: 12, sort: "title", direction: "asc" },
          ],
        },
      },
    },
    {
      id: "watching",
      label: "Anime + Drama + Movies",
      config: {
        ...defaultConfig(),
        types: [
          mediaType("anime", "Anime", "📺", "Anime", ["bgm_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("drama", "Drama", "🎭", "Drama", ["thetvdb_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("movie", "Movie", "🎬", "Movie", ["bgm_url", "thetvdb_url"], ["release_date"], ["complete_date"]),
        ],
      },
    },
    {
      id: "games",
      label: "Games",
      config: {
        ...defaultConfig(),
        types: [mediaType("games", "Games", "🎮", "Games", ["igdb_url"], ["release_date"], ["complete_date"])],
      },
    },
    {
      id: "books",
      label: "Books",
      config: {
        ...defaultConfig(),
        types: [mediaType("books", "Books", "📚", "Books", ["openlibrary_url", "isbn"], ["release_date"], ["complete_date"])],
      },
    },
    {
      id: "blank",
      label: "Custom Blank",
      config: {
        ...defaultConfig(),
        home: { title: "Home", sections: [] },
        types: [defaultEntityType()],
      },
    },
  ];
}

function mediaType(
  id: string,
  label: string,
  icon: string,
  path: string,
  externalRefs: string[],
  planningDates: string[],
  completedDates: string[],
): EntityTypeConfig {
  const stateOptions = ["Backlog", "Watching", "Playing", "Reading", "Completed", "Paused", "Dropped"];
  return {
    id,
    label,
    icon,
    path,
    filename: { titleLanguage: "zh", defaultTitle: true },
    fields: [
      { field: "uid", fieldType: "id", displayName: "UID" },
      { field: "id", fieldType: "id", displayName: "ID" },
      { field: "title", fieldType: "title", displayName: "Title", titleLanguage: "zh" },
      { field: "title_original", fieldType: "title", displayName: "Title (Original)", titleLanguage: "original" },
      { field: "title_en", fieldType: "title", displayName: "Title (English)", titleLanguage: "en" },
      { field: "title_ja", fieldType: "title", displayName: "Title (Japanese)", titleLanguage: "ja" },
      { field: "cover_url", fieldType: "image", displayName: "Cover" },
      { field: "state", fieldType: "enum", displayName: "State", enumOptions: stateOptions },
      { field: "progress", fieldType: "progress", displayName: "Progress", totalProgressField: "episodes" },
      { field: "episodes", fieldType: "totalProgress", displayName: "Episodes" },
      { field: "rating", fieldType: "rating", displayName: "Rating" },
      ...planningDates.map((field) =>
        field === "season"
          ? ({
              field,
              fieldType: "season",
              displayName: "Season",
              dateRole: "planning",
              seasonLanguage: "zh",
            } satisfies FieldConfig)
          : ({
              field,
              fieldType: "date",
              displayName: field === "release_date" ? "Release date" : field,
              dateRole: "planning",
            } satisfies FieldConfig),
      ),
      ...completedDates.map((field) => ({
        field,
        fieldType: "date",
        displayName: field === "complete_date" ? "Completed date" : field,
        dateRole: "completed",
      }) satisfies FieldConfig),
      ...externalRefs.map((field) => ({
        field,
        fieldType: "externalRef",
        displayName: field,
        externalRef: field.replace(/_url$/, ""),
      }) satisfies FieldConfig),
      { field: "franchise", fieldType: "relation", displayName: "Franchise", relationType: "franchise" },
      { field: "studio", fieldType: "relation", displayName: "Studio", relationType: "studio" },
      { field: "developer", fieldType: "relation", displayName: "Developer", relationType: "developer" },
    ],
  };
}

const fieldTypeOptions: FieldType[] = [
  "id",
  "title",
  "image",
  "imageList",
  "enum",
  "enumList",
  "progress",
  "totalProgress",
  "rating",
  "season",
  "date",
  "externalRef",
  "relation",
  "text",
  "textList",
];

function defaultField(): FieldConfig {
  return { field: "field", fieldType: "text", displayName: "" };
}

function replaceAt<T, K extends keyof T>(object: T, key: K, index: number, value: T[K] extends Array<infer U> ? U : never): T {
  const current = object[key];
  if (!Array.isArray(current)) return object;
  return { ...object, [key]: replaceArray(current, index, value) };
}

function replaceArray<T>(items: T[], index: number, value: T) {
  return items.map((item, itemIndex) => (itemIndex === index ? value : item));
}

function cleanStrings(values: string[]) {
  return values.map((value) => value.trim()).filter(Boolean);
}

function emptyToUndefined(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function joinPath(base: string, path: string) {
  if (!base) return path;
  if (!path) return base;
  return `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

function relativeToBase(path: string, base: string) {
  const normalizedBase = base.replace(/\/+$/, "");
  if (path === normalizedBase) return "";
  if (path.startsWith(`${normalizedBase}/`)) return path.slice(normalizedBase.length + 1);
  return path;
}
