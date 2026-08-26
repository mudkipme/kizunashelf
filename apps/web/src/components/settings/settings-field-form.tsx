//! The editor for one field of a type: the shape shared by every field, plus the
//! per-`fieldType` options — enum values and their canonical-status mapping,
//! relation targets, title language, and external-provider mappings.
//!
//! Which options appear is driven entirely by the declared `fieldType`, never by
//! the field's name.

import { useContext } from "react";
import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";

import { Select } from "@/components/ui/select";
import {
  externalSourceOptions,
  externalTypesForSource,
} from "@/lib/external-metadata";
import { fieldTypeLabel } from "@/lib/type-config";
import type {
  CanonicalStatus,
  ExternalProviderCatalog,
  FieldConfig,
  FieldType,
  SeasonLanguage,
} from "@/types/api";

import {
  Field,
  StringListEditor,
  TextField,
} from "./settings-controls";
import {
  fieldOptionKeys,
  fieldTypeOptions,
  type FieldOptionKey,
} from "./settings-field-descriptors";
import {
  ExternalFieldMappingsEditor,
  ExternalTypesEditor,
} from "./settings-external-mappings";
import { LanguageSelect, TitleLanguagesContext } from "./settings-shared";

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
          className="w-full"
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
            className="w-full"
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
          className="w-full"
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
          className="w-full"
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
        <Field label={t`Provider`}>
          <Select
            value={field.externalRef ?? ""}
            onChange={(event) =>
              onChange({
                ...field,
                externalRef: event.target.value,
                externalTypes: externalTypesForSource(providerCatalog, event.target.value),
              })
            }
            className="w-full"
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
          className="w-full"
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
                  className="w-40"
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
