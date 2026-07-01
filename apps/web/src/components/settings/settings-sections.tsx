import { createContext, useContext, type ReactNode } from "react";
import { PlusIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import {
  externalFieldOptionsForSource,
  externalSourceOptions,
  externalTypeOptionsForSource,
  externalTypesForSource,
} from "@/lib/external-metadata";
import { fieldDisplayLabel, fieldTypeLabel, isDateFieldType, supportsEnumOptions } from "@/lib/type-config";
import type {
  BodySection,
  BodySectionKind,
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalFieldMapping,
  ExternalProviderCatalog,
  FieldConfig,
  FieldType,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  Language,
  SeasonLanguage,
} from "@/types/api";

import {
  EmptyConfigLine,
  Field,
  IconButton,
  NumberField,
  OptionalToggle,
  PathField,
  StringListEditor,
  TextField,
  UnknownValueOption,
} from "./settings-controls";
import {
  fieldOptionKeys,
  fieldTypeOptions,
  type FieldOptionKey,
} from "./settings-field-descriptors";
import { arrayEditor } from "./settings-model";

export function DailyNotesEditor({
  config,
  vaultRoot,
  onChange,
}: {
  config: DailyNotesConfig;
  vaultRoot: string;
  onChange: (config: DailyNotesConfig) => void;
}) {
  const log = config.log ?? { section: "", lineFormat: "" };
  return (
    <div className="flex flex-col gap-3">
      <StringListEditor
        label="Paths"
        values={config.paths ?? []}
        placeholder="Daily Notes"
        base={vaultRoot}
        pathItems
        onChange={(paths) => onChange({ ...config, paths })}
      />
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <TextField
          label="Date format"
          value={config.dateFormat ?? ""}
          placeholder="YYYY-MM-DD"
          onChange={(dateFormat) => onChange({ ...config, dateFormat })}
        />
        <TextField
          label="New-note template"
          value={config.template ?? ""}
          placeholder="Templates/Daily Note.md"
          onChange={(template) => onChange({ ...config, template })}
        />
      </div>
      <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
        <div className="text-xs text-muted-foreground">
          Logging defaults — the heading log lines are written under, and the fallback line format.
          Each type can override these and adds its own tag.
        </div>
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <TextField
            label="Log section"
            value={log.section ?? ""}
            placeholder="Log"
            onChange={(section) => onChange({ ...config, log: { ...log, section } })}
          />
          <TextField
            label="Default line format"
            value={log.lineFormat ?? ""}
            placeholder="- {title} {note}"
            onChange={(lineFormat) => onChange({ ...config, log: { ...log, lineFormat } })}
          />
        </div>
      </div>
    </div>
  );
}

export function HomeSectionForm({
  section,
  types,
  onChange,
}: {
  section: HomeSectionConfig;
  types: EntityTypeConfig[];
  onChange: (section: HomeSectionConfig) => void;
}) {
  const selectedType = types.find((type) => type.id === section.type);
  const filterFields = selectedType?.fields.filter((field) => supportsEnumOptions(field.fieldType)) ?? [];
  const filters = section.filters ?? [];
  const sortOptions = [
    { value: "title", label: "Title" },
    { value: "recentlyUpdated", label: "Update time" },
    { value: "relationCount", label: "Relation count" },
    ...(selectedType?.fields ?? [])
      .filter((field) => isDateFieldType(field.fieldType))
      .map((field) => ({ value: `date:${field.field}`, label: `Date: ${fieldDisplayLabel(field)}` })),
  ];
  const currentSort = section.sort ?? "title";
  const filterList = arrayEditor(filters, (next) => onChange({ ...section, filters: next }));

  function updateFilter(index: number, filter: HomeSectionFilterConfig) {
    filterList.update(index, filter);
  }

  function removeFilter(index: number) {
    filterList.remove(index);
  }

  function addFilter() {
    const field = filterFields[0]?.field ?? "";
    filterList.append({ field, values: [] });
  }

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      <TextField label="ID" value={section.id} onChange={(id) => onChange({ ...section, id })} />
        <TextField label="Title" value={section.title} onChange={(title) => onChange({ ...section, title })} />
        <Field label="Type">
          <Select
            value={section.type}
            onChange={(event) => onChange({ ...section, type: event.target.value, filters: [] })}
            className="h-9 w-full text-base md:text-sm"
          >
            {types.map((type) => (
              <option key={type.id} value={type.id}>
                {type.label || type.id}
              </option>
            ))}
            <UnknownValueOption value={section.type} known={types.map((type) => type.id)} />
          </Select>
        </Field>
        <div className="lg:col-span-3">
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-sm font-medium">Filters</div>
                <div className="text-xs text-muted-foreground">Enum and enum list fields from this type.</div>
              </div>
              <Button type="button" variant="outline" size="sm" onClick={addFilter} disabled={filterFields.length === 0}>
                <PlusIcon data-icon="inline-start" />
                Add Filter
              </Button>
            </div>
            {filters.map((filter, index) => (
              <HomeSectionFilterEditor
                // Index, not filter.field: the field is editable; keying on it
                // would remount and drop focus on each change.
                key={index}
                filter={filter}
                fields={filterFields}
                onChange={(nextFilter) => updateFilter(index, nextFilter)}
                onRemove={() => removeFilter(index)}
              />
            ))}
            {filterFields.length === 0 ? <EmptyConfigLine>No enum fields available for this type.</EmptyConfigLine> : null}
          </div>
        </div>
        <NumberField label="Limit" value={section.limit} onChange={(limit) => onChange({ ...section, limit })} />
        <Field label="Sort">
          <Select
            value={currentSort}
            onChange={(event) => onChange({ ...section, sort: event.target.value })}
            className="h-9 w-full text-base md:text-sm"
          >
            {sortOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
            <UnknownValueOption value={currentSort} known={sortOptions.map((option) => option.value)} />
          </Select>
        </Field>
        <Field label="Direction">
          <Select
            value={section.direction ?? ""}
            onChange={(event) =>
              onChange({
                ...section,
                direction: event.target.value ? (event.target.value as "asc" | "desc") : null,
              })
            }
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="">Default</option>
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </Select>
        </Field>
    </div>
  );
}

function HomeSectionFilterEditor({
  filter,
  fields,
  onChange,
  onRemove,
}: {
  filter: HomeSectionFilterConfig;
  fields: FieldConfig[];
  onChange: (filter: HomeSectionFilterConfig) => void;
  onRemove: () => void;
}) {
  const selectedField = fields.find((field) => field.field === filter.field);
  const suggestions = selectedField?.enumOptions ?? [];
  return (
    <div className="rounded-md border border-dashed p-3">
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto]">
        <Field label="Field">
          <Select
            value={filter.field}
            onChange={(event) => onChange({ ...filter, field: event.target.value, values: [] })}
            className="h-9 w-full text-base md:text-sm"
          >
            {fields.map((field) => (
              <option key={field.field} value={field.field}>
                {field.displayName || field.field}
              </option>
            ))}
            <UnknownValueOption value={filter.field} known={fields.map((field) => field.field)} />
          </Select>
        </Field>
        <StringListEditor
          label="Values"
          values={filter.values ?? []}
          suggestions={suggestions}
          placeholder={suggestions[0] ?? "value"}
          onChange={(values) => onChange({ ...filter, values })}
        />
        <div className="flex items-end">
          <IconButton label="Remove filter" onClick={onRemove} />
        </div>
      </div>
    </div>
  );
}

