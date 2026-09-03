//! Dispatch from a field's declared type to the editor that can edit it.
//!
//! The `fieldType` in the schema decides which editor appears — never the
//! field's name — so a `title` field and a field called `title_jp` are only
//! treated alike if the schema says they are the same type.

import { useLingui } from "@lingui/react/macro";

import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";

import {
  listDisplayValues,
  numberOrString,
  toWikilink,
  valueToText,
  withCurrentOption,
} from "./frontmatter-utils";
import { ImageFieldInput } from "./metadata-image-input";
import { MultiValueInput, SeasonListInput } from "./metadata-list-inputs";
import { DatePickerInput, NumberStepper, ObjectValueInput } from "./metadata-scalar-inputs";
import type { EditableFieldSpec, FrontmatterValue } from "./metadata-types";

export function FieldValueInput({
  field,
  value,
  disabled,
  entityId,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onChange: (value: FrontmatterValue) => void;
}) {
  const { t } = useLingui();

  if (field.kind === "image" || field.kind === "imageList") {
    return (
      <ImageFieldInput
        field={field}
        value={value}
        disabled={disabled}
        entityId={entityId}
        onChange={onChange}
      />
    );
  }

  if (field.kind === "select") {
    const options = withCurrentOption(field.options, valueToText(value));
    return (
      <Select
        value={valueToText(value)}
        onChange={(event) => onChange(event.target.value || null)}
        className="w-full"
        aria-label={field.label}
        disabled={disabled}
      >
        <option value="">{t`Empty`}</option>
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (field.kind === "number") {
    return (
      <NumberStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
        disabled={disabled}
      />
    );
  }

  if (field.kind === "progress") {
    return (
      <NumberStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
        disabled={disabled}
        display="output"
      />
    );
  }

  if (field.kind === "date") {
    return (
      <DatePickerInput
        value={valueToText(value)}
        onChange={(next) => onChange(next || null)}
        ariaLabel={field.label}
        disabled={disabled}
      />
    );
  }

  if (field.kind === "boolean") {
    return (
      <Select
        value={value === true ? "true" : value === false ? "false" : ""}
        onChange={(event) =>
          onChange(event.target.value === "" ? null : event.target.value === "true")
        }
        className="w-full"
        aria-label={field.label}
        disabled={disabled}
      >
        <option value="">{t`Empty`}</option>
        <option value="true">{t`Yes`}</option>
        <option value="false">{t`No`}</option>
      </Select>
    );
  }

  if (field.kind === "list" || field.kind === "relation") {
    const relation = field.kind === "relation";
    const relationType = (field.relationType ?? "").trim();
    return (
      <MultiValueInput
        values={listDisplayValues(value, relation)}
        options={
          relation ? field.relationOptions : field.options.map((option) => ({ value: option }))
        }
        loadOptions={relation ? field.loadRelationOptions : undefined}
        placeholder={
          relation
            ? relationType
              ? t`Search or add ${relationType}`
              : t`Search or add relation`
            : t`Add value`
        }
        ariaLabel={field.label}
        wikilinks={relation}
        disabled={disabled}
        onChange={(values) => onChange(relation ? values.map(toWikilink) : values)}
      />
    );
  }

  if (field.kind === "season") {
    return (
      <SeasonListInput
        values={listDisplayValues(value, false)}
        language={field.seasonLanguage}
        ariaLabel={field.label}
        disabled={disabled}
        onChange={onChange}
      />
    );
  }

  if (field.kind === "object") {
    return (
      <ObjectValueInput
        value={value}
        onChange={onChange}
        disabled={disabled}
        ariaLabel={field.label}
      />
    );
  }

  return (
    <Input
      value={valueToText(value)}
      onChange={(event) => onChange(event.target.value || null)}
      list={field.options.length > 0 ? `${field.key}-options` : undefined}
      aria-label={field.label}
      disabled={disabled}
    />
  );
}
