//! The provider-mapping editors shared by the type form (which maps a whole
//! type onto external types) and the field form (which maps one field onto a
//! provider's fields).
//!
//! Unknown provider values are preserved rather than dropped: a mapping written
//! by hand, or one whose provider catalog is momentarily unavailable, still
//! shows and still saves.

import { Trans, useLingui } from "@lingui/react/macro";
import { PlusIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import {
  externalFieldOptionsForSource,
  externalSourceOptions,
  externalTypeOptionsForSource,
} from "@/lib/external-metadata";
import type { ExternalFieldMapping, ExternalProviderCatalog } from "@/types/api";

import { EmptyConfigLine, IconButton } from "./settings-controls";
import { arrayEditor } from "./settings-model";

export function ExternalFieldMappingsEditor({
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
                aria-label={t`Provider`}
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

export function ExternalTypesEditor({
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