// Title-language options (from `GET /api/languages`) made available to the
// nested field editors without drilling through every intermediate component.
export const TitleLanguagesContext = createContext<Language[]>([]);

/// A title-language picker over the supported languages. `value` is an empty
/// string for "None"; an unrecognized configured code is preserved as its own
/// option so editing never silently drops it.
function LanguageSelect({
  value,
  languages,
  onChange,
}: {
  value: string;
  languages: Language[];
  onChange: (value: string) => void;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="h-9 w-full text-base md:text-sm"
    >
      <option value="">None</option>
      {languages.map((language) => (
        <option key={language.code} value={language.code}>
          {language.label} ({language.code})
        </option>
      ))}
      <UnknownValueOption value={value} known={languages.map((language) => language.code)} />
    </Select>
  );
}

/// The basics / providers / filename portion of an entity type — everything
/// except its fields list, which is edited via a drill-down in the type dialog.
export function EntityTypeForm({
  config,
  providerCatalog,
  languages,
  taxonomyBase,
  taxonomyRoot,
  onChange,
}: {
  config: EntityTypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  languages: Language[];
  /** Absolute taxonomy dir (desktop "Browse" base). */
  taxonomyBase: string;
  /** Vault-relative taxonomy root; the type path is relative to it. */
  taxonomyRoot: string;
  onChange: (config: EntityTypeConfig) => void;
}) {
  return (
      <div className="flex flex-col gap-4">
        <ConfigSubsection title="Basics">
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-4">
            <TextField label="ID" value={config.id} onChange={(id) => onChange({ ...config, id })} />
            <TextField label="Label" value={config.label} onChange={(label) => onChange({ ...config, label })} />
            <TextField label="Icon" value={config.icon ?? ""} onChange={(icon) => onChange({ ...config, icon })} />
            <PathField
              label="Path"
              value={config.path}
              base={taxonomyBase}
              suggestionBase={taxonomyRoot}
              onChange={(path) => onChange({ ...config, path })}
            />
          </div>
        </ConfigSubsection>

        <ConfigSubsection title="Providers">
          <ExternalPriorityEditor
            providerCatalog={providerCatalog}
            values={config.externalPriority ?? []}
            onChange={(externalPriority) => onChange({ ...config, externalPriority })}
          />
          <BodySectionsEditor
            providerCatalog={providerCatalog}
            values={config.bodySections ?? []}
            onChange={(bodySections) => onChange({ ...config, bodySections })}
          />
        </ConfigSubsection>

        <ConfigSubsection title="Filename">
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
            <Field label="Filename title language">
              <LanguageSelect
                value={config.filename?.titleLanguage ?? ""}
                languages={languages}
                onChange={(titleLanguage) =>
                  onChange({
                    ...config,
                    filename: {
                      titleLanguage: titleLanguage || undefined,
                      titleRole: config.filename?.titleRole ?? undefined,
                    },
                  })
                }
              />
            </Field>
            <Field label="Filename title — used as">
              <Select
                value={config.filename?.titleRole ?? ""}
                onChange={(event) =>
                  onChange({
                    ...config,
                    filename: {
                      titleLanguage: config.filename?.titleLanguage,
                      titleRole: event.target.value === "original" ? "original" : undefined,
                    },
                  })
                }
                className="h-9 w-full text-base md:text-sm"
              >
                <option value="">None</option>
                <option value="original">Original (filename is the title)</option>
              </Select>
            </Field>
          </div>
        </ConfigSubsection>

        <ConfigSubsection title="Daily-note logging">
          <TypeLogEditor config={config} onChange={onChange} />
        </ConfigSubsection>
      </div>
  );
}

