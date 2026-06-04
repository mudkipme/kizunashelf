import type { ReactNode } from "react";
import { useEffect, useId, useState } from "react";
import { FolderOpenIcon, PlusIcon, SaveIcon, Trash2Icon } from "lucide-react";

import { getPathSuggestions, saveSettingsConfig } from "@/api/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { isDesktopRuntime, selectDirectory } from "@/lib/desktop";
import type {
  DailyNotesConfig,
  EntityFieldsConfig,
  EntityTypeConfig,
  HomeConfig,
  HomeSectionConfig,
  KizunaConfig,
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
        </div>
        <StringListEditor
          label="Relationship fields"
          values={config.relationshipFields}
          placeholder="frontmatter field"
          onChange={(relationshipFields) => setConfig((current) => ({ ...current, relationshipFields }))}
        />
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
              relationshipFields={config.relationshipFields}
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
  const statusMode = getStatusMode(section.status);

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
        <Field label="Status mode">
          <Select
            value={statusMode}
            onChange={(event) => {
              const mode = event.target.value as StatusMode;
              onChange({ ...section, status: statusForMode(mode, section.status) });
            }}
            className="h-9 w-full text-sm"
          >
            <option value="none">Any status</option>
            <option value="one">One status</option>
            <option value="many">Multiple statuses</option>
          </Select>
        </Field>
        {statusMode === "one" ? (
          <TextField
            label="Status"
            value={typeof section.status === "string" ? section.status : section.status?.[0] ?? ""}
            onChange={(status) => onChange({ ...section, status })}
          />
        ) : null}
        {statusMode === "many" ? (
          <StringListEditor
            label="Statuses"
            values={Array.isArray(section.status) ? section.status : section.status ? [section.status] : []}
            placeholder="status"
            onChange={(status) => onChange({ ...section, status })}
          />
        ) : null}
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
  relationshipFields,
  onChange,
  onRemove,
}: {
  config: EntityTypeConfig;
  taxonomyBase: string;
  relationshipFields: string[];
  onChange: (config: EntityTypeConfig) => void;
  onRemove: () => void;
}) {
  const languageOptions = Object.keys(config.fields.titleLanguages);
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
        <Field label="Default title language">
          <Select
            value={config.defaultTitleLanguage ?? ""}
            onChange={(event) => onChange({ ...config, defaultTitleLanguage: event.target.value })}
            className="h-9 w-full text-sm"
          >
            <option value="">None</option>
            {config.defaultTitleLanguage && !languageOptions.includes(config.defaultTitleLanguage) ? (
              <option value={config.defaultTitleLanguage}>{config.defaultTitleLanguage}</option>
            ) : null}
            {languageOptions.map((language) => (
              <option key={language} value={language}>
                {language}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <Separator className="my-3" />
      <EntityFieldsEditor
        fields={config.fields}
        relationshipFields={relationshipFields}
        onChange={(fields) => onChange({ ...config, fields })}
      />
    </div>
  );
}

function EntityFieldsEditor({
  fields,
  relationshipFields,
  onChange,
}: {
  fields: EntityFieldsConfig;
  relationshipFields: string[];
  onChange: (fields: EntityFieldsConfig) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <KeyedStringListEditor
        label="Title languages"
        values={fields.titleLanguages}
        onChange={(titleLanguages) => onChange({ ...fields, titleLanguages })}
      />
      <div className="flex flex-col gap-3">
        <StringListEditor label="Subtitle fields" values={fields.subtitle} onChange={(subtitle) => onChange({ ...fields, subtitle })} />
        <StringListEditor label="Image fields" values={fields.image} onChange={(image) => onChange({ ...fields, image })} />
        <StringListEditor label="Status fields" values={fields.status} onChange={(status) => onChange({ ...fields, status })} />
        <StringListEditor
          label="External ref fields"
          values={fields.externalRefs}
          onChange={(externalRefs) => onChange({ ...fields, externalRefs })}
        />
        <StringListEditor
          label="Relation fields"
          values={fields.relations}
          suggestions={relationshipFields}
          onChange={(relations) => onChange({ ...fields, relations })}
        />
      </div>
      <div className="flex flex-col gap-3 xl:col-span-2">
        <StringListEditor
          label="Planning date fields"
          values={fields.dateRoles.planning}
          onChange={(planning) => onChange({ ...fields, dateRoles: { ...fields.dateRoles, planning } })}
        />
        <StringListEditor
          label="Completed date fields"
          values={fields.dateRoles.completed}
          onChange={(completed) => onChange({ ...fields, dateRoles: { ...fields.dateRoles, completed } })}
        />
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
  onChange,
}: {
  label: string;
  values: Record<string, string[]>;
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
        <Button type="button" variant="outline" size="sm" onClick={() => onChange({ ...values, language: [] })}>
          <PlusIcon data-icon="inline-start" />
          Language
        </Button>
      </div>
      {entries.map(([language, fields]) => (
        <div key={language} className="rounded-md border p-3">
          <div className="flex items-center gap-2">
            <Input value={language} onChange={(event) => changeKey(language, event.target.value)} aria-label="Language" />
            <IconButton
              label="Remove language"
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
      {entries.length === 0 ? <EmptyConfigLine>No title languages configured.</EmptyConfigLine> : null}
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
    relationshipFields: config.relationshipFields ?? [],
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
    ...config,
    icon: config.icon ?? "",
    defaultTitleLanguage: config.defaultTitleLanguage ?? "",
    fields: {
      titleLanguages: config.fields?.titleLanguages ?? {},
      subtitle: config.fields?.subtitle ?? [],
      image: config.fields?.image ?? [],
      status: config.fields?.status ?? [],
      dateRoles: {
        planning: config.fields?.dateRoles?.planning ?? [],
        completed: config.fields?.dateRoles?.completed ?? [],
      },
      externalRefs: config.fields?.externalRefs ?? [],
      relations: config.fields?.relations ?? [],
    },
  };
}

function cleanConfig(config: KizunaConfig): KizunaConfig {
  return {
    vaultRoot: config.vaultRoot,
    taxonomyRoot: config.taxonomyRoot,
    relationshipFields: cleanStrings(config.relationshipFields),
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
            status: section.status ?? undefined,
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
      defaultTitleLanguage: emptyToUndefined(typeConfig.defaultTitleLanguage),
      fields: {
        titleLanguages: Object.fromEntries(
          Object.entries(typeConfig.fields.titleLanguages)
            .map(([language, fields]) => [language, cleanStrings(fields)] as const)
            .filter(([language, fields]) => language.trim() && fields.length > 0),
        ),
        subtitle: cleanStrings(typeConfig.fields.subtitle),
        image: cleanStrings(typeConfig.fields.image),
        status: cleanStrings(typeConfig.fields.status),
        dateRoles: {
          planning: cleanStrings(typeConfig.fields.dateRoles.planning),
          completed: cleanStrings(typeConfig.fields.dateRoles.completed),
        },
        externalRefs: cleanStrings(typeConfig.fields.externalRefs),
        relations: cleanStrings(typeConfig.fields.relations),
      },
    })),
  };
}

function defaultConfig(): KizunaConfig {
  return {
    vaultRoot: "",
    taxonomyRoot: "Taxonomy",
    relationshipFields: [],
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
    status: null,
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
    defaultTitleLanguage: "original",
    fields: {
      titleLanguages: { original: ["filename"] },
      subtitle: [],
      image: [],
      status: [],
      dateRoles: { planning: [], completed: [] },
      externalRefs: [],
      relations: [],
    },
  };
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

type StatusMode = "none" | "one" | "many";

function getStatusMode(status: HomeSectionConfig["status"]): StatusMode {
  if (Array.isArray(status)) return "many";
  return status ? "one" : "none";
}

function statusForMode(mode: StatusMode, current: HomeSectionConfig["status"]): HomeSectionConfig["status"] {
  if (mode === "none") return null;
  if (mode === "one") return typeof current === "string" ? current : current?.[0] ?? "";
  return Array.isArray(current) ? current : current ? [current] : [];
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
