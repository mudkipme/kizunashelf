import { PlusIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { externalFieldOptionsBySource, externalSourceOptions } from "@/lib/external-metadata";
import {
  fieldTypeLabel,
  supportsDateRole,
  supportsEnumOptions,
  supportsTitleOptions,
} from "@/lib/type-config";
import type {
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalFieldMapping,
  FieldConfig,
  FieldType,
  HomeConfig,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  SeasonLanguage,
} from "@/types/config";

import {
  EmptyConfigLine,
  Field,
  IconButton,
  NumberField,
  PathField,
  StringListEditor,
  TextField,
} from "./settings-controls";
import {
  defaultField,
  defaultHomeSection,
  externalTypeOptionsBySource,
  externalTypesForSource,
  replaceArray,
  replaceAt,
} from "./settings-model";

export function DailyNotesEditor({
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

export function HomeEditor({
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
  const selectedType = types.find((type) => type.id === section.type);
  const filterFields = selectedType?.fields.filter((field) => supportsEnumOptions(field.fieldType)) ?? [];
  const filters = section.filters ?? [];

  function updateFilter(index: number, filter: HomeSectionFilterConfig) {
    onChange({ ...section, filters: replaceArray(filters, index, filter) });
  }

  function removeFilter(index: number) {
    onChange({ ...section, filters: filters.filter((_, itemIndex) => itemIndex !== index) });
  }

  function addFilter() {
    const field = filterFields[0]?.field ?? "";
    onChange({ ...section, filters: [...filters, { field, values: [] }] });
  }

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
          <Select
            value={section.type}
            onChange={(event) => onChange({ ...section, type: event.target.value, filters: [] })}
            className="h-9 w-full text-sm"
          >
            {types.map((type) => (
              <option key={type.id} value={type.id}>
                {type.label || type.id}
              </option>
            ))}
            {!types.some((type) => type.id === section.type) ? <option value={section.type}>{section.type}</option> : null}
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
                key={`${filter.field}-${index}`}
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
            className="h-9 w-full text-sm"
          >
            {fields.map((field) => (
              <option key={field.field} value={field.field}>
                {field.displayName || field.field}
              </option>
            ))}
            {filter.field && !fields.some((field) => field.field === filter.field) ? (
              <option value={filter.field}>{filter.field}</option>
            ) : null}
          </Select>
        </Field>
        <StringListEditor
          label="Values"
          values={filter.values}
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

export function EntityTypeEditor({
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
                filename: {
                  ...(config.filename ?? {}),
                  titleLanguage: event.target.value || undefined,
                  defaultTitle: config.filename?.defaultTitle ?? false,
                },
              })
            }
            className="h-9 w-full text-sm"
          >
            <option value="">None</option>
            <option value="zh">Chinese</option>
            <option value="ja">Japanese</option>
            <option value="en">English</option>
          </Select>
        </Field>
        <Field label="Filename default title">
          <Select
            value={config.filename?.defaultTitle ? "true" : "false"}
            onChange={(event) =>
              onChange({
                ...config,
                filename: {
                  ...(config.filename ?? {}),
                  defaultTitle: event.target.value === "true",
                },
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
            <Field label="Title role">
              <Select
                value={field.titleRole ?? ""}
                onChange={(event) => onChange({ ...field, titleRole: (event.target.value || null) as FieldConfig["titleRole"] })}
                className="h-9 w-full text-sm"
              >
                <option value="">None</option>
                <option value="original">Original</option>
              </Select>
            </Field>
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
          <div className="lg:col-span-3">
            <StringListEditor
              label="Enum options"
              values={field.enumOptions ?? []}
              placeholder="Completed"
              onChange={(enumOptions) => onChange({ ...field, enumOptions })}
            />
          </div>
        ) : null}
        {field.fieldType !== "externalRef" ? (
          <div className="lg:col-span-3">
            <ExternalFieldMappingsEditor
              values={field.externalFields ?? []}
              onChange={(externalFields) => onChange({ ...field, externalFields })}
            />
          </div>
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
          <>
            <Field label="External source">
              <Select
                value={field.externalRef ?? ""}
                onChange={(event) =>
                  onChange({
                    ...field,
                    externalRef: event.target.value,
                    externalTypes: externalTypesForSource(event.target.value, ""),
                  })
                }
                className="h-9 w-full text-sm"
              >
                <option value="">None</option>
                {externalSourceOptions.map((option) => (
                  <option key={option.source} value={option.source}>
                    {option.label}
                  </option>
                ))}
              </Select>
            </Field>
            <div className="lg:col-span-2">
              <ExternalTypesEditor
                source={field.externalRef ?? ""}
                values={field.externalTypes ?? []}
                onChange={(externalTypes) => onChange({ ...field, externalTypes })}
              />
            </div>
          </>
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

function ExternalFieldMappingsEditor({
  values,
  onChange,
}: {
  values: ExternalFieldMapping[];
  onChange: (values: ExternalFieldMapping[]) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">External field mappings</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => onChange([...values, { source: externalSourceOptions[0]?.source ?? "bangumi", field: "" }])}
        >
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => {
          const fieldOptions = externalFieldOptionsBySource[value.source] ?? [];
          return (
            <div key={index} className="grid grid-cols-[minmax(0,0.8fr)_minmax(0,1fr)_auto] gap-2">
              <Select
                value={value.source}
                onChange={(event) => {
                  const source = event.target.value;
                  const firstField = externalFieldOptionsBySource[source]?.[0]?.field ?? "";
                  onChange(replaceArray(values, index, { source, field: firstField }));
                }}
                aria-label="External source"
              >
                {externalSourceOptions.map((option) => (
                  <option key={option.source} value={option.source}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <Select
                value={value.field}
                onChange={(event) => onChange(replaceArray(values, index, { ...value, field: event.target.value }))}
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
                onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
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
  values,
  onChange,
}: {
  source: string;
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const options = externalTypeOptionsBySource[source] ?? [];
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">External types</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={options.length === 0}
          onClick={() => onChange([...values, options[0]?.value ?? ""])}
        >
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => (
          <div key={index} className="flex items-center gap-2">
            <Select
              value={value}
              onChange={(event) => onChange(replaceArray(values, index, event.target.value))}
              aria-label="External type"
            >
              {options.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </Select>
            <IconButton
              label="Remove external type"
              onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
            />
          </div>
        ))}
        {values.length === 0 ? <EmptyConfigLine>No type filter.</EmptyConfigLine> : null}
      </div>
    </div>
  );
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
  "bool",
  "season",
  "date",
  "externalRef",
  "relation",
  "text",
  "textList",
];