/// The per-type daily-note logging block. The toggle is the opt-in: with it off
/// the type isn't loggable (no `log` block); on, it logs under the daily-notes
/// section (or a per-type override) using this line format. `{title}` is always
/// rendered as a wikilink, so the format just carries the tag.
function TypeLogEditor({
  config,
  onChange,
}: {
  config: EntityTypeConfig;
  onChange: (config: EntityTypeConfig) => void;
}) {
  const log = config.log ?? null;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs font-medium text-muted-foreground">Log to daily note</div>
          <div className="text-xs text-muted-foreground">
            When on, checking an episode or the Log button writes a line to the daily note. The title
            is auto-linked as a wikilink — just add your tag to the line format.
          </div>
        </div>
        <OptionalToggle
          enabled={Boolean(log)}
          onEnable={() => onChange({ ...config, log: { section: "", lineFormat: "" } })}
          onDisable={() => onChange({ ...config, log: null })}
        />
      </div>
      {log ? (
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <TextField
            label="Line format"
            value={log.lineFormat ?? ""}
            placeholder="- {title} {note} #Tag"
            onChange={(lineFormat) => onChange({ ...config, log: { ...log, lineFormat } })}
          />
          <TextField
            label="Section override"
            value={log.section ?? ""}
            placeholder="(daily-notes default)"
            onChange={(section) => onChange({ ...config, log: { ...log, section } })}
          />
        </div>
      ) : null}
    </div>
  );
}

