import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ArrowDownIcon, ArrowUpIcon, PlusIcon } from "lucide-react";

import { allTagsQuery } from "@/api/queries";
import { RuleBuilder, ruleFieldMetas } from "@/components/smart-lists/rule-builder";
import { titleLanguageLabel } from "@/lib/title-language";
import { Button } from "@/components/ui/button";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import {
  externalFieldOptionsForSource,
  externalSourceLabel,
  externalSourceOptions,
  externalTypeOptionsForSource,
  externalTypesForSource,
} from "@/lib/external-metadata";
import { fieldDisplayLabel, fieldTypeLabel, isDateFieldType } from "@/lib/type-config";
import type {
  BodySection,
  BodySectionKind,
  CanonicalStatus,
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalFieldMapping,
  ExternalProviderCatalog,
  FieldConfig,
  FieldType,
  HomeSectionConfig,
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
import { arrayEditor, externalRefProviderPriority } from "./settings-model";

export function DailyNotesEditor({
  config,
  vaultRoot,
  onChange,
}: {
  config: DailyNotesConfig;
  vaultRoot: string;
  onChange: (config: DailyNotesConfig) => void;
}) {
  const { t } = useLingui();
  const log = config.log ?? { section: "", lineFormat: "" };
  return (
    <div className="flex flex-col gap-3">
      <StringListEditor
        label={t`Paths`}
        values={config.paths ?? []}
        placeholder="Daily Notes"
        base={vaultRoot}
        pathItems
        onChange={(paths) => onChange({ ...config, paths })}
      />
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <TextField
          label={t`Date format`}
          value={config.dateFormat ?? ""}
          placeholder="YYYY-MM-DD"
          onChange={(dateFormat) => onChange({ ...config, dateFormat })}
        />
        <TextField
          label={t`New-note template`}
          value={config.template ?? ""}
          placeholder="Templates/Daily Note.md"
          onChange={(template) => onChange({ ...config, template })}
        />
      </div>
      <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
        <div className="text-xs text-muted-foreground">
          <Trans>
            Logging defaults — the heading log lines are written under, and the fallback line format.
            Each type can override these and adds its own tag.
          </Trans>
        </div>
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <TextField
            label={t`Log section`}
            value={log.section ?? ""}
            placeholder="Log"
            onChange={(section) => onChange({ ...config, log: { ...log, section } })}
          />
          <TextField
            label={t`Default line format`}
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
  tagsField,
  onChange,
}: {
  section: HomeSectionConfig;
  types: EntityTypeConfig[];
  tagsField: string;
  onChange: (section: HomeSectionConfig) => void;
}) {
  const { t } = useLingui();
  const selectedType = types.find((type) => type.id === section.type);
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  const fieldMetas = useMemo(
    () => ruleFieldMetas(selectedType ? [selectedType] : [], tagsField, allTags, t),
    [selectedType, tagsField, allTags, t],
  );
  const criteria = useMemo(
    () => section.criteria ?? { conjunction: "all" as const, rules: [] },
    [section.criteria],
  );
  const sortOptions = [
    { value: "title", label: t`Title` },
    { value: "recentlyUpdated", label: t`Update time` },
    { value: "relationCount", label: t`Relation count` },
    ...(selectedType?.fields ?? [])
      .filter((field) => isDateFieldType(field.fieldType))
      .map((field) => ({ value: `date:${field.field}`, label: t`Date: ${fieldDisplayLabel(field)}` })),
  ];
  const currentSort = section.sort ?? "title";

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      <TextField label={t`ID`} value={section.id} onChange={(id) => onChange({ ...section, id })} />
        <TextField label={t`Title`} value={section.title} onChange={(title) => onChange({ ...section, title })} />
        <Field label={t`Type`}>
          <Select
            value={section.type}
            onChange={(event) =>
              // Criteria reference the type's fields, so a type switch resets them.
              onChange({ ...section, type: event.target.value, criteria: null })
            }
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
            <div>
              <div className="text-sm font-medium"><Trans>Criteria</Trans></div>
              <div className="text-xs text-muted-foreground">
                <Trans>The same rules as smart lists; the section shows entries that match.</Trans>
              </div>
            </div>
            <RuleBuilder
              fieldMetas={fieldMetas}
              value={criteria}
              onChange={(next) => onChange({ ...section, criteria: next })}
            />
          </div>
        </div>
        <NumberField label={t`Limit`} value={section.limit} onChange={(limit) => onChange({ ...section, limit })} />
        <Field label={t`Sort`}>
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
        <Field label={t`Direction`}>
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
            <option value="">{t`Default`}</option>
            <option value="asc">{t`Ascending`}</option>
            <option value="desc">{t`Descending`}</option>
          </Select>
        </Field>
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
  const { t, i18n } = useLingui();
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="h-9 w-full text-base md:text-sm"
    >
      <option value="">{t`None`}</option>
      {languages.map((language) => (
        <option key={language.code} value={language.code}>
          {titleLanguageLabel(language.code, i18n.locale)} ({language.code})
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
  const { t } = useLingui();
  return (
      <div className="flex flex-col gap-4">
        <ConfigSubsection title={t`Basics`}>
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-4">
            <TextField label={t`ID`} value={config.id} onChange={(id) => onChange({ ...config, id })} />
            <TextField label={t`Label`} value={config.label} onChange={(label) => onChange({ ...config, label })} />
            <TextField label={t`Icon`} value={config.icon ?? ""} onChange={(icon) => onChange({ ...config, icon })} />
            <PathField
              label={t`Path`}
              value={config.path}
              base={taxonomyBase}
              suggestionBase={taxonomyRoot}
              onChange={(path) => onChange({ ...config, path })}
            />
          </div>
        </ConfigSubsection>

        <ConfigSubsection title={t`Providers`}>
          <ExternalPriorityEditor
            providerCatalog={providerCatalog}
            fields={config.fields}
            values={config.externalPriority ?? []}
            onChange={(externalPriority) => onChange({ ...config, externalPriority })}
          />
          <BodySectionsEditor
            providerCatalog={providerCatalog}
            values={config.bodySections ?? []}
            onChange={(bodySections) => onChange({ ...config, bodySections })}
          />
        </ConfigSubsection>

        <ConfigSubsection title={t`Filename`}>
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
            <Field label={t`Filename title language`}>
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
            <Field label={t`Filename title — used as`}>
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
                <option value="">{t`None`}</option>
                <option value="original">{t`Original (filename is the title)`}</option>
              </Select>
            </Field>
          </div>
        </ConfigSubsection>

        <ConfigSubsection title={t`Daily-note logging`}>
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
  const { t } = useLingui();
  const log = config.log ?? null;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs font-medium text-muted-foreground"><Trans>Log to daily note</Trans></div>
          <div className="text-xs text-muted-foreground">
            <Trans>
              When on, checking an episode or the Log button writes a line to the daily note. The title
              is auto-linked as a wikilink — just add your tag to the line format.
            </Trans>
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
            label={t`Line format`}
            value={log.lineFormat ?? ""}
            placeholder="- {title} {note} #Tag"
            onChange={(lineFormat) => onChange({ ...config, log: { ...log, lineFormat } })}
          />
          <TextField
            label={t`Section override`}
            value={log.section ?? ""}
            placeholder={t`(daily-notes default)`}
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
  fields,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  fields: FieldConfig[];
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  const ordered = externalRefProviderPriority(fields, values);

  function move(index: number, offset: -1 | 1) {
    const destination = index + offset;
    if (destination < 0 || destination >= ordered.length) return;
    const next = [...ordered];
    [next[index], next[destination]] = [next[destination], next[index]];
    onChange(next);
  }

  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs font-medium text-muted-foreground"><Trans>Provider priority</Trans></span>
      <div className="flex flex-col gap-2">
        {ordered.map((source, index) => {
          const label = externalSourceLabel(providerCatalog, source);
          return (
            <div key={source} className="flex items-center gap-2 rounded-md border px-3 py-2">
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{label}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled={index === 0}
                aria-label={t`Move ${label} up`}
                onClick={() => move(index, -1)}
              >
                <ArrowUpIcon />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled={index === ordered.length - 1}
                aria-label={t`Move ${label} down`}
                onClick={() => move(index, 1)}
              >
                <ArrowDownIcon />
              </Button>
            </div>
          );
        })}
        {ordered.length === 0 ? (
          <EmptyConfigLine><Trans>Add an External ref field with a provider to configure priority.</Trans></EmptyConfigLine>
        ) : null}
      </div>
    </div>
  );
}

type BodySectionTracking = NonNullable<BodySection["tracking"]>;

const EPISODE_TRACKING_OPTIONS: { value: BodySectionTracking; label: MessageDescriptor }[] = [
  { value: "checklist", label: msg`Checklist (per-item checkboxes)` },
  { value: "none", label: msg`None (plain list)` },
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
  const { t } = useLingui();
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground"><Trans>Markdown body sections</Trans></span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => list.append({ heading: t`Summary`, kind: "external", externalFields: [] })}
        >
          <PlusIcon data-icon="inline-start" />
          <Trans>Section</Trans>
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
        {values.length === 0 ? <EmptyConfigLine><Trans>No markdown body sections.</Trans></EmptyConfigLine> : null}
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
  const { t, i18n } = useLingui();
  return (
    <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
        <TextField
          label={t`Heading`}
          value={section.heading}
          onChange={(heading) => onChange({ ...section, heading })}
        />
        <Field label={t`Kind`}>
          <Select
            value={section.kind}
            onChange={(event) => onChange(changeBodySectionKind(section, event.target.value as BodySectionKind))}
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="external">{t`External metadata`}</option>
            <option value="episodes">{t`Item list`}</option>
            <UnknownValueOption value={section.kind} known={["external", "episodes"]} />
          </Select>
        </Field>
        <div className="flex items-end">
          <IconButton label={t`Remove body section`} onClick={onRemove} />
        </div>
      </div>
      {section.kind === "episodes" ? (
        <Field label={t`Tracking`}>
          <Select
            value={section.tracking ?? "checklist"}
            onChange={(event) => onChange({ ...section, tracking: event.target.value as BodySectionTracking })}
            className="h-9 w-full text-base md:text-sm"
          >
            {EPISODE_TRACKING_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {i18n._(option.label)}
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
  const { t } = useLingui();
  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      <TextField label={t`Field`} value={field.field} onChange={(value) => onChange({ ...field, field: value })} />
      <Field label={t`Type`}>
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
        label={t`Display name`}
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
  const { t } = useLingui();
  const titleLanguages = useContext(TitleLanguagesContext);
  if (optionKey === "titleOptions") {
    return (
      <>
        <Field label={t`Title language`}>
          <LanguageSelect
            value={field.titleLanguage ?? ""}
            languages={titleLanguages}
            onChange={(titleLanguage) => onChange({ ...field, titleLanguage: titleLanguage || undefined })}
          />
        </Field>
        <Field label={t`Used as`}>
          <Select
            value={field.titleRole ?? ""}
            onChange={(event) =>
              onChange({ ...field, titleRole: (event.target.value || null) as FieldConfig["titleRole"] })
            }
            className="h-9 w-full text-base md:text-sm"
          >
            <option value="">{t`None`}</option>
            <option value="original">{t`Original`}</option>
          </Select>
        </Field>
      </>
    );
  }

  if (optionKey === "enumOptions") {
    return (
      <div className="lg:col-span-3">
        <StringListEditor
          label={t`Enum options`}
          values={field.enumOptions ?? []}
          placeholder={t`Completed`}
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
        label={t`Total progress field`}
        value={field.totalProgressField ?? ""}
        onChange={(totalProgressField) => onChange({ ...field, totalProgressField })}
      />
    );
  }

  if (optionKey === "dateRole") {
    return (
      <Field label={t`Used as`}>
        <Select
          value={field.dateRole ?? ""}
          onChange={(event) =>
            onChange({ ...field, dateRole: (event.target.value || null) as FieldConfig["dateRole"] })
          }
          className="h-9 w-full text-base md:text-sm"
        >
          <option value="">{t`None`}</option>
          <option value="planning">{t`Planning`}</option>
          <option value="started">{t`Started`}</option>
          <option value="completed">{t`Completed`}</option>
          <option value="event">{t`Event`}</option>
        </Select>
      </Field>
    );
  }

  if (optionKey === "statusRole") {
    return <StatusRoleEditor field={field} onChange={onChange} />;
  }

  if (optionKey === "seasonLanguage") {
    return (
      <Field label={t`Season language`}>
        <Select
          value={field.seasonLanguage ?? "zh"}
          onChange={(event) => onChange({ ...field, seasonLanguage: event.target.value as SeasonLanguage })}
          className="h-9 w-full text-base md:text-sm"
        >
          <option value="zh">{t`Chinese`}</option>
          <option value="ja">{t`Japanese`}</option>
          <option value="en">{t`English`}</option>
        </Select>
      </Field>
    );
  }

  if (optionKey === "externalRef") {
    return (
      <>
        <Field label={t`External source`}>
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
            <option value="">{t`None`}</option>
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
        label={t`Relation type`}
        value={field.relationType ?? ""}
        onChange={(relationType) => onChange({ ...field, relationType })}
      />
    );
  }

  return null;
}

const STATUS_CANONICALS: { value: CanonicalStatus; label: MessageDescriptor }[] = [
  { value: "planning", label: msg`Planning` },
  { value: "ongoing", label: msg`Ongoing` },
  { value: "paused", label: msg`Paused` },
  { value: "completed", label: msg`Completed` },
  { value: "dropped", label: msg`Dropped` },
];

/// Marks an enum field as the type's status field and maps each option to a
/// canonical status. The mapping is edited per-option ("what does this option
/// mean?"), and rebuilt in `enumOptions` order — so the first option mapped to a
/// canonical is its write target when a log flips status.
function StatusRoleEditor({
  field,
  onChange,
}: {
  field: FieldConfig;
  onChange: (field: FieldConfig) => void;
}) {
  const { t, i18n } = useLingui();
  const isStatus = field.enumRole === "status";
  const options = field.enumOptions ?? [];

  const canonicalOf = new Map<string, CanonicalStatus>();
  for (const { value } of STATUS_CANONICALS) {
    for (const option of field.statusValues?.[value] ?? []) {
      canonicalOf.set(option, value);
    }
  }

  const setOptionCanonical = (option: string, canonical: CanonicalStatus | undefined) => {
    const next = new Map(canonicalOf);
    if (canonical) next.set(option, canonical);
    else next.delete(option);
    const values: Record<CanonicalStatus, string[]> = {
      planning: [],
      ongoing: [],
      paused: [],
      completed: [],
      dropped: [],
    };
    // Rebuild in enumOptions order so the first mapped option is the write target.
    for (const candidate of options) {
      const mapped = next.get(candidate);
      if (mapped) values[mapped].push(candidate);
    }
    const hasAny = STATUS_CANONICALS.some(({ value }) => values[value].length > 0);
    onChange({ ...field, statusValues: hasAny ? values : undefined });
  };

  return (
    <div className="lg:col-span-3 flex flex-col gap-2">
      <Field label={t`Used as`}>
        <Select
          value={field.enumRole ?? ""}
          onChange={(event) =>
            onChange({
              ...field,
              enumRole: event.target.value === "status" ? "status" : undefined,
              // Drop the mapping when the field is no longer a status field.
              statusValues: event.target.value === "status" ? field.statusValues : undefined,
            })
          }
          className="h-9 w-full text-base md:text-sm"
        >
          <option value="">{t`Regular enum`}</option>
          <option value="status">{t`Status field`}</option>
        </Select>
      </Field>
      {isStatus ? (
        options.length > 0 ? (
          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted-foreground">
              <Trans>Map each option to a status</Trans>
            </span>
            {options.map((option) => (
              <div key={option} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-sm">{option}</span>
                <Select
                  value={canonicalOf.get(option) ?? ""}
                  onChange={(event) =>
                    setOptionCanonical(option, (event.target.value || undefined) as CanonicalStatus | undefined)
                  }
                  className="h-9 w-40 text-base md:text-sm"
                >
                  <option value="">{t`Unmapped`}</option>
                  {STATUS_CANONICALS.map(({ value, label }) => (
                    <option key={value} value={value}>
                      {i18n._(label)}
                    </option>
                  ))}
                </Select>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            <Trans>Add enum options above to map them to statuses.</Trans>
          </p>
        )
      ) : null}
    </div>
  );
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
  const { t } = useLingui();
  const sourceOptions = externalSourceOptions(providerCatalog);
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground"><Trans>External field mappings</Trans></span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => list.append({ source: sourceOptions[0]?.source ?? "", field: "" })}
          disabled={sourceOptions.length === 0}
        >
          <PlusIcon data-icon="inline-start" />
          <Trans>Add</Trans>
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
                aria-label={t`External source`}
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
                aria-label={t`External field`}
              >
                <option value="">{t`Select field`}</option>
                {fieldOptions.map((option) => (
                  <option key={option.field} value={option.field}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <IconButton
                label={t`Remove external field mapping`}
                onClick={() => list.remove(index)}
              />
            </div>
          );
        })}
        {values.length === 0 ? <EmptyConfigLine><Trans>No external mappings.</Trans></EmptyConfigLine> : null}
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
  const { t } = useLingui();
  const options = externalTypeOptionsForSource(providerCatalog, source);
  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs font-medium text-muted-foreground"><Trans>External types</Trans></span>
      {options.length > 0 ? (
        <MultiValueCombobox
          values={values}
          options={options.map((option) => ({ value: option.value, label: option.label }))}
          placeholder={t`Select type`}
          ariaLabel={t`External types`}
          onChange={onChange}
        />
      ) : (
        <EmptyConfigLine><Trans>No type options.</Trans></EmptyConfigLine>
      )}
    </div>
  );
}
