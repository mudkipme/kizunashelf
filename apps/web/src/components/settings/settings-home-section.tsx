//! The editor for one Home-screen section.

import { useMemo } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";

import { allTagsQuery } from "@/api/queries";
import { RuleBuilder } from "@/components/smart-lists/rule-builder";
import { ruleFieldMetas } from "@/components/smart-lists/rule-field-meta";
import { Select } from "@/components/ui/select";
import { fieldDisplayLabel, isDateFieldType } from "@/lib/type-config";
import type { EntityTypeConfig, HomeSectionConfig } from "@/types/api";

import { Field, NumberField, TextField, UnknownValueOption } from "./settings-controls";

export function HomeSectionForm({
  section,
  types,
  tagsField,
  onChange,
}: {
  section: HomeSectionConfig;
  types: EntityTypeConfig[];
  tagsField: string | undefined;
  onChange: (section: HomeSectionConfig) => void;
}) {
  const { t } = useLingui();
  const selectedType = types.find((type) => type.id === section.type);
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  const fieldMetas = useMemo(
    () => ruleFieldMetas(selectedType, tagsField, allTags, t),
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
            className="w-full"
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
                <Trans>The same rules as smart lists; the section shows entities that match.</Trans>
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
            className="w-full"
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
            className="w-full"
          >
            <option value="">{t`Default`}</option>
            <option value="asc">{t`Ascending`}</option>
            <option value="desc">{t`Descending`}</option>
          </Select>
        </Field>
    </div>
  );
}