function ConfigSubsection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h4 className="text-xs font-semibold uppercase text-muted-foreground">{title}</h4>
      {children}
    </div>
  );
}

function ExternalPriorityEditor({
  providerCatalog,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const sourceOptions = externalSourceOptions(providerCatalog);
  const available = sourceOptions.filter((option) => !values.includes(option.source));
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">Provider priority</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={available.length === 0}
          onClick={() => list.append(available[0]?.source ?? "")}
        >
          <PlusIcon data-icon="inline-start" />
          Provider
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => {
          const options = sourceOptions.filter(
            (option) => option.source === value || !values.includes(option.source),
          );
          return (
            <div key={index} className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2">
              <span className="text-xs font-medium tabular-nums text-muted-foreground">{index + 1}</span>
              <Select
                value={value}
                onChange={(event) => list.update(index, event.target.value)}
                aria-label="Provider priority"
              >
                {options.map((option) => (
                  <option key={option.source} value={option.source}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <IconButton
                label="Remove provider priority"
                onClick={() => list.remove(index)}
              />
            </div>
          );
        })}
        {values.length === 0 ? <EmptyConfigLine>Default provider order is used.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

type BodySectionTracking = NonNullable<BodySection["tracking"]>;

const EPISODE_TRACKING_OPTIONS: { value: BodySectionTracking; label: string }[] = [
  { value: "checklist", label: "Checklist (per-item checkboxes)" },
  { value: "none", label: "None (plain list)" },
];

/// Reshapes a section when its `kind` changes, dropping the now-irrelevant
/// payload so the saved config doesn't carry stale fields from the other kind.
function changeBodySectionKind(section: BodySection, kind: BodySectionKind): BodySection {
  if (kind === "episodes") {
    return { heading: section.heading, kind, tracking: section.tracking ?? "checklist" };
  }
  return { heading: section.heading, kind, externalFields: section.externalFields ?? [] };
}

function BodySectionsEditor({
  providerCatalog,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  values: BodySection[];
  onChange: (values: BodySection[]) => void;
}) {
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">Markdown body sections</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => list.append({ heading: "Summary", kind: "external", externalFields: [] })}
        >
          <PlusIcon data-icon="inline-start" />
          Section
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((section, index) => (
          <BodySectionEditor
            // Index, not heading: the heading is editable; keying on it would
            // remount and drop focus on each keystroke.
            key={index}
            providerCatalog={providerCatalog}
            section={section}
            onChange={(next) => list.update(index, next)}
            onRemove={() => list.remove(index)}
          />
        ))}
        {values.length === 0 ? <EmptyConfigLine>No markdown body sections.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

function BodySectionEditor({
  providerCatalog,
  section,
  onChange,
  onRemove,
}: {
  providerCatalog?: ExternalProviderCatalog;
  section: BodySection;
  onChange: (section: BodySection) => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
        <TextField
          label="Heading"
          value={section.heading}
          onChange={(heading) => onChange({ ...section, heading })}
        />
        <Field label="Kind">
          <Select
            value={section.kind}
            onChange={(event) => onChange(changeBodySectionKind(section, event.target.value as BodySectionKind))}
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="external">External metadata</option>
            <option value="episodes">Item list</option>
            <UnknownValueOption value={section.kind} known={["external", "episodes"]} />
          </Select>
        </Field>
        <div className="flex items-end">
          <IconButton label="Remove body section" onClick={onRemove} />
        </div>
      </div>
      {section.kind === "episodes" ? (
        <Field label="Tracking">
          <Select
            value={section.tracking ?? "checklist"}
            onChange={(event) => onChange({ ...section, tracking: event.target.value as BodySectionTracking })}
            className="h-9 w-full text-base md:text-sm"
          >
            {EPISODE_TRACKING_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
            <UnknownValueOption
              value={section.tracking ?? "checklist"}
              known={EPISODE_TRACKING_OPTIONS.map((option) => option.value)}
            />
          </Select>
        </Field>
      ) : (
        <ExternalFieldMappingsEditor
          providerCatalog={providerCatalog}
          values={section.externalFields ?? []}
          onChange={(externalFields) => onChange({ ...section, externalFields })}
        />
      )}
    </div>
  );
}

/// The editor body for a single field. Rendered inside the type dialog's
/// field drill-down view; must be wrapped in a [`TitleLanguagesContext`]
/// provider so the title-language picker has its options.
export function FieldForm({
  providerCatalog,
  field,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  field: FieldConfig;
  onChange: (field: FieldConfig) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      <TextField label="Field" value={field.field} onChange={(value) => onChange({ ...field, field: value })} />
      <Field label="Type">
        <Select
          value={field.fieldType}
          onChange={(event) => onChange({ ...field, fieldType: event.target.value as FieldType })}
          className="h-9 w-full text-base md:text-sm"
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
      <FieldOptionEditors providerCatalog={providerCatalog} field={field} onChange={onChange} />
    </div>
  );
}

function FieldOptionEditors({
  providerCatalog,
  field,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  field: FieldConfig;
  onChange: (field: FieldConfig) => void;
}) {
  return (
    <>
      {fieldOptionKeys(field.fieldType).map((optionKey) => (
        <FieldOptionEditor
          key={optionKey}
          optionKey={optionKey}
          providerCatalog={providerCatalog}
          field={field}
          onChange={onChange}
        />
      ))}
    </>
  );
}

function FieldOptionEditor({
  optionKey,
  providerCatalog,
  field,
  onChange,
}: {
  optionKey: FieldOptionKey;
  providerCatalog?: ExternalProviderCatalog;
  field: FieldConfig;
  onChange: (field: FieldConfig) => void;
}) {
  const titleLanguages = useContext(TitleLanguagesContext);
  if (optionKey === "titleOptions") {
    return (
      <>
        <Field label="Title language">
          <LanguageSelect
            value={field.titleLanguage ?? ""}
            languages={titleLanguages}
            onChange={(titleLanguage) => onChange({ ...field, titleLanguage: titleLanguage || undefined })}
          />
        </Field>
        <Field label="Used as">
          <Select
            value={field.titleRole ?? ""}
            onChange={(event) =>
              onChange({ ...field, titleRole: (event.target.value || null) as FieldConfig["titleRole"] })
            }
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="">None</option>
            <option value="original">Original</option>
          </Select>
        </Field>
      </>
    );
  }

  if (optionKey === "enumOptions") {
    return (
      <div className="lg:col-span-3">
        <StringListEditor
          label="Enum options"
          values={field.enumOptions ?? []}
          placeholder="Completed"
          onChange={(enumOptions) => onChange({ ...field, enumOptions })}
        />
      </div>
    );
  }

  if (optionKey === "externalMappings") {
    return (
      <div className="lg:col-span-3">
        <ExternalFieldMappingsEditor
          providerCatalog={providerCatalog}
          values={field.externalFields ?? []}
          onChange={(externalFields) => onChange({ ...field, externalFields })}
        />
      </div>
    );
  }

  if (optionKey === "progressTotal") {
    return (
      <TextField
        label="Total progress field"
        value={field.totalProgressField ?? ""}
        onChange={(totalProgressField) => onChange({ ...field, totalProgressField })}
      />
    );
  }

  if (optionKey === "dateRole") {
    return (
      <Field label="Used as">
        <Select
          value={field.dateRole ?? ""}
          onChange={(event) =>
            onChange({ ...field, dateRole: (event.target.value || null) as FieldConfig["dateRole"] })
          }
          className="h-9 w-full text-base md:text-sm"
        >
          <option value="">None</option>
          <option value="planning">Planning</option>
          <option value="started">Started</option>
          <option value="completed">Completed</option>
        </Select>
      </Field>
    );
  }

  if (optionKey === "seasonLanguage") {
    return (
      <Field label="Season language">
        <Select
          value={field.seasonLanguage ?? "zh"}
          onChange={(event) => onChange({ ...field, seasonLanguage: event.target.value as SeasonLanguage })}
          className="h-9 w-full text-base md:text-sm"
        >
          <option value="zh">Chinese</option>
          <option value="ja">Japanese</option>
          <option value="en">English</option>
        </Select>
      </Field>
    );
  }

  if (optionKey === "externalRef") {
    return (
      <>
        <Field label="External source">
          <Select
            value={field.externalRef ?? ""}
            onChange={(event) =>
              onChange({
                ...field,
                externalRef: event.target.value,
                externalTypes: externalTypesForSource(providerCatalog, event.target.value),
              })
            }
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="">None</option>
            {externalSourceOptions(providerCatalog).map((option) => (
              <option key={option.source} value={option.source}>
                {option.label}
              </option>
            ))}
          </Select>
        </Field>
        <div className="lg:col-span-2">
          <ExternalTypesEditor
            source={field.externalRef ?? ""}
            providerCatalog={providerCatalog}
            values={field.externalTypes ?? []}
            onChange={(externalTypes) => onChange({ ...field, externalTypes })}
          />
        </div>
      </>
    );
  }

  if (optionKey === "relationType") {
    return (
      <TextField
        label="Relation type"
        value={field.relationType ?? ""}
        onChange={(relationType) => onChange({ ...field, relationType })}
      />
    );
  }

  return null;
}

function ExternalFieldMappingsEditor({
  providerCatalog,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  values: ExternalFieldMapping[];
  onChange: (values: ExternalFieldMapping[]) => void;
}) {
  const sourceOptions = externalSourceOptions(providerCatalog);
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">External field mappings</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => list.append({ source: sourceOptions[0]?.source ?? "", field: "" })}
          disabled={sourceOptions.length === 0}
        >
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => {
          const fieldOptions = externalFieldOptionsForSource(providerCatalog, value.source);
          return (
            <div key={index} className="grid grid-cols-[minmax(0,0.8fr)_minmax(0,1fr)_auto] gap-2">
              <Select
                value={value.source}
                onChange={(event) => {
                  const source = event.target.value;
                  const firstField = externalFieldOptionsForSource(providerCatalog, source)[0]?.field ?? "";
                  list.update(index, { source, field: firstField });
                }}
                aria-label="External source"
              >
                {sourceOptions.map((option) => (
                  <option key={option.source} value={option.source}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <Select
                value={value.field}
                onChange={(event) => list.update(index, { ...value, field: event.target.value })}
                aria-label="External field"
              >
                <option value="">Select field</option>
                {fieldOptions.map((option) => (
                  <option key={option.field} value={option.field}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <IconButton
                label="Remove external field mapping"
                onClick={() => list.remove(index)}
              />
            </div>
          );
        })}
        {values.length === 0 ? <EmptyConfigLine>No external mappings.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

function ExternalTypesEditor({
  source,
  providerCatalog,
  values,
  onChange,
}: {
  source: string;
  providerCatalog?: ExternalProviderCatalog;
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const options = externalTypeOptionsForSource(providerCatalog, source);
  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs font-medium text-muted-foreground">External types</span>
      {options.length > 0 ? (
        <MultiValueCombobox
          values={values}
          options={options.map((option) => ({ value: option.value, label: option.label }))}
          placeholder="Select type"
          ariaLabel="External types"
          onChange={onChange}
        />
      ) : (
        <EmptyConfigLine>No type options.</EmptyConfigLine>
      )}
    </div>
  );
}
